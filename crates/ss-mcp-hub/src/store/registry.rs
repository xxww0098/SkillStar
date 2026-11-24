use crate::error::{McpHubError, McpHubResult};
use crate::formats::paste::{derive_name_from_cmd, sanitize_slug, split_shell_words};
use crate::model::{McpServer, StoreItem, Transport};
use crate::store::snapshot::{RawMarketServer, convert_raw_server, is_valid_pypi_name};
use regex::Regex;
use serde::Deserialize;
use ss_core::infra::http_client::probe_http_client;
use std::sync::LazyLock;
use std::time::Duration;

const REGISTRY_SEARCH_URL: &str = "https://registry.modelcontextprotocol.io/v0/servers";
const GITHUB_RAW_BASE_URL: &str = "https://raw.githubusercontent.com";
const PYPI_BASE_URL: &str = "https://pypi.org/pypi";
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);

static GITHUB_REPO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:https?://)?github\.com/([^/]+)/([^/#?]+)").unwrap());
static OWNER_REPO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z0-9][A-Za-z0-9._-]*)/([^/#?\s]+)$").unwrap());
static PYPROJECT_NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^\s*name\s*=\s*["']([^"']+)["']"#).unwrap());
static PYPROJECT_SCRIPT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?ms)\[project\.scripts\](.*?)(?:\n\[|\z)").unwrap());

/// Result of querying the live MCP Registry and fallback address probes.
#[derive(Debug, Default)]
pub struct OnlineSearchOutcome {
    pub items: Vec<StoreItem>,
    pub online_used: bool,
    pub online_error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RegistryListResponse {
    #[serde(default)]
    servers: Vec<RegistryEnvelope>,
}

#[derive(Debug, Deserialize)]
struct RegistryEnvelope {
    server: RawMarketServer,
}

#[derive(Debug, Deserialize)]
struct PypiInfoResponse {
    info: Option<PypiInfo>,
}

#[derive(Debug, Deserialize)]
struct PypiInfo {
    name: Option<String>,
    summary: Option<String>,
    home_page: Option<String>,
}

/// Query the official MCP Registry and resolve direct-address / command fallbacks.
pub async fn search_online(query: &str, limit: usize) -> McpHubResult<Vec<StoreItem>> {
    let outcome = search_online_outcome(query, limit).await?;
    if outcome.items.is_empty()
        && let Some(err) = outcome.online_error
    {
        return Err(McpHubError::Store(err));
    }
    Ok(outcome.items)
}

/// Query the official MCP Registry and preserve fallback resolution (`resolve_direct_address`
/// and offline command/URL parsing) even when the Registry errors or returns non-2xx.
pub async fn search_online_outcome(
    query: &str,
    limit: usize,
) -> McpHubResult<OnlineSearchOutcome> {
    let client = probe_http_client(HTTP_TIMEOUT).map_err(McpHubError::Other)?;
    Ok(search_online_with(
        &client,
        REGISTRY_SEARCH_URL,
        GITHUB_RAW_BASE_URL,
        PYPI_BASE_URL,
        query,
        limit,
    )
    .await)
}

async fn search_online_with(
    client: &reqwest::Client,
    registry_url: &str,
    github_raw_base: &str,
    pypi_base: &str,
    query: &str,
    limit: usize,
) -> OnlineSearchOutcome {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return OnlineSearchOutcome::default();
    }

    let mut items = Vec::new();
    let mut online_used = false;
    let mut online_error = None;

    // 1. Official MCP Registry search endpoint
    match client
        .get(registry_url)
        .query(&[("search", trimmed), ("limit", &limit.min(30).to_string())])
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.json::<RegistryListResponse>().await {
            Ok(body) => {
                online_used = true;
                for env in body.servers {
                    if let Some(item) = convert_raw_server(&env.server, "registry") {
                        items.push(item);
                    }
                }
            }
            Err(e) => {
                online_error = Some(format!("Failed parsing MCP Registry response: {e}"));
            }
        },
        Ok(resp) => {
            online_error = Some(format!("MCP Registry returned HTTP {}", resp.status()));
        }
        Err(e) => {
            online_error = Some(format!("MCP Registry unavailable: {e}"));
        }
    }

    // 2. Direct address / command / GitHub / PyPI lookup whenever Registry had no match
    //    OR errored (do not gate fallback on `Registry 200 && items.is_empty()`).
    if items.is_empty()
        && let Some((extra, probed_network)) =
            resolve_direct_address_with_bases(client, github_raw_base, pypi_base, trimmed).await
    {
        if probed_network {
            online_used = true;
        }
        items.push(extra);
    }

    OnlineSearchOutcome {
        items,
        online_used,
        online_error,
    }
}

/// Offline-capable direct recognition for `npx ...`, `uvx ...`, `npm:<pkg>`, and non-GitHub URLs.
pub(crate) fn resolve_offline_direct_address(query: &str) -> Option<StoreItem> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Direct `npx ...` or `uvx ...` invocation
    if let Some(item) = parse_cli_runner_item(trimmed) {
        return Some(item);
    }

    // 2. Explicit `npm:<package>` prefix
    if trimmed.len() > 4 && trimmed[..4].eq_ignore_ascii_case("npm:") {
        let pkg = trimmed[4..].trim();
        if !pkg.is_empty() && !pkg.contains(char::is_whitespace) && !pkg.starts_with('-') {
            let args = vec!["-y".to_string(), pkg.to_string()];
            let slug = derive_name_from_cmd("npx", &args);
            return Some(StoreItem {
                id: format!("npm:{pkg}"),
                name: slug.clone(),
                title: pkg.to_string(),
                description: format!("npm MCP package {pkg} via npx"),
                publisher: "npm".into(),
                icon: String::new(),
                homepage: format!("https://www.npmjs.com/package/{pkg}"),
                transport: Transport::Stdio,
                runs: "npx".into(),
                sign_in: false,
                featured: false,
                match_by_addr: true,
                source: "npm".into(),
                stars: None,
                have: None,
                inputs: vec![],
                template: McpServer {
                    name: slug,
                    transport: Transport::Stdio,
                    command: "npx".into(),
                    args,
                    ..Default::default()
                },
            });
        }
    }

    // 3. Direct HTTPS/HTTP MCP endpoint (`custom_at`) for non-GitHub URLs
    if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && !trimmed.contains(char::is_whitespace)
        && let Ok(parsed) = url::Url::parse(trimmed)
        && !parsed
            .host_str()
            .unwrap_or("")
            .eq_ignore_ascii_case("github.com")
    {
        let host = parsed.host_str().unwrap_or("mcp-server");
        let slug = sanitize_slug(host.split('.').next().unwrap_or("mcp-server"));
        return Some(StoreItem {
            id: format!("custom:{slug}"),
            name: slug.clone(),
            title: host.to_string(),
            description: format!("Custom remote MCP endpoint at {trimmed}"),
            publisher: host.to_string(),
            icon: String::new(),
            homepage: trimmed.to_string(),
            transport: Transport::Http,
            runs: String::new(),
            sign_in: false,
            featured: false,
            match_by_addr: true,
            source: "custom_at".into(),
            stars: None,
            have: None,
            inputs: vec![],
            template: McpServer {
                name: slug,
                transport: Transport::Http,
                url: trimmed.to_string(),
                ..Default::default()
            },
        });
    }

    None
}

fn parse_cli_runner_item(trimmed: &str) -> Option<StoreItem> {
    let words = split_shell_words(trimmed).ok()?;
    let cmd = words.first()?.as_str();
    let runner = if cmd.eq_ignore_ascii_case("npx") || cmd.eq_ignore_ascii_case("npx.cmd") {
        "npx"
    } else if cmd.eq_ignore_ascii_case("uvx") || cmd.eq_ignore_ascii_case("uvx.exe") {
        "uvx"
    } else {
        return None;
    };
    if words.len() < 2 || !words[1..].iter().any(|a| !a.starts_with('-')) {
        return None;
    }

    let mut args = words[1..].to_vec();
    if runner == "npx" && !args.iter().any(|a| a == "-y" || a == "--yes") {
        args.insert(0, "-y".to_string());
    }
    let pkg_display = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| runner.to_string());
    let slug = derive_name_from_cmd(runner, &args);
    Some(StoreItem {
        id: format!("{runner}:{slug}"),
        name: slug.clone(),
        title: pkg_display,
        description: format!("Run `{trimmed}` directly via {runner}"),
        publisher: runner.into(),
        icon: String::new(),
        homepage: String::new(),
        transport: Transport::Stdio,
        runs: runner.into(),
        sign_in: false,
        featured: false,
        match_by_addr: true,
        source: runner.into(),
        stars: None,
        have: None,
        inputs: vec![],
        template: McpServer {
            name: slug,
            transport: Transport::Stdio,
            command: runner.into(),
            args,
            ..Default::default()
        },
    })
}

async fn resolve_direct_address_with_bases(
    client: &reqwest::Client,
    github_raw_base: &str,
    pypi_base: &str,
    query: &str,
) -> Option<(StoreItem, bool)> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(item) = resolve_offline_direct_address(trimmed) {
        return Some((item, false));
    }

    // Case A: GitHub repository (`github.com/owner/repo` or pure `owner/repo`)
    if !trimmed.contains(char::is_whitespace)
        && let Some((owner, repo)) = extract_github_owner_repo(trimmed)
    {
        for branch in ["main", "master"] {
            let raw_url = format!("{github_raw_base}/{owner}/{repo}/{branch}/pyproject.toml");
            if let Ok(resp) = client.get(&raw_url).send().await
                && resp.status().is_success()
                && let Ok(text) = resp.text().await
                && let Some(item) = build_item_from_pyproject(owner, repo, &text)
            {
                return Some((item, true));
            }
        }
        return None;
    }

    // Case C: Bare PyPI package lookup (`mcp-server-fetch`, no `.` or `/`)
    if is_valid_pypi_name(trimmed) {
        let pkg_lower = trimmed.to_lowercase();
        let pypi_url = format!("{pypi_base}/{pkg_lower}/json");
        if let Ok(resp) = client.get(&pypi_url).send().await
            && resp.status().is_success()
            && let Ok(parsed) = resp.json::<PypiInfoResponse>().await
            && let Some(info) = parsed.info
            && let Some(pkg_name) = info.name
        {
            let slug = sanitize_slug(&pkg_name);
            return Some((
                StoreItem {
                    id: format!("pypi:{pkg_name}"),
                    name: slug.clone(),
                    title: pkg_name.clone(),
                    description: info.summary.unwrap_or_default(),
                    publisher: "PyPI".into(),
                    icon: String::new(),
                    homepage: info.home_page.unwrap_or_default(),
                    transport: Transport::Stdio,
                    runs: "uvx".into(),
                    sign_in: false,
                    featured: false,
                    match_by_addr: true,
                    source: "pypi".into(),
                    stars: None,
                    have: None,
                    inputs: vec![],
                    template: McpServer {
                        name: slug,
                        transport: Transport::Stdio,
                        command: "uvx".into(),
                        args: vec![pkg_name],
                        ..Default::default()
                    },
                },
                true,
            ));
        }
    }

    None
}

fn extract_github_owner_repo(trimmed: &str) -> Option<(&str, &str)> {
    if let Some(caps) = GITHUB_REPO_RE.captures(trimmed) {
        let owner = caps.get(1)?.as_str();
        let repo = caps.get(2)?.as_str().trim_end_matches(".git");
        if !owner.is_empty() && !repo.is_empty() {
            return Some((owner, repo));
        }
    }
    if let Some(caps) = OWNER_REPO_RE.captures(trimmed) {
        let owner = caps.get(1)?.as_str();
        let repo = caps.get(2)?.as_str().trim_end_matches(".git");
        if !owner.is_empty() && !repo.is_empty() {
            return Some((owner, repo));
        }
    }
    None
}

fn build_item_from_pyproject(owner: &str, repo: &str, toml_text: &str) -> Option<StoreItem> {
    let pkg_name = PYPROJECT_NAME_RE
        .captures(toml_text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| repo.to_string());
    let has_scripts = PYPROJECT_SCRIPT_RE.is_match(toml_text);
    if !has_scripts && !toml_text.to_lowercase().contains("mcp") {
        return None;
    }
    let slug = sanitize_slug(&pkg_name);
    let url = format!("https://github.com/{owner}/{repo}");
    Some(StoreItem {
        id: format!("github:{owner}/{repo}"),
        name: slug.clone(),
        title: repo.to_string(),
        description: format!("Python MCP package from {owner}/{repo} via uvx"),
        publisher: owner.to_string(),
        icon: format!("https://github.com/{owner}.png?size=96"),
        homepage: url,
        transport: Transport::Stdio,
        runs: "uvx".into(),
        sign_in: false,
        featured: false,
        match_by_addr: true,
        source: "github".into(),
        stars: None,
        have: None,
        inputs: vec![],
        template: McpServer {
            name: slug,
            transport: Transport::Stdio,
            command: "uvx".into(),
            args: vec![pkg_name],
            ..Default::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_mock_http<F>(handler: F) -> String
    where
        F: Fn(&str) -> (u16, &'static str, String) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handler = std::sync::Arc::new(handler);
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let handler = std::sync::Arc::clone(&handler);
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req.lines().next().unwrap_or("");
                    let path = first_line.split_whitespace().nth(1).unwrap_or("/");
                    let (status, ct, body) = handler(path);
                    let resp = format!(
                        "HTTP/1.1 {status} OK\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    fn no_proxy_client() -> reqwest::Client {
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
    }

    #[test]
    fn offline_direct_address_parses_npm_npx_uvx_and_custom_url() {
        let npm_scoped = resolve_offline_direct_address("npm:@org/mcp-tool").unwrap();
        assert_eq!(npm_scoped.source, "npm");
        assert_eq!(npm_scoped.template.command, "npx");
        assert_eq!(npm_scoped.template.args, vec!["-y", "@org/mcp-tool"]);

        let npx = resolve_offline_direct_address("npx @org/tool \"/my dir\"").unwrap();
        assert_eq!(npx.source, "npx");
        assert_eq!(npx.template.command, "npx");
        assert_eq!(npx.template.args, vec!["-y", "@org/tool", "/my dir"]);

        let uvx = resolve_offline_direct_address("uvx mcp-server-git --repository /tmp").unwrap();
        assert_eq!(uvx.source, "uvx");
        assert_eq!(uvx.template.command, "uvx");
        assert_eq!(
            uvx.template.args,
            vec!["mcp-server-git", "--repository", "/tmp"]
        );

        let custom = resolve_offline_direct_address("https://mcp.example.com/mcp").unwrap();
        assert_eq!(custom.source, "custom_at");
        assert_eq!(custom.template.url, "https://mcp.example.com/mcp");
    }

    #[tokio::test]
    async fn pypi_bare_name_and_owner_repo_and_registry_error_fallback_work() {
        let base = spawn_mock_http(|path| {
            if path.starts_with("/registry") {
                return (503, "text/plain", "upstream unavailable".into());
            }
            if path == "/pypi/mcp-server-fetch/json" {
                return (
                    200,
                    "application/json",
                    r#"{"info":{"name":"mcp-server-fetch","summary":"Fetch MCP","home_page":"https://pypi.org/p/mcp-server-fetch"}}"#.into(),
                );
            }
            if path == "/github/octocat/mcp-demo/main/pyproject.toml" {
                return (
                    200,
                    "text/plain",
                    "[project]\nname = \"mcp-demo\"\n\n[project.scripts]\nmcp-demo = \"mcp_demo:main\"\n".into(),
                );
            }
            (404, "text/plain", "not found".into())
        })
        .await;

        let client = no_proxy_client();
        let registry_url = format!("{base}/registry");
        let gh_base = format!("{base}/github");
        let pypi_base = format!("{base}/pypi");

        // Bare PyPI name (`mcp-server-fetch`) must reach PyPI even when Registry returns 503.
        let pypi_res = search_online_with(
            &client,
            &registry_url,
            &gh_base,
            &pypi_base,
            "mcp-server-fetch",
            10,
        )
        .await;
        assert!(pypi_res.online_error.is_some());
        assert!(pypi_res.online_used);
        assert_eq!(pypi_res.items.len(), 1);
        assert_eq!(pypi_res.items[0].source, "pypi");
        assert_eq!(pypi_res.items[0].template.args, vec!["mcp-server-fetch"]);

        // Pure `owner/repo` resolves GitHub pyproject.toml
        let gh_res = search_online_with(
            &client,
            &registry_url,
            &gh_base,
            &pypi_base,
            "octocat/mcp-demo",
            10,
        )
        .await;
        assert_eq!(gh_res.items.len(), 1);
        assert_eq!(gh_res.items[0].id, "github:octocat/mcp-demo");
        assert_eq!(gh_res.items[0].template.args, vec!["mcp-demo"]);

        // Scoped npm name must NOT be treated as GitHub owner/repo
        assert!(extract_github_owner_repo("@modelcontextprotocol/server-fetch").is_none());

        // Manual URL still resolves `custom_at` when Registry fails
        let custom_res = search_online_with(
            &client,
            &registry_url,
            &gh_base,
            &pypi_base,
            "https://api.custom-mcp.example/mcp",
            10,
        )
        .await;
        assert!(custom_res.online_error.is_some());
        assert_eq!(custom_res.items.len(), 1);
        assert_eq!(custom_res.items[0].source, "custom_at");
    }
}

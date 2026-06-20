//! Data model for the MCP server marketplace, backed by the **GitHub MCP
//! Registry** (`https://api.mcp.github.com/v0/servers`, following the
//! `modelcontextprotocol/registry` v0.1 `server.json` schema).
//!
//! These types are the local-snapshot + IPC representation. They intentionally
//! use `camelCase` serde (matching the MCP domain's `McpServerEntry`) so the
//! frontend MCP feature sees one consistent naming convention.
//!
//! The snapshot also keeps the **raw `server` JSON** for each entry
//! (`raw_server_json`) so the app layer can reconstruct a full install spec
//! (`McpServerEntry`) without re-fetching — see `registry_to_entry` in
//! `skillstar-app`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Whether a registry server can run locally (stdio package), remotely
/// (http/sse url), or both.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum McpServerKind {
    Stdio,
    Remote,
    Both,
    Unknown,
}

impl McpServerKind {
    /// Stable string used for DB storage (matches the camelCase serde tag).
    pub fn as_db_str(&self) -> &'static str {
        match self {
            McpServerKind::Stdio => "stdio",
            McpServerKind::Remote => "remote",
            McpServerKind::Both => "both",
            McpServerKind::Unknown => "unknown",
        }
    }

    pub fn from_db_str(value: &str) -> Self {
        match value {
            "stdio" => McpServerKind::Stdio,
            "remote" => McpServerKind::Remote,
            "both" => McpServerKind::Both,
            _ => McpServerKind::Unknown,
        }
    }
}

/// Summary of a runnable package (stdio transport) for card/detail display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryPackageSummary {
    /// Runner command we'd use: `npx` / `uvx` / `docker` / `dnx` / …
    pub runtime: String,
    /// Package identifier (npm/pypi/oci name).
    pub identifier: String,
    pub version: Option<String>,
    /// Names of env vars the user must supply (required or secret).
    #[serde(default)]
    pub required_env: Vec<String>,
}

/// Summary of a remote endpoint (http/sse transport) for card/detail display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryRemoteSummary {
    /// Normalized transport: `http` or `sse`.
    pub transport: String,
    pub url: String,
    /// Names of headers the user must supply (required or secret).
    #[serde(default)]
    pub required_headers: Vec<String>,
}

/// A normalized registry server stored in the local snapshot. Carries card
/// metadata, display summaries, readme, and the raw `server` JSON for install.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryServer {
    /// Stable registry id (falls back to `namespace` when absent).
    pub id: String,
    /// Cleaned display name — last path segment of `namespace`.
    pub name: String,
    /// Full registry name, e.g. `io.github.netdata/mcp-server`.
    pub namespace: String,
    pub description: String,
    pub repo_url: String,
    pub stars: u32,
    pub license: Option<String>,
    pub version: Option<String>,
    pub kind: McpServerKind,
    /// Distinct runner hints across packages, e.g. `["uvx"]`, `["npx"]`.
    pub runtimes: Vec<String>,
    pub readme: Option<String>,
    pub updated_at: Option<String>,
    pub packages: Vec<McpRegistryPackageSummary>,
    pub remotes: Vec<McpRegistryRemoteSummary>,
    /// Serialized `server` object — input for the app's install mapping.
    pub raw_server_json: String,
    /// Curated SkillStar recommendation, stored outside the remote registry so
    /// refreshes never overwrite it.
    #[serde(default)]
    pub recommended: bool,
    /// Source bucket for local curated rows, e.g. `skillstar-curated`.
    #[serde(default)]
    pub source: Option<String>,
}

/// Lightweight card model for list/search results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpMarketEntry {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub description: String,
    pub repo_url: String,
    pub stars: u32,
    pub license: Option<String>,
    pub version: Option<String>,
    pub kind: McpServerKind,
    pub runtimes: Vec<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub source: Option<String>,
}

/// Detail model: card fields + readme + package/remote display.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpMarketServerDetail {
    #[serde(flatten)]
    pub entry: McpMarketEntry,
    pub readme: Option<String>,
    pub packages: Vec<McpRegistryPackageSummary>,
    pub remotes: Vec<McpRegistryRemoteSummary>,
}

/// One official MCP publisher shown on the marketplace grid.
///
/// The `id` doubles as the curated `source` bucket (`"adspower"` / `"bigmodel"`)
/// or the special `"github"` publisher which maps to the full
/// `mcp_registry_server` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpPublisherSummary {
    /// Publisher id — also the curated `source` value, or `"github"`.
    pub id: String,
    /// Display name (e.g. "AdsPower", "BigModel", "GitHub").
    pub name: String,
    /// Number of MCP servers offered by this publisher.
    pub server_count: u32,
    /// External landing page (docs / repo).
    pub url: String,
}

impl McpRegistryServer {
    pub fn to_card(&self) -> McpMarketEntry {
        McpMarketEntry {
            id: self.id.clone(),
            name: self.name.clone(),
            namespace: self.namespace.clone(),
            description: self.description.clone(),
            repo_url: self.repo_url.clone(),
            stars: self.stars,
            license: self.license.clone(),
            version: self.version.clone(),
            kind: self.kind,
            runtimes: self.runtimes.clone(),
            updated_at: self.updated_at.clone(),
            recommended: self.recommended,
            source: self.source.clone(),
        }
    }

    pub fn to_detail(&self) -> McpMarketServerDetail {
        McpMarketServerDetail {
            entry: self.to_card(),
            readme: self.readme.clone(),
            packages: self.packages.clone(),
            remotes: self.remotes.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing from the GitHub MCP Registry wire format
// ---------------------------------------------------------------------------

/// Map a registry package's `registry_type`/`registry_name`/`runtime_hint`
/// into the runner command we'd invoke for a stdio server.
pub fn runtime_command_for(registry_type: &str, runtime_hint: &str) -> String {
    if !runtime_hint.trim().is_empty() {
        return runtime_hint.trim().to_string();
    }
    match registry_type.trim().to_ascii_lowercase().as_str() {
        "npm" => "npx".to_string(),
        "pypi" | "uv" => "uvx".to_string(),
        "oci" | "docker" => "docker".to_string(),
        "nuget" => "dnx".to_string(),
        other if !other.is_empty() => other.to_string(),
        _ => String::new(),
    }
}

fn str_field(obj: &Value, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Last `/`-separated segment of a registry name, used as the default config key.
fn clean_name(namespace: &str) -> String {
    namespace
        .rsplit('/')
        .next()
        .unwrap_or(namespace)
        .trim()
        .to_string()
}

/// Normalize a remote's transport tag into `http` | `sse`.
fn normalize_remote_transport(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "sse" => "sse".to_string(),
        // "streamable-http", "streamable_http", "http", "" → http
        _ => "http".to_string(),
    }
}

/// Names of env vars / headers that the user must fill in (required or secret).
fn required_var_names(items: Option<&Value>) -> Vec<String> {
    let Some(arr) = items.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter(|v| {
            let secret = v.get("is_secret").and_then(Value::as_bool).unwrap_or(false);
            let required = v
                .get("is_required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            secret || required
        })
        .filter_map(|v| str_field(v, "name"))
        .collect()
}

fn parse_packages(server: &Value) -> Vec<McpRegistryPackageSummary> {
    let Some(arr) = server.get("packages").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .map(|pkg| {
            // The registry has used both `registry_type` and `registry_name`;
            // and both `identifier` and `name` for the package id.
            let registry_type = str_field(pkg, "registry_type")
                .or_else(|| str_field(pkg, "registry_name"))
                .unwrap_or_default();
            let runtime_hint = str_field(pkg, "runtime_hint").unwrap_or_default();
            let identifier = str_field(pkg, "identifier")
                .or_else(|| str_field(pkg, "name"))
                .unwrap_or_default();
            McpRegistryPackageSummary {
                runtime: runtime_command_for(&registry_type, &runtime_hint),
                identifier,
                version: str_field(pkg, "version"),
                required_env: required_var_names(pkg.get("environment_variables")),
            }
        })
        .collect()
}

fn parse_remotes(server: &Value) -> Vec<McpRegistryRemoteSummary> {
    let Some(arr) = server.get("remotes").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|remote| {
            let url = str_field(remote, "url")?;
            let transport = normalize_remote_transport(
                &str_field(remote, "transport_type")
                    .or_else(|| str_field(remote, "type"))
                    .unwrap_or_default(),
            );
            Some(McpRegistryRemoteSummary {
                transport,
                url,
                required_headers: required_var_names(remote.get("headers")),
            })
        })
        .collect()
}

fn parse_stars_and_license(element: &Value, server: &Value) -> (u32, Option<String>) {
    // GitHub metadata may live on the envelope (`x-github`) or inside the
    // server object's `_meta`.
    let gh = element
        .get("x-github")
        .or_else(|| server.get("x-github"))
        .or_else(|| {
            server
                .get("_meta")
                .and_then(|m| m.get("io.modelcontextprotocol.registry/official"))
        });
    let stars = gh
        .and_then(|g| {
            g.get("stars")
                .or_else(|| g.get("github_stars"))
                .or_else(|| g.get("stargazers_count"))
        })
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let license = gh.and_then(|g| {
        g.get("license").and_then(|l| {
            l.as_str()
                .map(str::to_string)
                .or_else(|| str_field(l, "name"))
        })
    });
    (stars, license)
}

/// Extract the `server` object from a registry list element. The API has
/// shipped both `{ server: {...}, x-github }` and the bare `server.json`.
fn server_object(element: &Value) -> Option<&Value> {
    match element.get("server") {
        Some(inner) if inner.is_object() => Some(inner),
        _ if element.is_object() => Some(element),
        _ => None,
    }
}

/// Parse one registry list element into a normalized `McpRegistryServer`.
/// Returns `None` only when the element lacks a usable name.
pub fn parse_registry_element(element: &Value) -> Option<McpRegistryServer> {
    let server = server_object(element)?;
    let namespace = str_field(server, "name")?;
    let name = clean_name(&namespace);
    if name.is_empty() {
        return None;
    }

    let id = str_field(server, "id").unwrap_or_else(|| namespace.clone());
    let description = str_field(server, "description").unwrap_or_default();
    let repository = server.get("repository");
    let repo_url = repository
        .and_then(|r| str_field(r, "url"))
        .unwrap_or_default();
    let readme = repository.and_then(|r| str_field(r, "readme"));
    let version = server
        .get("version_detail")
        .and_then(|v| str_field(v, "version"))
        .or_else(|| str_field(server, "version"));
    let updated_at = str_field(server, "updated_at").or_else(|| str_field(server, "created_at"));

    let packages = parse_packages(server);
    let remotes = parse_remotes(server);
    let kind = match (packages.is_empty(), remotes.is_empty()) {
        (false, false) => McpServerKind::Both,
        (false, true) => McpServerKind::Stdio,
        (true, false) => McpServerKind::Remote,
        (true, true) => McpServerKind::Unknown,
    };
    let mut runtimes: Vec<String> = Vec::new();
    for pkg in &packages {
        if !pkg.runtime.is_empty() && !runtimes.contains(&pkg.runtime) {
            runtimes.push(pkg.runtime.clone());
        }
    }

    let (stars, license) = parse_stars_and_license(element, server);
    let raw_server_json = serde_json::to_string(server).unwrap_or_default();

    Some(McpRegistryServer {
        id,
        name,
        namespace,
        description,
        repo_url,
        stars,
        license,
        version,
        kind,
        runtimes,
        readme,
        updated_at,
        packages,
        remotes,
        raw_server_json,
        recommended: false,
        source: None,
    })
}

/// Parse a full `/v0/servers` response body into normalized servers plus the
/// pagination cursor (`metadata.next_cursor`, if any).
pub fn parse_servers_response(
    body: &str,
) -> anyhow::Result<(Vec<McpRegistryServer>, Option<String>)> {
    let root: Value = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("Failed to parse MCP registry response: {e}"))?;

    let elements = root
        .get("servers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let servers: Vec<McpRegistryServer> =
        elements.iter().filter_map(parse_registry_element).collect();

    let next_cursor = root
        .get("metadata")
        .and_then(|m| m.get("next_cursor").or_else(|| m.get("nextCursor")))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    Ok((servers, next_cursor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stdio_package_server() {
        let body = r#"{
            "servers": [
                {
                    "server": {
                        "id": "abc123",
                        "name": "microsoft/markitdown",
                        "description": "Convert files to Markdown.",
                        "packages": [
                            { "registry_type": "pypi", "identifier": "markitdown-mcp", "version": "0.0.1a4", "runtime_hint": "uvx",
                              "environment_variables": [ { "name": "MD_TOKEN", "is_secret": true } ] }
                        ],
                        "remotes": [],
                        "repository": { "url": "https://github.com/microsoft/markitdown", "source": "github", "readme": "hi" },
                        "version_detail": { "version": "1.0.0" },
                        "updated_at": "2026-01-21T09:35:10Z"
                    },
                    "x-github": { "stars": 42, "license": "MIT" }
                }
            ],
            "metadata": { "next_cursor": "CURSOR2" }
        }"#;

        let (servers, cursor) = parse_servers_response(body).unwrap();
        assert_eq!(cursor.as_deref(), Some("CURSOR2"));
        assert_eq!(servers.len(), 1);
        let s = &servers[0];
        assert_eq!(s.name, "markitdown");
        assert_eq!(s.namespace, "microsoft/markitdown");
        assert_eq!(s.kind, McpServerKind::Stdio);
        assert_eq!(s.stars, 42);
        assert_eq!(s.license.as_deref(), Some("MIT"));
        assert_eq!(s.version.as_deref(), Some("1.0.0"));
        assert_eq!(s.runtimes, vec!["uvx".to_string()]);
        assert_eq!(s.packages.len(), 1);
        assert_eq!(s.packages[0].runtime, "uvx");
        assert_eq!(s.packages[0].identifier, "markitdown-mcp");
        assert_eq!(s.packages[0].required_env, vec!["MD_TOKEN".to_string()]);
        assert!(!s.raw_server_json.is_empty());
    }

    #[test]
    fn parses_remote_server_with_secret_header() {
        let body = r#"{
            "servers": [
                {
                    "server": {
                        "name": "io.github.netdata/mcp-server",
                        "description": "Monitoring.",
                        "packages": [],
                        "remotes": [
                            { "transport_type": "streamable-http", "url": "https://app.netdata.cloud/api/v1/mcp",
                              "headers": [ { "name": "Authorization", "value": "Bearer {TOKEN}", "is_secret": true } ] }
                        ],
                        "repository": { "url": "https://github.com/netdata/netdata", "source": "github" }
                    }
                }
            ]
        }"#;

        let (servers, cursor) = parse_servers_response(body).unwrap();
        assert!(cursor.is_none());
        let s = &servers[0];
        assert_eq!(s.name, "mcp-server");
        assert_eq!(s.kind, McpServerKind::Remote);
        assert_eq!(s.remotes.len(), 1);
        assert_eq!(s.remotes[0].transport, "http");
        assert_eq!(s.remotes[0].url, "https://app.netdata.cloud/api/v1/mcp");
        assert_eq!(
            s.remotes[0].required_headers,
            vec!["Authorization".to_string()]
        );
    }

    #[test]
    fn runtime_command_falls_back_to_registry_type() {
        assert_eq!(runtime_command_for("npm", ""), "npx");
        assert_eq!(runtime_command_for("pypi", ""), "uvx");
        assert_eq!(runtime_command_for("oci", ""), "docker");
        assert_eq!(runtime_command_for("npm", "bunx"), "bunx"); // hint wins
    }

    #[test]
    fn handles_bare_server_element_without_envelope() {
        let body = r#"{ "servers": [ { "name": "acme/thing", "packages": [], "remotes": [] } ] }"#;
        let (servers, _) = parse_servers_response(body).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "thing");
        assert_eq!(servers[0].kind, McpServerKind::Unknown);
    }
}

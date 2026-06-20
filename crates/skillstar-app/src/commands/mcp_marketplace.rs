//! Tauri commands for the **MCP marketplace** (GitHub MCP Registry).
//!
//! Read/sync commands delegate to `skillstar_marketplace::mcp_snapshot`
//! (local-first). The bridge command `mcp_market_entry_to_draft` converts a
//! registry server into a prefilled [`McpServerEntry`] draft — the frontend
//! opens it in the existing MCP server form (highlighting secret env/headers the
//! user must fill) and then calls the existing `create_mcp_server` command,
//! which projects it into Codex/Claude/Gemini/OpenCode via the established sync
//! machinery. No new install/clone path is introduced.

use std::collections::BTreeMap;

use serde_json::Value;
use skillstar_core::infra::error::AppError;
use skillstar_marketplace::mcp_models::runtime_command_for;
use skillstar_marketplace::{
    LocalFirstResult, McpMarketEntry, McpMarketServerDetail, McpPublisherSummary,
    McpRegistryServer, SyncStateEntry, mcp_snapshot,
};
use skillstar_models::mcp::McpServerEntry;
use tracing::{debug, error};

const MCP_REGISTRY_SCOPE: &str = "mcp_registry";

#[tauri::command]
pub async fn list_mcp_publishers_local() -> Result<Vec<McpPublisherSummary>, AppError> {
    debug!(target: "mcp_marketplace", "list_mcp_publishers_local called");
    mcp_snapshot::list_mcp_publishers().map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_mcp_servers_by_publisher_local(
    publisher_id: String,
) -> Result<LocalFirstResult<Vec<McpMarketEntry>>, AppError> {
    debug!(target: "mcp_marketplace", publisher = %publisher_id, "list_mcp_servers_by_publisher_local called");
    mcp_snapshot::list_mcp_servers_by_publisher(&publisher_id)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_mcp_market_servers_local()
-> Result<LocalFirstResult<Vec<McpMarketEntry>>, AppError> {
    debug!(target: "mcp_marketplace", "list_mcp_market_servers_local called");
    mcp_snapshot::list_mcp_servers_local()
        .await
        .map_err(|e| {
            error!(target: "mcp_marketplace", error = %e, "list local failed");
            AppError::Other(e.to_string())
        })
}

#[tauri::command]
pub async fn search_mcp_market_local(
    query: String,
    limit: Option<u32>,
) -> Result<LocalFirstResult<Vec<McpMarketEntry>>, AppError> {
    debug!(target: "mcp_marketplace", query = %query, "search_mcp_market_local called");
    mcp_snapshot::search_mcp_servers_local(&query, limit)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn get_mcp_market_server_detail_local(
    id: String,
) -> Result<LocalFirstResult<Option<McpMarketServerDetail>>, AppError> {
    debug!(target: "mcp_marketplace", id = %id, "get_mcp_market_server_detail_local called");
    mcp_snapshot::get_mcp_server_detail_local(&id)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn sync_mcp_market_scope(scope: String) -> Result<(), AppError> {
    debug!(target: "mcp_marketplace", scope = %scope, "sync_mcp_market_scope called");
    // Only the registry scope exists today; accept it (and the empty default)
    // and reject anything unexpected so typos surface instead of silently
    // syncing the wrong thing.
    if !scope.is_empty() && scope != MCP_REGISTRY_SCOPE {
        return Err(AppError::Other(format!("Unknown MCP market scope: {scope}")));
    }
    mcp_snapshot::sync_mcp_registry_scope()
        .await
        .map_err(|e| {
            error!(target: "mcp_marketplace", error = %e, "sync failed");
            AppError::Other(e.to_string())
        })
}

#[tauri::command]
pub async fn get_mcp_market_sync_states() -> Result<Vec<SyncStateEntry>, AppError> {
    mcp_snapshot::mcp_market_sync_states().map_err(|e| AppError::Other(e.to_string()))
}

/// Convert a marketplace server into a prefilled, ready-to-edit
/// [`McpServerEntry`] draft (id empty, secrets blank). The frontend finalizes it
/// in the MCP server form and submits via the existing `create_mcp_server`.
#[tauri::command]
pub async fn mcp_market_entry_to_draft(id: String) -> Result<McpServerEntry, AppError> {
    debug!(target: "mcp_marketplace", id = %id, "mcp_market_entry_to_draft called");
    let server = mcp_snapshot::get_registry_server_local(&id)
        .map_err(|e| AppError::Other(e.to_string()))?
        .ok_or_else(|| {
            AppError::Other(format!("MCP server '{id}' not found in local snapshot"))
        })?;
    Ok(registry_to_entry(&server))
}

// ---------------------------------------------------------------------------
// registry server.json → McpServerEntry draft
// ---------------------------------------------------------------------------

/// Sanitize a registry name into a valid MCP config key (servers are written
/// verbatim into tool configs, so keep it to `[A-Za-z0-9_-]`).
fn sanitize_key(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() { "mcp-server".to_string() } else { trimmed.to_string() }
}

fn str_at(obj: &Value, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Extract CLI args from a registry `runtime_arguments`/`package_arguments`
/// array. Best-effort: strings pass through; objects contribute their flag name
/// (for `named`) and their `value`/`default`.
fn extract_args(value: Option<&Value>) -> Vec<String> {
    let Some(arr) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for arg in arr {
        if let Some(s) = arg.as_str() {
            out.push(s.to_string());
            continue;
        }
        if arg.get("type").and_then(Value::as_str) == Some("named") {
            if let Some(name) = str_at(arg, "name") {
                out.push(name);
            }
        }
        if let Some(val) = str_at(arg, "value").or_else(|| str_at(arg, "default")) {
            out.push(val);
        }
    }
    out
}

/// Extract env vars: required/secret vars are left blank (placeholders the user
/// must fill); others get their default/value.
fn extract_env(value: Option<&Value>) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let Some(arr) = value.and_then(Value::as_array) else {
        return env;
    };
    for item in arr {
        let Some(name) = str_at(item, "name") else { continue };
        let secret = item.get("is_secret").and_then(Value::as_bool).unwrap_or(false);
        let required = item.get("is_required").and_then(Value::as_bool).unwrap_or(false);
        let value = if secret || required {
            String::new()
        } else {
            str_at(item, "default").or_else(|| str_at(item, "value")).unwrap_or_default()
        };
        env.insert(name, value);
    }
    env
}

fn normalize_transport(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "sse" => "sse".to_string(),
        _ => "http".to_string(),
    }
}

fn fill_stdio(entry: &mut McpServerEntry, pkg: &Value) {
    let registry_type = str_at(pkg, "registry_type")
        .or_else(|| str_at(pkg, "registry_name"))
        .unwrap_or_default();
    let runtime_hint = str_at(pkg, "runtime_hint").unwrap_or_default();
    let identifier = str_at(pkg, "identifier").or_else(|| str_at(pkg, "name")).unwrap_or_default();
    let version = str_at(pkg, "version");
    let command = runtime_command_for(&registry_type, &runtime_hint);

    entry.transport = "stdio".to_string();
    if !command.is_empty() {
        entry.command = Some(command.clone());
    }

    let runtime_args = extract_args(pkg.get("runtime_arguments"));
    let package_args = extract_args(pkg.get("package_arguments"));
    let versioned = |sep: &str| match &version {
        Some(v) => format!("{identifier}{sep}{v}"),
        None => identifier.clone(),
    };

    let mut args = runtime_args.clone();
    match command.as_str() {
        "npx" | "bunx" => {
            if !args.iter().any(|a| a == "-y" || a == "--yes") {
                args.push("-y".to_string());
            }
            if !identifier.is_empty() {
                args.push(versioned("@"));
            }
        }
        "uvx" | "dnx" => {
            if !identifier.is_empty() {
                args.push(versioned("@"));
            }
        }
        "docker" => {
            if !runtime_args.iter().any(|a| a == "run") {
                args.extend(["run", "-i", "--rm"].iter().map(|s| s.to_string()));
            }
            if !identifier.is_empty() {
                args.push(versioned(":"));
            }
        }
        _ => {
            if !identifier.is_empty() {
                args.push(versioned("@"));
            }
        }
    }
    args.extend(package_args);
    entry.args = args;
    entry.env = extract_env(pkg.get("environment_variables"));
}

fn fill_remote(entry: &mut McpServerEntry, remote: &Value) {
    entry.transport = normalize_transport(
        &str_at(remote, "transport_type").or_else(|| str_at(remote, "type")).unwrap_or_default(),
    );
    entry.url = str_at(remote, "url");
    let mut headers = BTreeMap::new();
    if let Some(arr) = remote.get("headers").and_then(Value::as_array) {
        for header in arr {
            let Some(name) = str_at(header, "name") else { continue };
            // Keep the template value (e.g. "Bearer {TOKEN}") so the user sees
            // the expected format and replaces the placeholder.
            let value = str_at(header, "value").unwrap_or_default();
            headers.insert(name, value);
        }
    }
    entry.headers = headers;
}

/// Map a cached registry server into a prefilled `McpServerEntry` draft.
pub fn registry_to_entry(server: &McpRegistryServer) -> McpServerEntry {
    let raw: Value = serde_json::from_str(&server.raw_server_json).unwrap_or(Value::Null);

    let mut entry = McpServerEntry {
        id: String::new(),
        name: sanitize_key(&server.name),
        transport: "stdio".to_string(),
        command: None,
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
        url: None,
        headers: BTreeMap::new(),
        description: (!server.description.is_empty()).then(|| server.description.clone()),
        homepage: (!server.repo_url.is_empty()).then(|| server.repo_url.clone()),
        tags: Vec::new(),
        enabled: BTreeMap::new(),
        sort_index: 0,
        created_at: None,
        updated_at: None,
    };

    let first_package = raw.get("packages").and_then(Value::as_array).and_then(|a| a.first());
    let first_remote = raw.get("remotes").and_then(Value::as_array).and_then(|a| a.first());

    // Prefer a runnable local package; fall back to a remote endpoint.
    if let Some(pkg) = first_package {
        fill_stdio(&mut entry, pkg);
    } else if let Some(remote) = first_remote {
        fill_remote(&mut entry, remote);
    }

    entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_marketplace::McpServerKind;

    fn server_with_raw(name: &str, raw: &str) -> McpRegistryServer {
        McpRegistryServer {
            id: "id".into(),
            name: name.into(),
            namespace: format!("acme/{name}"),
            description: "desc".into(),
            repo_url: "https://github.com/acme/x".into(),
            stars: 0,
            license: None,
            version: None,
            kind: McpServerKind::Unknown,
            runtimes: vec![],
            readme: None,
            updated_at: None,
            packages: vec![],
            remotes: vec![],
            raw_server_json: raw.into(),
            recommended: false,
            source: None,
        }
    }

    #[test]
    fn maps_npm_package_to_stdio_npx() {
        let raw = r#"{ "packages": [
            { "registry_type": "npm", "identifier": "@modelcontextprotocol/server-filesystem", "version": "1.2.0",
              "environment_variables": [ { "name": "ROOT", "default": "/tmp" }, { "name": "API_KEY", "is_secret": true } ] }
        ] }"#;
        let entry = registry_to_entry(&server_with_raw("server-filesystem", raw));
        assert_eq!(entry.transport, "stdio");
        assert_eq!(entry.command.as_deref(), Some("npx"));
        assert_eq!(
            entry.args,
            vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem@1.2.0".to_string()]
        );
        assert_eq!(entry.env.get("ROOT").map(String::as_str), Some("/tmp"));
        assert_eq!(entry.env.get("API_KEY").map(String::as_str), Some("")); // secret blanked
        assert_eq!(entry.name, "server-filesystem");
    }

    #[test]
    fn maps_pypi_package_to_uvx() {
        let raw = r#"{ "packages": [
            { "registry_type": "pypi", "identifier": "markitdown-mcp", "version": "0.0.1a4", "runtime_hint": "uvx" }
        ] }"#;
        let entry = registry_to_entry(&server_with_raw("markitdown", raw));
        assert_eq!(entry.command.as_deref(), Some("uvx"));
        assert_eq!(entry.args, vec!["markitdown-mcp@0.0.1a4".to_string()]);
    }

    #[test]
    fn maps_oci_package_to_docker_run() {
        let raw = r#"{ "packages": [
            { "registry_type": "oci", "identifier": "mcp/everything", "version": "latest" }
        ] }"#;
        let entry = registry_to_entry(&server_with_raw("everything", raw));
        assert_eq!(entry.command.as_deref(), Some("docker"));
        assert_eq!(
            entry.args,
            vec![
                "run".to_string(),
                "-i".to_string(),
                "--rm".to_string(),
                "mcp/everything:latest".to_string()
            ]
        );
    }

    #[test]
    fn maps_remote_to_http_with_header_template() {
        let raw = r#"{ "packages": [], "remotes": [
            { "transport_type": "streamable-http", "url": "https://app.netdata.cloud/api/v1/mcp",
              "headers": [ { "name": "Authorization", "value": "Bearer {TOKEN}", "is_secret": true } ] }
        ] }"#;
        let entry = registry_to_entry(&server_with_raw("mcp-server", raw));
        assert_eq!(entry.transport, "http");
        assert_eq!(entry.url.as_deref(), Some("https://app.netdata.cloud/api/v1/mcp"));
        assert_eq!(entry.headers.get("Authorization").map(String::as_str), Some("Bearer {TOKEN}"));
        assert!(entry.command.is_none());
    }

    #[test]
    fn sanitizes_dotted_namespace_name() {
        // Already-cleaned name should pass through; weird chars become '-'.
        assert_eq!(sanitize_key("mcp-server"), "mcp-server");
        assert_eq!(sanitize_key("foo.bar baz"), "foo-bar-baz");
        assert_eq!(sanitize_key("--"), "mcp-server");
    }

    /// BigModel curated servers use the GitHub registry wire shape with an npm
    /// package carrying a required secret env var. Verify the real raw JSON
    /// stored in marketplace.db converts to a runnable stdio entry.
    #[test]
    fn converts_bigmodel_vision_curated_raw() {
        let raw = r#"{
            "id": "bigmodel-vision",
            "name": "bigmodel-vision",
            "packages": [
                { "registry_type": "npm", "identifier": "@z_ai/mcp-server", "runtime_hint": "npx",
                  "environment_variables": [ { "name": "Z_AI_API_KEY", "is_secret": true, "is_required": true } ] }
            ],
            "remotes": []
        }"#;
        let entry = registry_to_entry(&server_with_raw("bigmodel-vision", raw));
        assert_eq!(entry.transport, "stdio");
        assert_eq!(entry.command.as_deref(), Some("npx"));
        // npx gets -y + the scoped identifier
        assert!(entry.args.iter().any(|a| a == "-y"));
        assert!(entry.args.iter().any(|a| a == "@z_ai/mcp-server"));
        // required secret env must be present and blanked for user fill-in
        assert_eq!(entry.env.get("Z_AI_API_KEY").map(String::as_str), Some(""));
    }

    /// AdsPower curated server: npm package with a defaulted PORT and a
    /// required API_KEY. Verify the real raw JSON converts correctly.
    #[test]
    fn converts_adspower_curated_raw() {
        let raw = r#"{
            "id": "adspower-local-api",
            "name": "adspower-local-api",
            "packages": [
                { "registry_type": "npm", "identifier": "local-api-mcp-typescript", "runtime_hint": "npx",
                  "environment_variables": [
                    { "name": "PORT", "default": "50325" },
                    { "name": "API_KEY", "is_secret": true, "is_required": true }
                  ] }
            ],
            "remotes": []
        }"#;
        let entry = registry_to_entry(&server_with_raw("adspower-local-api", raw));
        assert_eq!(entry.transport, "stdio");
        assert_eq!(entry.command.as_deref(), Some("npx"));
        assert!(entry.args.iter().any(|a| a == "local-api-mcp-typescript"));
        // PORT keeps its default; API_KEY is blanked
        assert_eq!(entry.env.get("PORT").map(String::as_str), Some("50325"));
        assert_eq!(entry.env.get("API_KEY").map(String::as_str), Some(""));
    }
}

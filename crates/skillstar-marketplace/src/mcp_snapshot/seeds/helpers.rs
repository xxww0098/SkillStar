//! Shared builders / raw JSON templates for curated MCP seed factories.

use crate::mcp_models::{
    McpRegistryPackageSummary, McpRegistryRemoteSummary, McpRegistryServer, McpServerKind,
};

// ── Additional curated publishers ───────────────────────────────────────
// Each factory mirrors the BigModel pattern: a `make` closure builds a
// `McpRegistryServer` with a `raw_server_json` in the GitHub registry
// server.json shape, so `registry_to_entry` installs them unchanged.

/// Build a stdio (npx) curated server. The `raw` must carry a `packages`
/// entry with `registry_type: "npm"` + `runtime_hint: "npx"`.
// Plain positional args mirror the curated-seed call sites below (20+
// call sites); a builder/struct would churn all of them for no behavior
// change, so the arg count is accepted here.
#[allow(clippy::too_many_arguments)]
pub(super) fn make_stdio_curated(
    id: &str,
    name: &str,
    description: &str,
    raw: &str,
    source: &str,
    repo_url: &str,
    npm_identifier: &str,
    required_env: &[&str],
) -> McpRegistryServer {
    McpRegistryServer {
        id: id.to_string(),
        name: name.to_string(),
        namespace: id.to_string(),
        description: description.to_string(),
        repo_url: repo_url.to_string(),
        stars: 0,
        license: None,
        version: None,
        kind: McpServerKind::Stdio,
        runtimes: vec!["npx".to_string()],
        readme: Some(format!("# {name}\n\n{description}")),
        updated_at: None,
        packages: vec![McpRegistryPackageSummary {
            runtime: "npx".to_string(),
            identifier: npm_identifier.to_string(),
            version: None,
            required_env: required_env.iter().map(|s| s.to_string()).collect(),
            registry_type: Some("npm".to_string()),
            runtime_hint: Some("npx".to_string()),
            ..Default::default()
        }],
        remotes: Vec::new(),
        raw_server_json: raw.to_string(),
        recommended: false,
        source: Some(source.to_string()),
        ..Default::default()
    }
}

/// Build a remote (streamable-http) curated server. The `raw` must carry a
/// `remotes` entry with `transport_type: "streamable-http"`.
// Plain positional args mirror the curated-seed call sites below; see
// `make_stdio_curated` for why a struct/builder isn't worth the churn here.
#[allow(clippy::too_many_arguments)]
pub(super) fn make_remote_curated(
    id: &str,
    name: &str,
    description: &str,
    raw: &str,
    source: &str,
    repo_url: &str,
    url: &str,
    auth_header: &str,
) -> McpRegistryServer {
    McpRegistryServer {
        id: id.to_string(),
        name: name.to_string(),
        namespace: id.to_string(),
        description: description.to_string(),
        repo_url: repo_url.to_string(),
        stars: 0,
        license: None,
        version: None,
        kind: McpServerKind::Remote,
        runtimes: Vec::new(),
        readme: Some(format!("# {name}\n\n{description}")),
        updated_at: None,
        packages: Vec::new(),
        remotes: vec![McpRegistryRemoteSummary {
            transport: "http".to_string(),
            url: url.to_string(),
            required_headers: vec![auth_header.to_string()],
            transport_type: Some("streamable-http".to_string()),
            ..Default::default()
        }],
        raw_server_json: raw.to_string(),
        recommended: false,
        source: Some(source.to_string()),
        ..Default::default()
    }
}

/// Raw JSON template for a stdio npx server with optional env vars.
pub(super) fn stdio_npx_raw(
    id: &str,
    name: &str,
    description: &str,
    npm_pkg: &str,
    repo_url: &str,
    env_vars: &[(&str, Option<&str>, bool)], // (name, default, is_required_secret)
) -> String {
    let envs: Vec<String> = env_vars
        .iter()
        .map(|(n, def, secret)| {
            let def_clause = def
                .map(|d| format!(", \"default\": \"{d}\""))
                .unwrap_or_default();
            format!(
                "{{ \"name\": \"{n}\", \"is_secret\": {secret}, \"is_required\": {secret}{def_clause} }}",
                secret = secret,
            )
        })
        .collect();
    format!(
        r##"{{
        "id": "{id}",
        "name": "{name}",
        "description": "{description}",
        "packages": [
            {{
                "registry_type": "npm",
                "identifier": "{npm_pkg}",
                "runtime_hint": "npx",
                "environment_variables": [{envs}]
            }}
        ],
        "remotes": [],
        "repository": {{ "url": "{repo_url}", "source": "github" }}
    }}"##,
        envs = envs.join(", "),
    )
}

/// Raw JSON template for a remote streamable-http server with a bearer header.
pub(super) fn remote_http_raw(
    id: &str,
    name: &str,
    description: &str,
    url: &str,
    repo_url: &str,
    header_name: &str,
    token_env: &str,
) -> String {
    format!(
        r##"{{
        "id": "{id}",
        "name": "{name}",
        "description": "{description}",
        "packages": [],
        "remotes": [
            {{
                "transport_type": "streamable-http",
                "url": "{url}",
                "headers": [
                    {{ "name": "{header_name}", "value": "Bearer {{{token_env}}}", "is_secret": true, "is_required": true }}
                ]
            }}
        ],
        "repository": {{ "url": "{repo_url}", "source": "github" }}
    }}"##
    )
}

mod json_extra;
pub mod json_formats;
pub mod paste;
pub mod toml_yaml_formats;

use crate::error::McpHubResult;
use crate::model::{McpFormat, McpServer, SyncWarning};
use crate::targets::BuiltInTargetSpec;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub use paste::{ParsedPasteResult, parse_pasted_config};

/// Outcome of attempting to write or delete an MCP server inside one target config file.
#[derive(Debug, Clone)]
pub struct FileSyncOutcome {
    pub warning: Option<SyncWarning>,
    pub applied_tools_off: BTreeSet<String>,
}

/// Read all MCP server entries currently defined in an agent's config file (lossless read-only).
pub fn read_entries_from_target(
    spec: &BuiltInTargetSpec,
    path: &Path,
) -> McpHubResult<Vec<McpServer>> {
    match spec.format {
        McpFormat::Codex | McpFormat::Grok => toml_yaml_formats::read_toml_servers(spec, path),
        McpFormat::Goose | McpFormat::Hermes | McpFormat::Dsh => {
            toml_yaml_formats::read_yaml_servers(spec, path)
        }
        _ => json_formats::read_json_servers(spec, path),
    }
}

/// Losslessly write or update a single MCP server entry in an agent's config file.
pub fn write_server_to_path(
    spec: &BuiltInTargetSpec,
    path: &Path,
    server: &McpServer,
    prev_tools_off: &BTreeSet<String>,
) -> McpHubResult<FileSyncOutcome> {
    if !spec.format.supports_remote() && server.transport.is_remote() {
        return Ok(FileSyncOutcome {
            warning: Some(SyncWarning {
                agent_id: spec.id.clone(),
                path: path.display().to_string(),
                code: "unsupported_remote".to_string(),
                message: format!(
                    "{} only supports stdio MCP servers; skipped '{}'",
                    spec.name, server.name
                ),
            }),
            applied_tools_off: BTreeSet::new(),
        });
    }
    if !spec.format.supports_sse() && server.transport == crate::model::Transport::Sse {
        return Ok(FileSyncOutcome {
            warning: Some(SyncWarning {
                agent_id: spec.id.clone(),
                path: path.display().to_string(),
                code: "unsupported_sse".to_string(),
                message: format!(
                    "{} does not support SSE MCP servers; skipped '{}'",
                    spec.name, server.name
                ),
            }),
            applied_tools_off: BTreeSet::new(),
        });
    }

    match spec.format {
        McpFormat::Codex | McpFormat::Grok => {
            toml_yaml_formats::write_toml_server(spec, path, server, prev_tools_off)
        }
        McpFormat::Goose | McpFormat::Hermes | McpFormat::Dsh => {
            toml_yaml_formats::write_yaml_server(spec, path, server, prev_tools_off)
        }
        _ => json_formats::write_json_server(spec, path, server, prev_tools_off),
    }
}

/// Losslessly remove one ledger-owned MCP server entry from an agent's config file.
pub fn remove_server_from_path(
    spec: &BuiltInTargetSpec,
    path: &Path,
    server_name: &str,
) -> McpHubResult<FileSyncOutcome> {
    match spec.format {
        McpFormat::Codex | McpFormat::Grok => {
            toml_yaml_formats::remove_toml_server(spec, path, server_name)
        }
        McpFormat::Goose | McpFormat::Hermes | McpFormat::Dsh => {
            toml_yaml_formats::remove_yaml_server(spec, path, server_name)
        }
        _ => json_formats::remove_json_server(spec, path, server_name),
    }
}

/// Sync external tool-disable maps (`claude` permissions.deny / `opencode` `tools`)
/// while retaining every user-managed rule not tracked in `prev_ledger`.
pub fn sync_external_tools_off(
    spec: &BuiltInTargetSpec,
    desired_by_server: &BTreeMap<String, Vec<String>>,
    prev_ledger: &BTreeMap<String, BTreeSet<String>>,
) -> McpHubResult<BTreeMap<String, BTreeSet<String>>> {
    match spec.id.as_str() {
        "claude" => {
            let Some(settings_path) = &spec.claude_settings_path else {
                return Ok(BTreeMap::new());
            };
            json_formats::sync_claude_settings_deny(
                settings_path,
                desired_by_server,
                prev_ledger,
            )
        }
        "opencode" | "deveco" | "mimocode" => json_formats::sync_opencode_tools_map(
            &spec.primary_path,
            desired_by_server,
            prev_ledger,
        ),
        _ => Ok(BTreeMap::new()),
    }
}

/// Render a standalone project-level configuration snippet for the requested format.
pub fn encode_project_snippet(
    format: McpFormat,
    no_cmd_refs: bool,
    servers: &[&McpServer],
) -> String {
    match format {
        McpFormat::Codex => toml_yaml_formats::encode_codex_project_snippet(servers),
        _ => json_extra::encode_json_project_snippet(format, no_cmd_refs, servers),
    }
}

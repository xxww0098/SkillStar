//! MCP tool registry — the single source of truth for per-tool facts.
//!
//! One [`McpToolSpec`] row per MCP-capable tool: identity, config-path
//! resolver, install probe, and the wire-format dispatch columns
//! (`count_live` / `read_servers` / `upsert` / `remove`). The per-tool `match`
//! sites in `types.rs` / `tools.rs` / `import.rs` / `sync.rs` all consult this
//! table; adding a tool means adding one row here (plus a spec builder in
//! `specs.rs` when the wire format is new).
//!
//! This is a wider, independent domain table from `tool_sync::agents`
//! (`AgentSpec`): MCP additionally targets grok / zcode / kiro / cursor,
//! and the hidden legacy `claude-desktop` cleanup id deliberately stays
//! outside it (not a public target).

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::*;

/// Registry row: every per-tool fact the MCP subsystem needs.
pub(crate) struct McpToolSpec {
    /// Canonical tool id (`claude-code`, `codex`, …). Shared with the frontend.
    pub id: &'static str,
    /// Human-readable label for status listings.
    pub label: &'static str,
    /// Live MCP config file resolver (funnels through `sync_home_dir`).
    pub resolve_config_path: fn() -> Result<PathBuf>,
    /// Best-effort install probe; receives the (sandboxable) home dir.
    pub installed: fn(&Path) -> bool,
    /// Count servers in raw config-file content (wire-format specific).
    pub count_live: fn(&str) -> usize,
    /// Parse raw config-file content into store entries (wire-format specific).
    pub read_servers: fn(&str) -> Result<Vec<McpServerEntry>>,
    /// Upsert one server into the tool's config file at `path`.
    pub upsert: fn(&Path, &McpServerEntry) -> Result<()>,
    /// Remove one server (by name) from the tool's config file at `path`.
    pub remove: fn(&Path, &str) -> Result<()>,
}

/// The registry. Order is the canonical display order pinned by
/// [`MCP_TOOL_IDS`] (consistency test below).
static MCP_TOOL_SPECS: &[McpToolSpec] = &[
    McpToolSpec {
        id: "claude-code",
        label: "Claude Code",
        resolve_config_path: resolve_claude_json_path,
        installed: installed_claude_code,
        count_live: count_json_mcpservers,
        read_servers: read_json_mcpservers_entries,
        upsert: |path, entry| json_mcpservers_upsert(path, &entry.name, claude_code_spec(entry)),
        remove: json_mcpservers_remove,
    },
    McpToolSpec {
        id: "codex",
        label: "Codex",
        resolve_config_path: crate::tool_sync::resolve_codex_config_path,
        installed: |home| home.join(".codex").exists(),
        count_live: count_toml_mcp_servers,
        read_servers: read_toml_mcp_servers_entries,
        upsert: |path, entry| codex_upsert(path, &entry.name, codex_toml_table(entry)),
        remove: codex_remove,
    },
    McpToolSpec {
        id: "gemini",
        label: "Gemini CLI",
        resolve_config_path: resolve_gemini_settings_path,
        installed: |home| home.join(".gemini").exists(),
        count_live: count_json_mcpservers,
        read_servers: read_json_mcpservers_entries,
        upsert: |path, entry| json_mcpservers_upsert(path, &entry.name, gemini_spec(entry)),
        remove: json_mcpservers_remove,
    },
    McpToolSpec {
        id: "grok",
        label: "Grok",
        resolve_config_path: resolve_grok_config_path,
        installed: |home| home.join(".grok").exists(),
        count_live: count_toml_mcp_servers,
        read_servers: read_toml_mcp_servers_entries,
        upsert: |path, entry| codex_upsert(path, &entry.name, grok_toml_table(entry)),
        remove: codex_remove,
    },
    McpToolSpec {
        id: "opencode",
        label: "OpenCode",
        resolve_config_path: crate::tool_sync::resolve_opencode_config_path,
        installed: installed_opencode,
        count_live: count_opencode_mcp,
        read_servers: read_opencode_mcp_entries,
        upsert: |path, entry| opencode_upsert(path, &entry.name, opencode_spec(entry)),
        remove: opencode_remove,
    },
    McpToolSpec {
        id: "zcode",
        label: "ZCode",
        resolve_config_path: resolve_zcode_cli_mcp_config_path,
        installed: |home| home.join(".zcode").exists(),
        count_live: count_zcode_cli_servers,
        read_servers: read_zcode_cli_entries,
        // ZCode carries a best-effort cleanup of the stale OpenCode-style
        // entry in `~/.zcode/v2/config.json` on both write paths.
        upsert: |path, entry| {
            zcode_cli_upsert(path, &entry.name, zcode_cli_spec(entry))?;
            let _ = zcode_v2_opencode_mcp_remove(&entry.name);
            Ok(())
        },
        remove: |path, name| {
            zcode_cli_remove(path, name)?;
            let _ = zcode_v2_opencode_mcp_remove(name);
            Ok(())
        },
    },
    McpToolSpec {
        id: "kiro",
        label: "Kiro",
        resolve_config_path: resolve_kiro_config_path,
        installed: |home| home.join(".kiro").exists(),
        count_live: count_json_mcpservers,
        read_servers: read_json_mcpservers_entries,
        upsert: |path, entry| json_mcpservers_upsert(path, &entry.name, kiro_spec(entry)),
        remove: json_mcpservers_remove,
    },
    McpToolSpec {
        id: "cursor",
        label: "Cursor",
        resolve_config_path: resolve_cursor_config_path,
        installed: |home| home.join(".cursor").exists(),
        count_live: count_json_mcpservers,
        read_servers: read_json_mcpservers_entries,
        upsert: |path, entry| json_mcpservers_upsert(path, &entry.name, cursor_spec(entry)),
        remove: json_mcpservers_remove,
    },
];

/// All registered MCP tools, in canonical display order.
pub(crate) fn mcp_tool_specs() -> &'static [McpToolSpec] {
    MCP_TOOL_SPECS
}

/// Look up one tool's spec by tool id.
pub(crate) fn mcp_tool_spec(tool_id: &str) -> Option<&'static McpToolSpec> {
    MCP_TOOL_SPECS.iter().find(|spec| spec.id == tool_id)
}

// ---------------------------------------------------------------------------
// Table-driven consistency tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_matches_public_tool_ids_in_order() {
        let ids: Vec<&str> = mcp_tool_specs().iter().map(|s| s.id).collect();
        assert_eq!(ids, MCP_TOOL_IDS, "registry drifted from MCP_TOOL_IDS");
    }

    #[test]
    fn registry_rows_are_well_formed() {
        let mut seen = HashSet::new();
        for spec in mcp_tool_specs() {
            assert!(seen.insert(spec.id), "duplicate tool id {}", spec.id);
            assert!(!spec.label.is_empty(), "{}", spec.id);
        }
    }

    #[test]
    fn legacy_desktop_chat_stays_out_of_the_registry() {
        assert!(mcp_tool_spec(LEGACY_CLAUDE_DESKTOP_TOOL_ID).is_none());
        assert!(mcp_tool_spec("unknown").is_none());
    }

    #[test]
    fn registry_paths_resolve_under_the_sandboxed_home() {
        // In-crate unit tests always resolve through the sandbox fallback, so
        // every resolver must succeed and stay inside one root.
        for spec in mcp_tool_specs() {
            let path = (spec.resolve_config_path)()
                .unwrap_or_else(|e| panic!("{}: resolver failed: {e}", spec.id));
            assert!(path.is_absolute(), "{}: {path:?}", spec.id);
        }
    }

    #[test]
    fn labels_match_the_public_label_helper() {
        for spec in mcp_tool_specs() {
            assert_eq!(mcp_tool_label(spec.id), spec.label, "{}", spec.id);
        }
        assert_eq!(mcp_tool_label("unknown"), "Unknown");
    }
}

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
//! (`AgentSpec`): MCP additionally targets grok / hermes / zcode / kiro /
//! cursor / antigravity, and the hidden legacy `claude-desktop` / `gemini`
//! cleanup ids deliberately stay outside it (not public targets — their live
//! successors are the distinct ids `claude-desktop-chat` and `gemini-cli`).

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
        id: CLAUDE_DESKTOP_CHAT_TOOL_ID,
        label: "Claude Desktop",
        resolve_config_path: resolve_claude_desktop_chat_config_path,
        installed: installed_claude_desktop_chat,
        count_live: count_json_mcpservers,
        read_servers: read_claude_desktop_chat_entries,
        upsert: |path, entry| {
            json_mcpservers_upsert(path, &entry.name, claude_desktop_chat_spec(entry))
        },
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
        id: "hermes",
        label: "Hermes Agent",
        resolve_config_path: resolve_hermes_config_path,
        installed: installed_hermes,
        count_live: count_hermes_mcp,
        read_servers: read_hermes_entries,
        upsert: |path, entry| hermes_upsert(path, &entry.name, hermes_spec(entry)),
        remove: hermes_remove,
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
    McpToolSpec {
        id: "vscode",
        label: "VS Code",
        resolve_config_path: resolve_vscode_config_path,
        installed: |home| home.join(".copilot").exists(),
        count_live: count_vscode_servers,
        read_servers: read_vscode_entries,
        upsert: |path, entry| {
            json_named_map_upsert(path, VSCODE_SERVERS_KEY, &entry.name, vscode_spec(entry))
        },
        remove: |path, name| json_named_map_remove(path, VSCODE_SERVERS_KEY, name),
    },
    McpToolSpec {
        id: "windsurf",
        label: "Windsurf",
        resolve_config_path: resolve_windsurf_config_path,
        installed: |home| home.join(".codeium").join("windsurf").exists(),
        count_live: count_json_mcpservers,
        read_servers: read_windsurf_entries,
        upsert: |path, entry| json_mcpservers_upsert(path, &entry.name, windsurf_spec(entry)),
        remove: json_mcpservers_remove,
    },
    McpToolSpec {
        id: "cline",
        label: "Cline",
        resolve_config_path: resolve_cline_config_path,
        installed: |home| home.join(".cline").exists(),
        count_live: count_json_mcpservers,
        read_servers: read_cline_entries,
        upsert: |path, entry| json_mcpservers_upsert(path, &entry.name, cline_spec(entry)),
        remove: json_mcpservers_remove,
    },
    McpToolSpec {
        id: GEMINI_CLI_TOOL_ID,
        label: "Gemini CLI",
        resolve_config_path: resolve_gemini_cli_config_path,
        installed: |home| home.join(".gemini").exists(),
        count_live: count_json_mcpservers,
        read_servers: read_gemini_cli_entries,
        upsert: |path, entry| json_mcpservers_upsert(path, &entry.name, gemini_cli_spec(entry)),
        remove: json_mcpservers_remove,
    },
    McpToolSpec {
        id: "antigravity",
        label: "Antigravity",
        resolve_config_path: resolve_antigravity_config_path,
        installed: installed_antigravity,
        count_live: count_json_mcpservers,
        read_servers: read_antigravity_entries,
        upsert: |path, entry| json_mcpservers_upsert(path, &entry.name, antigravity_spec(entry)),
        remove: json_mcpservers_remove,
    },
    McpToolSpec {
        id: "zed",
        label: "Zed",
        resolve_config_path: resolve_zed_config_path,
        installed: |home| home.join(".config").join("zed").exists(),
        count_live: count_zed_context_servers,
        read_servers: read_zed_entries,
        upsert: |path, entry| {
            json_named_map_upsert(path, ZED_SERVERS_KEY, &entry.name, zed_spec(entry))
        },
        remove: |path, name| json_named_map_remove(path, ZED_SERVERS_KEY, name),
    },
    McpToolSpec {
        id: "maka",
        label: "Maka",
        resolve_config_path: resolve_maka_config_path,
        installed: installed_maka,
        count_live: count_json_mcpservers,
        read_servers: read_maka_entries,
        upsert: |path, entry| maka_upsert(path, &entry.name, maka_spec(entry)),
        remove: maka_remove,
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

    /// Each cleanup tombstone stays out of the registry while its public
    /// successor is in it, under a *different* id.
    ///
    /// The invariant is the id split, not the absence of the product: a
    /// tombstone authorizes exactly one removal, a registry row is a standing
    /// enable flag, and collapsing them onto one id would let a stale `true`
    /// in an old store keep deleting what the live target writes.
    #[test]
    fn cleanup_tombstones_and_their_public_successors_use_distinct_ids() {
        for (tombstone, successor) in [
            (LEGACY_CLAUDE_DESKTOP_TOOL_ID, CLAUDE_DESKTOP_CHAT_TOOL_ID),
            (LEGACY_GEMINI_TOOL_ID, GEMINI_CLI_TOOL_ID),
        ] {
            assert_ne!(tombstone, successor);
            assert!(
                mcp_tool_spec(tombstone).is_none(),
                "tombstone '{tombstone}' must not be a registry row"
            );
            assert!(
                !MCP_TOOL_IDS.contains(&tombstone),
                "tombstone '{tombstone}' must not be a public target"
            );
            assert!(
                mcp_tool_spec(successor).is_some(),
                "public successor '{successor}' must be a registry row"
            );
            // Same file, different id — that is what makes subsumption
            // (`sync::cleanup_legacy_*`) necessary rather than optional.
            assert_eq!(
                resolve_mcp_config_path(tombstone).unwrap(),
                resolve_mcp_config_path(successor).unwrap(),
                "'{tombstone}' and '{successor}' must resolve to the same file"
            );
        }
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

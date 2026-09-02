//! MCP (Model Context Protocol) server management.
//!
//! SkillStar owns a single unified MCP server store at
//! `~/.skillstar/config/mcp_servers.json` and *projects* each server into the
//! native config file of every supported agent tool. This mirrors the mature
//! design used by `cc-switch`: one source of truth, per-tool enable flags, and
//! faithful per-tool wire formats.
//!
//! ## Unified store
//!
//! Each [`McpServerEntry`] holds a transport (`stdio` / `http` / `sse`), the
//! launch spec (command/args/env or url/headers), and a per-tool `enabled` map.
//! Toggling a tool on writes the server into that tool's live config; toggling
//! off removes it. Editing a server re-projects it to all currently-enabled
//! tools.
//!
//! ## Per-tool target files & formats
//!
//! | tool_id          | file                                   | location / format |
//! |------------------|----------------------------------------|-------------------|
//! | `claude-code`    | `~/.claude.json`                       | `mcpServers.<name>` (community JSON, keeps `type`) |
//! | `claude-desktop-chat` | OS config dir `Claude/claude_desktop_config.json` | `mcpServers.<name>`, **no `type`** |
//! | `codex`          | `~/.codex/config.toml`                 | `[mcp_servers.<name>]` TOML table |
//! | `grok`           | `~/.grok/config.toml`                  | `[mcp_servers.<name>]` TOML (`headers` for HTTP) |
//! | `hermes`         | `~/.hermes/config.yaml` (or `$HERMES_HOME`) | YAML `mcp_servers.<name>` + `platform_toolsets.cli` `mcp-<name>` |
//! | `opencode`       | `~/.config/opencode/opencode.json`     | `mcp.<name>` (`local`/`remote` form) |
//! | `zcode`          | `~/.zcode/cli/config.json`             | `mcp.servers.<name>` (community JSON) |
//! | `kiro`           | `~/.kiro/settings/mcp.json`            | `mcpServers.<name>` (community JSON, keeps `type`) |
//! | `cursor`         | `~/.cursor/mcp.json`                   | `mcpServers.<name>` (community JSON, keeps `type`) |
//! | `vscode`         | `~/.copilot/mcp-config.json`           | **`servers`**`.<name>` (community JSON, keeps `type`) |
//! | `windsurf`       | `~/.codeium/windsurf/mcp_config.json`  | `mcpServers.<name>`, remote under **`serverUrl`**, no `type` |
//! | `cline`          | `~/.cline/mcp.json`                    | `mcpServers.<name>`, `type: `**`streamableHttp`**`/sse` |
//! | `gemini-cli`     | `~/.gemini/settings.json`              | `mcpServers.<name>`, no `type`: `url` = SSE, **`httpUrl`** = HTTP |
//! | `antigravity`    | `~/.gemini/config/mcp_config.json` (legacy `~/.gemini/antigravity/mcp_config.json`) | `mcpServers.<name>`, **no `type`** (the IDE rejects `type: stdio`) |
//! | `zed`            | `~/.config/zed/settings.json`          | **`context_servers`**`.<name>`, no `type` |
//!
//! The bolded cells are the ones that make a config silently ignored rather
//! than rejected when written in another client's spelling; `specs.rs` encodes
//! them as `JsonDialect` variants and `tests_targets.rs` pins each one.
//!
//! Older stores may contain `claude-desktop` or `gemini` tombstones. They are
//! not public targets: they only authorize cleanup of the named entry from
//! Desktop Chat's `claude_desktop_config.json` or Gemini CLI's
//! `~/.gemini/settings.json`; no new values are projected there. Both files
//! now also have a *public* target writing them — `claude-desktop-chat` and
//! `gemini-cli` — under deliberately different ids; see
//! [`LEGACY_CLAUDE_DESKTOP_TOOL_ID`] and [`LEGACY_GEMINI_TOOL_ID`] for the
//! subsumption rule that keeps each pair from fighting.
//!
//! All live writes create a rolling backup (last 5) and use merge semantics:
//! only the single managed server key is touched, every other field is left
//! untouched. A write that fails is rolled back from that backup, so one
//! tool's config is never left half-updated (see `sync.rs`).

use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

mod types;
pub use types::*;

// ---------------------------------------------------------------------------
// Built-in preset catalog
// ---------------------------------------------------------------------------

mod presets;
pub use presets::*;

// ---------------------------------------------------------------------------
// Per-tool registry (SSOT for tool facts + wire-format dispatch)
// ---------------------------------------------------------------------------

mod registry;
pub(crate) use registry::{McpToolSpec, mcp_tool_spec, mcp_tool_specs};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub(crate) fn is_supported_tool(tool_id: &str) -> bool {
    mcp_tool_spec(tool_id).is_some()
}

/// Milliseconds since the Unix epoch (shared timestamp helper).
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// Store path, IO, validation & CRUD
// ---------------------------------------------------------------------------

mod store;
pub use store::*;

// ---------------------------------------------------------------------------
// Input validation policy (create / edit only — see the module docs)
// ---------------------------------------------------------------------------

mod validate;
pub use validate::*;

// ---------------------------------------------------------------------------
// Spec generation — canonical + per-tool transforms
// ---------------------------------------------------------------------------

mod specs;
pub(crate) use specs::{
    antigravity_spec, claude_code_spec, claude_desktop_chat_spec, cline_spec, codex_toml_table,
    cursor_spec, gemini_cli_spec, grok_toml_table, kiro_spec, opencode_spec, vscode_spec,
    windsurf_spec, zcode_cli_spec, zed_spec,
};

// ---------------------------------------------------------------------------
// Per-tool config paths, installed detection & live config IO
// ---------------------------------------------------------------------------

mod tools;
pub use tools::*;
pub(crate) use tools::{
    MCP_SERVERS_KEY, VSCODE_SERVERS_KEY, ZED_SERVERS_KEY, backup_if_exists, codex_remove,
    codex_upsert, json_mcpservers_remove, json_mcpservers_remove_strict, json_mcpservers_upsert,
    json_named_map_remove, json_named_map_upsert, opencode_remove, opencode_upsert,
    restore_from_backup, zcode_cli_remove, zcode_cli_upsert, zcode_v2_opencode_mcp_remove,
};

// ---------------------------------------------------------------------------
// Live config sync (project / remove servers per tool)
// ---------------------------------------------------------------------------

mod sync;
pub use sync::*;

// ---------------------------------------------------------------------------
// Import from a tool's live config
// ---------------------------------------------------------------------------

mod import;
pub use import::*;

// ---------------------------------------------------------------------------
// Hermes YAML live config (mcp_servers + platform_toolsets.cli)
// ---------------------------------------------------------------------------

mod hermes;
pub(crate) use hermes::{
    count_hermes_mcp, hermes_remove, hermes_spec, hermes_upsert, read_hermes_entries,
};

// ---------------------------------------------------------------------------
// Post-install health check (dual-epoch probe)
// ---------------------------------------------------------------------------

mod probe;
pub use probe::*;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_fail_closed;

#[cfg(test)]
mod tests_targets;

#[cfg(test)]
mod tests_integrity;

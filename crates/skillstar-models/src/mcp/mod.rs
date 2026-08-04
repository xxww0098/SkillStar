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
//! | `codex`          | `~/.codex/config.toml`                 | `[mcp_servers.<name>]` TOML table |
//! | `grok`           | `~/.grok/config.toml`                  | `[mcp_servers.<name>]` TOML (`headers` for HTTP) |
//! | `opencode`       | `~/.config/opencode/opencode.json`     | `mcp.<name>` (`local`/`remote` form) |
//! | `zcode`          | `~/.zcode/cli/config.json`             | `mcp.servers.<name>` (community JSON) |
//! | `kiro`           | `~/.kiro/settings/mcp.json`            | `mcpServers.<name>` (community JSON, keeps `type`) |
//! | `cursor`         | `~/.cursor/mcp.json`                   | `mcpServers.<name>` (community JSON, keeps `type`) |
//!
//! Older stores may contain `claude-desktop` or `gemini` tombstones. They are
//! not public targets: they only authorize cleanup of the named entry from
//! Desktop Chat's `claude_desktop_config.json` or Gemini CLI's
//! `~/.gemini/settings.json`; no new values are projected there.
//!
//! All live writes create a rolling backup (last 5) and use merge semantics:
//! only the single managed server key is touched, every other field is left
//! untouched.

use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

mod types;
pub use types::*;

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
// Spec generation — canonical + per-tool transforms
// ---------------------------------------------------------------------------

mod specs;
pub(crate) use specs::{
    claude_code_spec, codex_toml_table, cursor_spec, grok_toml_table, kiro_spec, opencode_spec,
    zcode_cli_spec,
};

// ---------------------------------------------------------------------------
// Per-tool config paths, installed detection & live config IO
// ---------------------------------------------------------------------------

mod tools;
pub use tools::*;
pub(crate) use tools::{
    backup_if_exists, codex_remove, codex_upsert, json_mcpservers_remove,
    json_mcpservers_remove_strict, json_mcpservers_upsert, opencode_remove, opencode_upsert,
    zcode_cli_remove, zcode_cli_upsert, zcode_v2_opencode_mcp_remove,
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;

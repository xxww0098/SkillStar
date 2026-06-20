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
//! | `claude-desktop` | `claude_desktop_config.json`           | `mcpServers.<name>` (stdio only, no `type`) |
//! | `codex`          | `~/.codex/config.toml`                 | `[mcp_servers.<name>]` TOML table |
//! | `gemini`         | `~/.gemini/settings.json`              | `mcpServers.<name>` (community JSON) |
//! | `opencode`       | `~/.config/opencode/opencode.json`     | `mcp.<name>` (`local`/`remote` form) |
//! | `zcode`          | `~/.zcode/cli/config.json`             | `mcp.servers.<name>` (community JSON) |
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
// Shared helpers
// ---------------------------------------------------------------------------

pub(crate) fn is_supported_tool(tool_id: &str) -> bool {
    MCP_TOOL_IDS.contains(&tool_id)
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
pub(crate) use specs::{canonical_spec, claude_desktop_spec, codex_toml_table, opencode_spec, zcode_cli_spec};

// ---------------------------------------------------------------------------
// Per-tool config paths, installed detection & live config IO
// ---------------------------------------------------------------------------

mod tools;
pub use tools::*;
pub(crate) use tools::{
    backup_if_exists, codex_remove, codex_upsert, json_mcpservers_remove, json_mcpservers_upsert,
    opencode_remove, opencode_upsert, zcode_cli_remove, zcode_cli_upsert,
    zcode_v2_opencode_mcp_remove,
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

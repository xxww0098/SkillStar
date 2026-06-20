//! Tool configuration sync module.
//!
//! Writes provider settings to external tool config files (Claude Code, Codex).
//! Only supports hardcoded known config paths for security.
//!
//! ## Flat Store Sync (v2)
//!
//! The flat store sync functions write provider credentials from `ProviderEntryFlat`
//! to external tool config files:
//! - Claude Code: `~/.claude/settings.json` env block (ANTHROPIC_BASE_URL, ANTHROPIC_AUTH_TOKEN, ANTHROPIC_MODEL)
//! - Codex: `~/.codex/auth.json` (OPENAI_API_KEY) + `~/.codex/config.toml` (model_provider, model, [model_providers.skillstar])
//!
//! All writes use rolling backups (keep last 5) and merge semantics (preserve non-managed fields).

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::providers::{
    FlatProvidersStore, ModelCatalogEntry, ProviderEntry, ProviderEntryFlat, ProviderSettings,
    ToolActivation, catalog_from_meta,
};

mod types;
pub use types::*;

mod conflicts;
pub use conflicts::*;

mod backup_merge;
pub use backup_merge::*;

mod paths_files;
pub use paths_files::*;

mod sync;
pub use sync::*;

// ---------------------------------------------------------------------------
// Sandboxed home resolution (single source of truth for tool-config paths)
// ---------------------------------------------------------------------------

/// Env var that re-roots every tool-config path under a sandbox directory.
///
/// When set to a non-empty path, resolution of `~/.claude`, `~/.codex`,
/// `~/.gemini`, `~/.config/opencode`, etc. happens *inside that directory*
/// instead of the user's real home. Tests MUST set this so the suite never
/// overwrites a developer's live tool configuration (a real bug we hit:
/// `resync_active_tools` tests clobbered `~/.codex/config.toml` and
/// `~/.claude/settings.json`). It also lets advanced users sandbox sync.
pub const TOOL_SYNC_HOME_ENV: &str = "SKILLSTAR_TOOL_SYNC_HOME";

/// The sandbox root: [`TOOL_SYNC_HOME_ENV`] if set, otherwise — in this crate's
/// own unit tests only — a per-process throwaway temp dir. The `cfg(test)`
/// fallback is a hard safety net: it guarantees a unit test can NEVER resolve
/// the developer's real `~/.codex`/`~/.claude` even if it forgot to set the
/// override (a recurring footgun). Integration tests compile this lib in
/// non-test mode, so they must set [`TOOL_SYNC_HOME_ENV`] explicitly.
fn sandbox_home() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os(TOOL_SYNC_HOME_ENV) {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    #[cfg(test)]
    {
        return Some(test_sandbox_home());
    }
    #[cfg(not(test))]
    None
}

/// Per-process throwaway home used as the unit-test default (see [`sandbox_home`]).
#[cfg(test)]
fn test_sandbox_home() -> PathBuf {
    use std::sync::LazyLock;
    static DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        let dir =
            std::env::temp_dir().join(format!("skillstar-toolsync-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    });
    DIR.clone()
}

/// Resolve the home directory for tool-config sync — the **single** home
/// resolution point for this module.
///
/// Honors [`TOOL_SYNC_HOME_ENV`]. Do not call `dirs::home_dir()` directly
/// elsewhere in this file: funnelling every path through here is what makes the
/// whole module sandboxable and keeps tests off the real `~/.codex` etc.
fn sync_home_dir() -> Result<PathBuf> {
    if let Some(dir) = sandbox_home() {
        return Ok(dir);
    }
    dirs::home_dir().context("Could not determine home directory")
}

/// `Option`-returning variant of [`sync_home_dir`] for callers that propagate
/// `Option` rather than `Result`.
fn sync_home_dir_opt() -> Option<PathBuf> {
    sandbox_home().or_else(dirs::home_dir)
}

/// Resolve the OS config directory (`~/Library/Application Support`, `%APPDATA%`,
/// or `~/.config`) for tool-config sync, re-rooted under the sandbox when
/// [`TOOL_SYNC_HOME_ENV`] is set.
fn sync_config_dir() -> Result<PathBuf> {
    if let Some(dir) = sandbox_home() {
        return Ok(dir.join(".config"));
    }
    dirs::config_dir().context("Could not determine config directory")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;

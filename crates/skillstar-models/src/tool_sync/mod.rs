//! Tool configuration sync module.
//!
//! Writes provider credentials from the v4 store (`Provider` / `AgentBinding`)
//! to external tool config files. Only supports hardcoded known config paths for
//! security. For example:
//! - Claude Code: `~/.claude/settings.json` env block (ANTHROPIC_BASE_URL, ANTHROPIC_AUTH_TOKEN, ANTHROPIC_MODEL)
//! - Codex: `~/.codex/auth.json` (OPENAI_API_KEY) + `~/.codex/config.toml` (model_provider, model, [model_providers.skillstar])
//!
//! All writes use rolling backups (keep last 5) and merge semantics (preserve non-managed fields).

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::providers::{
    AgentBinding, BindingEntry, DroppedRole, ModelCatalogEntry, ModelRef, Provider,
    ProvidersStoreV4, RequiredWire, RoleCapability, RoleDef, RoleDropReason, catalog_cache,
};
use crate::providers::{ROLE_DEFAULT, ROLE_FAST, ROLE_PLAN, ROLE_SUBAGENT, ROLE_VISION};

mod view;
use view::*;

mod agents;
pub use agents::*;

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

mod multi_provider;
pub use multi_provider::*;

mod omp_provider;
pub use omp_provider::*;

mod migrate_configs;
pub use migrate_configs::*;

// ---------------------------------------------------------------------------
// Sandboxed home resolution (single source of truth for tool-config paths)
// ---------------------------------------------------------------------------

/// Env var that re-roots every tool-config path under a sandbox directory.
///
/// When set to a non-empty path, resolution of `~/.claude`, `~/.codex`,
/// `~/.config/opencode`, etc. happens *inside that directory*
/// instead of the user's real home. Tests MUST set this so the suite never
/// overwrites a developer's live tool configuration (a real bug we hit:
/// `resync_active_tools` tests clobbered `~/.codex/config.toml` and
/// `~/.claude/settings.json`). It also lets advanced users sandbox sync.
pub const TOOL_SYNC_HOME_ENV: &str = "SKILLSTAR_TOOL_SYNC_HOME";

/// The sandbox root: a per-test override if one is installed (unit tests only),
/// else [`TOOL_SYNC_HOME_ENV`] if set, otherwise — in this crate's
/// own unit tests only — a per-process throwaway temp dir. The `cfg(test)`
/// fallback is a hard safety net: it guarantees a unit test can NEVER resolve
/// the developer's real `~/.codex`/`~/.claude` even if it forgot to set the
/// override (a recurring footgun). Integration tests compile this lib in
/// non-test mode, so they must set [`TOOL_SYNC_HOME_ENV`] explicitly.
fn sandbox_home() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(dir) = test_home_override() {
        return Some(dir);
    }
    if let Some(v) = std::env::var_os(TOOL_SYNC_HOME_ENV)
        && !v.is_empty()
    {
        return Some(PathBuf::from(v));
    }
    #[cfg(test)]
    {
        Some(test_sandbox_home())
    }
    #[cfg(not(test))]
    None
}

// Per-*test* sandbox home, taking precedence over `TOOL_SYNC_HOME_ENV`.
//
// libtest runs each test on its own thread, so a thread-local override hands
// every test a private home *without* mutating the process-global env var —
// which parallel tests would stomp on each other. Sharing one sandbox is what
// made `~/.codex/auth.json` tests race (see `tests::use_sandbox_home`).
#[cfg(test)]
thread_local! {
    static TEST_HOME_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Install (or clear) this thread's sandbox home, returning the previous value.
#[cfg(test)]
fn set_test_home_override(dir: Option<PathBuf>) -> Option<PathBuf> {
    TEST_HOME_OVERRIDE.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), dir))
}

#[cfg(test)]
fn test_home_override() -> Option<PathBuf> {
    TEST_HOME_OVERRIDE.with(|slot| slot.borrow().clone())
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
pub(crate) fn sync_home_dir() -> Result<PathBuf> {
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

/// Read an upstream CLI's own home override (`CODEX_HOME`, `GROK_HOME`, …).
///
/// Callers must consult [`sandbox_home`] **first**: the sandbox is a safety
/// net against tests writing a developer's live credentials, and an exported
/// `CODEX_HOME` in the developer's shell must not punch through it.
fn upstream_home_override(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Resolve the OS config directory (`~/Library/Application Support`, `%APPDATA%`,
/// or `~/.config`) while keeping legacy MCP compatibility writes sandboxable.
pub(crate) fn sync_config_dir() -> Result<PathBuf> {
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

// ═══════════════════════════════════════════════════════════════════
//  Domain modules
// ═══════════════════════════════════════════════════════════════════

pub mod ai;
pub mod config;
pub mod git;
pub mod infra;
pub mod projects;
pub mod terminal;

// ═══════════════════════════════════════════════════════════════════
//  Application core
// ═══════════════════════════════════════════════════════════════════

pub mod acp_client;
pub mod ai_provider;
pub mod app_shell;
pub mod lockfile;
pub mod marketplace;
pub mod marketplace_snapshot;
pub mod model_config;
pub mod path_env;
pub mod patrol;
pub mod project_manifest;
pub mod security_scan;
pub mod skill;
pub mod skills;
pub mod terminal_backend;
pub mod translation_api;
pub mod update_checker;

// ── Public API re-exports (Tauri commands / CLI) ─────────────────────

#[allow(unused_imports)]
pub use skills::installed_skill;
#[allow(unused_imports)]
pub use skills::local_skill;
#[allow(unused_imports)]
pub use skills::repo_scanner;
#[allow(unused_imports)]
pub use skills::skill_bundle;
#[allow(unused_imports)]
pub use skills::skill_group;
#[allow(unused_imports)]
pub use skills::skill_install;
#[allow(unused_imports)]
pub use skills::skill_pack;
#[allow(unused_imports)]
pub use skills::skill_update;

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Acquire the test environment lock, recovering from poisoned state.
///
/// When a test panics while holding the lock, `Mutex::lock()` returns
/// `PoisonError` for all subsequent callers. This helper transparently
/// recovers by calling `into_inner()`, preventing cascading failures
/// across the entire test suite.
#[cfg(test)]
pub(crate) fn lock_test_env() -> std::sync::MutexGuard<'static, ()> {
    test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

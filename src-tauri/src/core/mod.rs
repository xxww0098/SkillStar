// ═══════════════════════════════════════════════════════════════════
//  Tauri-specific glue modules
// ═══════════════════════════════════════════════════════════════════

pub mod app_shell;
pub mod channel_auto_update;
pub mod dock_menu;
pub mod github_auth;
pub mod marketplace_snapshot;
pub mod patrol;

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Acquire the test environment lock, recovering from poisoned state.
#[cfg(test)]
pub(crate) fn lock_test_env() -> std::sync::MutexGuard<'static, ()> {
    test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

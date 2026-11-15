//! Reusable Git operations for SkillStar.
//!
//! Provides clone, fetch, sparse-checkout, tree-hash, and shallow-fetch
//! helpers that are agnostic to the caller's application context.
//!
//! Owned by `ss-git`. `ss-skills::git::gh_manager` stays in
//! `ss-skills` because it is coupled to content/lockfile/shared_channels.

pub mod ops;
pub mod repo_history;
pub mod transport;
mod tree;

#[cfg(test)]
mod transport_tests;

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
pub(crate) fn lock_test_env() -> std::sync::MutexGuard<'static, ()> {
    test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

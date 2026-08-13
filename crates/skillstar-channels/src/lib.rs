//! Shared channels + patrol (split out of `skillstar-skills`).
//!
//! Owns the organization shared-channel domain (GitHub REST orchestration,
//! permission projection, versioned descriptors, local registries, member/
//! invitation facades, registration sessions, immutable release manifests,
//! publish sessions, subscription stores, precise publication installs,
//! per-skill channel upgrade transactions and auto-upgrade policy) and the
//! patrol check/collect logic that inspects channel-managed skills.
//!
//! This crate consumes `skillstar-skills` lifecycle primitives through the
//! injected operation-level Git scanner/installer/updater seams. It registers
//! the [`policy::ChannelAwarePolicy`] mutation gate via
//! [`policy::install_global_policy`] from application composition roots.

pub mod patrol;
pub mod policy;
pub mod shared_channels;

/// Crate-wide serialization point for tests that mutate process-global env
/// vars (`SKILLSTAR_HUB_DIR`, `SKILLSTAR_DATA_DIR`, `HOME`, …).
///
/// This is an *async-aware* mutex on purpose: several `#[tokio::test]` cases
/// must keep the guard held across `.await`, because the awaited installer
/// work resolves its paths from exactly those env vars. A `std::sync::Mutex`
/// guard cannot legally cross an await point, so the async and blocking
/// acquire paths below share one `tokio::sync::Mutex` instead — which keeps
/// sync and async tests mutually exclusive with each other.
#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static tokio::sync::Mutex<()> {
    use std::sync::OnceLock;
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Acquire the env lock from a synchronous `#[test]` body.
#[cfg(test)]
pub(crate) fn lock_test_env() -> tokio::sync::MutexGuard<'static, ()> {
    test_env_lock().blocking_lock()
}

/// Acquire the env lock from an `async` test body; the returned guard may be
/// held across await points.
#[cfg(test)]
pub(crate) async fn lock_test_env_async() -> tokio::sync::MutexGuard<'static, ()> {
    test_env_lock().lock().await
}

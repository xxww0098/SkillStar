//! Shared helpers for tests that redirect process-wide storage roots
//! (`SKILLSTAR_DATA_DIR`, `SKILLSTAR_TOOL_SYNC_HOME`, …).
//!
//! Every test that mutates these env vars must serialize on [`ENV_LOCK`] —
//! a single crate-wide lock is the only way tests from different modules
//! cannot swap each other's storage root mid-flight. [`EnvGuard`] acquires
//! the lock itself and restores every variable on drop; [`crate::test_env_lock`]
//! hands out the same mutex so the fetcher-side guards serialize against
//! the switch-engine tests too.
//!
//! The lock is a `std` mutex held inside guard structs (not as a bare local)
//! so it can be held across `await` points in `current_thread` tests without
//! tripping clippy's `await_holding_lock`.

use std::path::Path;

pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Acquire the crate-wide env lock without changing any variables — for
/// tests that only need serialization against env-mutating tests.
pub(crate) fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

pub(crate) struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    pub(crate) fn set(values: &[(&'static str, &Path)]) -> Self {
        let _lock = lock_env();
        let saved = values
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        for (key, value) in values {
            // SAFETY: the only test in this process that mutates these roots
            // holds ENV_LOCK until this guard restores every value.
            unsafe { std::env::set_var(key, value) };
        }
        Self { _lock, saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            // SAFETY: see EnvGuard::set; ENV_LOCK is still held while drop runs.
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}
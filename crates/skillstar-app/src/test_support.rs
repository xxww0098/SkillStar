//! Shared helpers for tests that redirect process-wide storage roots
//! (`SKILLSTAR_DATA_DIR`, `SKILLSTAR_TOOL_SYNC_HOME`, …).
//!
//! Every test module that mutates these env vars must hold [`ENV_LOCK`] while
//! an [`EnvGuard`] is alive — a single crate-wide lock is the only way tests
//! from different modules cannot swap each other's storage root mid-flight.

use std::path::Path;

pub(crate) static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) struct EnvGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    pub(crate) fn set(values: &[(&'static str, &Path)]) -> Self {
        let saved = values
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        for (key, value) in values {
            // SAFETY: the only test in this process that mutates these roots
            // holds ENV_LOCK until this guard restores every value.
            unsafe { std::env::set_var(key, value) };
        }
        Self { saved }
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

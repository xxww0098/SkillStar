//! Shared GitHub REST rate-limit cooldown.
//!
//! `api.github.com` allows 60 unauthenticated requests per hour per IP and
//! every consumer shares that budget. When any consumer hits a rate-limit
//! response it records the reset deadline in
//! `state/skills/github_api_cooldown.json`; until the deadline passes, other
//! consumers (skill update checks, the release check) skip the API entirely.
//! Consumers that see a 403 without response headers record a conservative
//! one-hour deadline instead.

use serde::{Deserialize, Serialize};

use crate::infra::fs_ops::atomic_write;
use crate::infra::paths::github_api_cooldown_path;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cooldown {
    reset_unix: u64,
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

pub fn active(now: u64) -> bool {
    std::fs::read_to_string(github_api_cooldown_path())
        .ok()
        .and_then(|content| serde_json::from_str::<Cooldown>(&content).ok())
        .is_some_and(|cooldown| cooldown.reset_unix > now)
}

pub fn record(reset_unix: u64) {
    let path = github_api_cooldown_path();
    let Ok(content) = serde_json::to_string(&Cooldown { reset_unix }) else {
        return;
    };
    if let Err(error) = atomic_write(&path, content.as_bytes()) {
        tracing::warn!(target: "github_api_cooldown", path = %path.display(), "unable to record the GitHub rate-limit cooldown: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes env mutation and keeps `SKILLSTAR_DATA_DIR` pointed at a
    /// temp dir for the test's duration, restoring the previous value after.
    struct Sandbox {
        _temp: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
    }

    impl Sandbox {
        fn new() -> Self {
            let guard = crate::config::test_env_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let temp = tempfile::TempDir::new().unwrap();
            let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe {
                std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
            }
            Self {
                _temp: temp,
                _guard: guard,
                previous,
            }
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
                None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
            }
        }
    }

    #[test]
    fn record_then_wait_out_the_cooldown() {
        let _sandbox = Sandbox::new();
        record(1_000);
        assert!(active(999));
        assert!(!active(1_000));
    }

    #[test]
    fn missing_file_is_not_active() {
        let _sandbox = Sandbox::new();
        assert!(!active(0));
    }
}

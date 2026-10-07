//! One env sandbox for tests that touch Skill storage.
//!
//! Every variable that can route a write into the developer's real home is set
//! (or cleared) together and restored on drop, while holding the crate-wide
//! env lock so env-mutating tests stay serialized.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(crate) struct Sandbox {
    previous: Vec<(&'static str, Option<OsString>)>,
    temp: tempfile::TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Sandbox {
    /// Data and hub roots re-rooted under a temp dir (the usual dev/test layout).
    pub(crate) fn new() -> Self {
        Self::build(true)
    }

    /// Production layout: only `HOME`/`USERPROFILE` point at a temp dir and the
    /// data/hub overrides are cleared, so the canonical root is the real-shape
    /// `$HOME/.skillstar/data/skills/installed`.
    pub(crate) fn production() -> Self {
        Self::build(false)
    }

    fn build(isolated_roots: bool) -> Self {
        let guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let roots = |path: &str| isolated_roots.then(|| temp.path().join(path));
        let overrides: Vec<(&'static str, Option<PathBuf>)> = vec![
            ("HOME", Some(home.clone())),
            ("USERPROFILE", Some(home.clone())),
            ("SKILLSTAR_DATA_DIR", roots("data")),
            ("SKILLSTAR_HUB_DIR", roots("hub")),
            (
                "SKILLSTAR_TOOL_SYNC_HOME",
                Some(temp.path().join("tool-home")),
            ),
            ("XDG_STATE_HOME", None),
            ("XDG_CONFIG_HOME", Some(home.join(".config"))),
            ("CLAUDE_CONFIG_DIR", None),
            ("CODEX_HOME", None),
            // Other Agent homes that would otherwise point a sweep or deploy
            // at the developer's real directories.
            ("AUTOHAND_HOME", None),
            ("DSH_HOME", None),
            ("GROK_HOME", None),
            ("HERMES_HOME", None),
            ("VIBE_HOME", None),
        ];
        let previous = overrides
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        unsafe {
            for (key, value) in &overrides {
                match value {
                    Some(path) => std::env::set_var(key, path),
                    None => std::env::remove_var(key),
                }
            }
        }
        reset_caches();
        Self {
            previous,
            temp,
            _guard: guard,
        }
    }

    pub(crate) fn home(&self) -> PathBuf {
        self.temp.path().join("home")
    }

    pub(crate) fn root(&self) -> &Path {
        self.temp.path()
    }

    /// Enable an Agent profile the way the Settings switch does.
    pub(crate) fn enable_agent(&self, id: &str) {
        if !crate::agents::list_profiles()
            .iter()
            .any(|profile| profile.id == id && profile.enabled)
        {
            crate::agents::toggle_profile(id).unwrap();
        }
        reset_caches();
    }
}

fn reset_caches() {
    crate::deployment::invalidate_profile_cache();
    crate::installed_skill::invalidate_cache();
    crate::update_state::reset_for_test();
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        reset_caches();
        unsafe {
            for (key, previous) in self.previous.drain(..).rev() {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

//! One-time cleanup of the pre-D-081 install model.
//!
//! Removes the legacy persistent hub (`~/.skillstar/hub/{skills,repos}`), the
//! v5 `lock.json`, and every agent link that pointed into it — the links would
//! dangle once the cache is gone. Idempotent via a state marker; skipped
//! entirely whenever path overrides are active (dev/test sandboxes must never
//! touch another root's data). Project-scope links are left to the projects
//! domain's own stale reconciliation.

use std::path::Path;

use skillstar_core::infra::paths;

fn marker_path() -> std::path::PathBuf {
    paths::state_dir().join("agents-migration-done")
}

fn overrides_active() -> bool {
    ["SKILLSTAR_HUB_DIR", "SKILLSTAR_DATA_DIR"]
        .iter()
        .any(|key| {
            std::env::var(key)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        })
}

/// Run the cleanup once per machine. Safe to call from every entry point
/// (Tauri setup, CLI main): the marker makes repeats no-ops.
pub fn run_once() {
    if overrides_active() || marker_path().exists() {
        return;
    }
    let legacy = paths::legacy_hub_root();
    let mut removed_links = 0usize;
    // 1. Agent links whose target resolves into the legacy hub dangle once the
    //    hub is deleted — remove them first, while the target still resolves.
    for profile in crate::agents::list_profiles() {
        if !profile.has_global_skills() {
            continue;
        }
        removed_links += remove_links_into(&profile.global_skills_dir, &legacy);
    }
    // 2. The legacy skills farm and repo cache.
    for dir in ["skills", "repos", "content"] {
        let _ = fs_remove_dir_all(&legacy.join(dir));
    }
    // 3. The v5 lockfile (and its cross-process lock residue).
    let _ = std::fs::remove_file(legacy.join("lock.json"));
    let _ = std::fs::remove_file(paths::data_root().join("state/lockfile.lock"));

    if removed_links > 0 {
        tracing::info!(
            target: "legacy_cleanup",
            links = removed_links,
            "removed agent links into the pre-D-081 hub; reinstall skills to restore them"
        );
    }
    let _ = std::fs::create_dir_all(paths::state_dir());
    let _ = std::fs::write(marker_path(), chrono::Utc::now().to_rfc3339());
}

/// Remove link entries under `dir` whose resolved target sits inside `root`.
fn remove_links_into(dir: &Path, root: &Path) -> usize {
    let Ok(root_canonical) = std::fs::canonicalize(root) else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !skillstar_core::infra::fs_ops::is_link(&path) {
            continue;
        }
        let points_into_root = skillstar_core::infra::fs_ops::read_link_resolved(&path)
            .ok()
            .and_then(|target| std::fs::canonicalize(&target).ok())
            .is_some_and(|target| target.starts_with(&root_canonical));
        if points_into_root
            && skillstar_core::infra::fs_ops::remove_symlink(&path).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

fn fs_remove_dir_all(path: &Path) -> std::io::Result<()> {
    match path.symlink_metadata() {
        Ok(_) => skillstar_core::infra::fs_ops::remove_dir_all_retry(path),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sandbox {
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
        _temp: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Sandbox {
        fn new() -> Self {
            let _guard = crate::lock_test_env();
            let temp = tempfile::tempdir().unwrap();
            let overrides = [
                ("SKILLSTAR_DATA_DIR", temp.path().join("data")),
                ("SKILLSTAR_TOOL_SYNC_HOME", temp.path().join("tool-home")),
                ("HOME", temp.path().join("home")),
                ("USERPROFILE", temp.path().join("home")),
            ];
            let previous = overrides
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            unsafe {
                for (key, value) in overrides {
                    std::env::set_var(key, value);
                }
                std::env::remove_var("SKILLSTAR_HUB_DIR");
            }
            Self {
                previous,
                _temp: temp,
                _guard,
            }
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
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

    #[test]
    fn run_once_is_skipped_under_overrides() {
        let _sandbox = Sandbox::new(); // SKILLSTAR_DATA_DIR is set
        let legacy = paths::legacy_hub_root();
        std::fs::create_dir_all(legacy.join("skills/keep-me")).unwrap();
        run_once();
        assert!(legacy.join("skills/keep-me").exists(), "sandboxed runs never clean");
        assert!(!marker_path().exists());
    }

    #[test]
    fn marker_makes_runs_idempotent() {
        let _sandbox = Sandbox::new();
        std::fs::create_dir_all(marker_path().parent().unwrap()).unwrap();
        std::fs::write(marker_path(), "done").unwrap();
        let legacy = paths::legacy_hub_root();
        std::fs::create_dir_all(legacy.join("skills/keep-me")).unwrap();
        run_once();
        assert!(legacy.join("skills/keep-me").exists());
    }

    #[cfg(unix)]
    #[test]
    fn removes_legacy_hub_links_but_keeps_unrelated_entries() {
        // Direct unit test of the link filter (run_once itself is guarded by
        // overrides in tests).
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("legacy-hub");
        let repo_skill = legacy.join("repos/owner--repo/skills/foo");
        std::fs::create_dir_all(&repo_skill).unwrap();
        let agent_dir = temp.path().join("agent-skills");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::os::unix::fs::symlink(&repo_skill, agent_dir.join("foo")).unwrap();
        std::fs::create_dir_all(temp.path().join("other")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("other"), agent_dir.join("mine")).unwrap();

        let removed = remove_links_into(&agent_dir, &legacy);
        assert_eq!(removed, 1);
        assert!(!agent_dir.join("foo").exists());
        assert!(agent_dir.join("mine").exists(), "unrelated links survive");
    }
}

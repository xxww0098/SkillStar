//! One-time cleanup of the pre-D-081 install model.
//!
//! Removes the legacy persistent hub (`~/.skillstar/hub/{skills,repos}`), the
//! v5 `lock.json`, and every agent link that pointed into its `skills/`,
//! `repos/` or `content/` subtrees — those links would dangle once the cache
//! is gone. Before the lock goes, its git-backed entries are saved as a
//! pending-reinstall list that `health` reports and repair reinstalls. Links
//! into `hub/local` (user content, re-pointed by the v3 `storage_migration`)
//! are out of scope. The completion marker is written only when every step
//! succeeded, so an interrupted run retries. Skipped entirely whenever path
//! overrides are active (dev/test sandboxes must never touch another root's
//! data). Project-scope links are left to the projects domain's own stale
//! reconciliation.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use ss_core::infra::paths;

fn marker_path() -> PathBuf {
    paths::state_dir().join("agents-migration-done")
}

fn pending_path() -> PathBuf {
    paths::state_dir().join("legacy-reinstall.json")
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

/// A Skill the v5 lock recorded that the cleanup removed and that a repair can
/// fetch again from its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyReinstall {
    pub name: String,
    #[serde(alias = "git_url")]
    pub git_url: String,
    #[serde(default, alias = "git_ref", skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    #[serde(
        default,
        alias = "source_folder",
        skip_serializing_if = "Option::is_none"
    )]
    pub source_folder: Option<String>,
}

#[derive(Deserialize)]
struct LegacyLock {
    #[serde(default)]
    skills: Vec<LegacyReinstall>,
}

/// Skills still waiting for the reinstall the cleanup promised.
pub fn pending_reinstalls() -> Vec<LegacyReinstall> {
    std::fs::read_to_string(pending_path())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// Drop `name` from the pending list once it is installed again.
pub(crate) fn forget_pending(name: &str) -> anyhow::Result<()> {
    let mut pending = pending_reinstalls();
    let before = pending.len();
    pending.retain(|entry| entry.name != name);
    if pending.len() != before {
        save_pending(&pending)?;
    }
    Ok(())
}

fn save_pending(pending: &[LegacyReinstall]) -> anyhow::Result<()> {
    let path = pending_path();
    if pending.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(paths::state_dir())?;
    ss_core::infra::fs_ops::atomic_write(&path, &serde_json::to_vec_pretty(pending)?)
        .with_context(|| format!("Failed to write {}", path.display()))
}

/// Git-backed v5 entries not already installed in the canonical root, merged
/// into the pending list. An unreadable lock is an error: deleting it would
/// lose the only record of what was installed.
fn capture_pending(lock: &Path) -> anyhow::Result<()> {
    let content = match std::fs::read_to_string(lock) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("Failed to read the legacy lock"),
    };
    let legacy: LegacyLock =
        serde_json::from_str(&content).context("Failed to parse the legacy lock")?;
    let canonical = paths::hub_skills_dir();
    let mut pending = pending_reinstalls();
    for entry in legacy.skills {
        let installed = crate::materialize::canonical_skill_name(&entry.name)
            .is_ok_and(|folder| canonical.join(folder).symlink_metadata().is_ok());
        if entry.git_url.trim().is_empty()
            || installed
            || pending.iter().any(|known| known.name == entry.name)
        {
            continue;
        }
        pending.push(entry);
    }
    save_pending(&pending)
}

/// Run the cleanup until it completes once. Safe to call from every entry
/// point (GUI setup, CLI main): the marker makes repeats no-ops.
pub fn run_once() {
    if overrides_active() || marker_path().exists() {
        return;
    }
    match run_cleanup() {
        Ok(()) => {
            let _ = std::fs::create_dir_all(paths::state_dir());
            let _ = std::fs::write(marker_path(), chrono::Utc::now().to_rfc3339());
        }
        Err(error) => tracing::warn!(
            target: "legacy_cleanup",
            error = %format!("{error:#}"),
            "legacy hub cleanup incomplete; it will retry on next start"
        ),
    }
}

/// Run the cleanup again even when the marker says it finished (repair
/// found its leftovers). Path overrides still make it a no-op.
pub(crate) fn rerun() -> anyhow::Result<()> {
    if overrides_active() {
        return Ok(());
    }
    run_cleanup()?;
    std::fs::create_dir_all(paths::state_dir())?;
    std::fs::write(marker_path(), chrono::Utc::now().to_rfc3339())?;
    Ok(())
}

fn run_cleanup() -> anyhow::Result<()> {
    let legacy = paths::legacy_hub_root();
    let lock = legacy.join("lock.json");
    capture_pending(&lock)?;

    let mut failures = Vec::new();
    let mut removed_links = 0usize;
    // 1. Agent links whose target resolves into the legacy hub's removed
    //    subtrees dangle once the hub is deleted — remove them first, while
    //    the target still resolves. hub/local targets are re-pointed by
    //    storage_migration and must survive here.
    for profile in crate::agents::list_profiles() {
        if !profile.has_global_skills() {
            continue;
        }
        for dir in ["skills", "repos", "content"] {
            let (removed, failed) =
                remove_links_into(&profile.global_skills_dir, &legacy.join(dir));
            removed_links += removed;
            failures.extend(failed);
        }
    }
    // 2. The legacy skills farm and repo cache.
    for dir in ["skills", "repos", "content"] {
        let path = legacy.join(dir);
        if let Err(error) = fs_remove_dir_all(&path) {
            failures.push(format!("{}: {error}", path.display()));
        }
    }
    // 3. The v5 lockfile (and its cross-process lock residue).
    for path in [lock, paths::data_root().join("state/lockfile.lock")] {
        match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                failures.push(format!("{}: {error}", path.display()));
            }
            _ => {}
        }
    }

    if removed_links > 0 {
        tracing::info!(
            target: "legacy_cleanup",
            links = removed_links,
            "removed agent links into the pre-D-081 hub; repair reinstalls them"
        );
    }
    if !failures.is_empty() {
        anyhow::bail!("Could not remove: {}", failures.join("; "));
    }
    Ok(())
}

/// Remove link entries under `dir` whose resolved target sits inside `root`.
/// Returns how many were removed and the ones that could not be.
fn remove_links_into(dir: &Path, root: &Path) -> (usize, Vec<String>) {
    let mut failed = Vec::new();
    let Ok(root_canonical) = std::fs::canonicalize(root) else {
        return (0, failed);
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, failed);
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !ss_core::infra::fs_ops::is_link(&path) {
            continue;
        }
        let points_into_root = ss_core::infra::fs_ops::read_link_resolved(&path)
            .ok()
            .and_then(|target| std::fs::canonicalize(&target).ok())
            .is_some_and(|target| target.starts_with(&root_canonical));
        if !points_into_root {
            continue;
        }
        match ss_core::infra::fs_ops::remove_symlink(&path) {
            Ok(()) => removed += 1,
            Err(error) => failed.push(format!("{}: {error:#}", path.display())),
        }
    }
    (removed, failed)
}

fn fs_remove_dir_all(path: &Path) -> std::io::Result<()> {
    match path.symlink_metadata() {
        Ok(_) => ss_core::infra::fs_ops::remove_dir_all_retry(path),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_sandbox::Sandbox;

    #[test]
    fn run_once_is_skipped_under_overrides() {
        let _sandbox = Sandbox::new(); // SKILLSTAR_DATA_DIR is set
        let legacy = paths::legacy_hub_root();
        std::fs::create_dir_all(legacy.join("skills/keep-me")).unwrap();
        run_once();
        assert!(
            legacy.join("skills/keep-me").exists(),
            "sandboxed runs never clean"
        );
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

        // A local-skill link (target under hub/local) must survive run_once's
        // scoped removal, which iterates only skills/repos/content.
        let local_skill = legacy.join("local/foo");
        std::fs::create_dir_all(&local_skill).unwrap();
        std::os::unix::fs::symlink(&local_skill, agent_dir.join("mine-local")).unwrap();

        let mut removed = 0;
        for dir in ["skills", "repos", "content"] {
            removed += remove_links_into(&agent_dir, &legacy.join(dir)).0;
        }
        assert_eq!(removed, 1);
        assert!(!agent_dir.join("foo").exists());
        assert!(agent_dir.join("mine").exists(), "unrelated links survive");
        assert!(
            agent_dir.join("mine-local").exists(),
            "links into hub/local are re-pointed by storage_migration, not deleted"
        );
    }

    fn write_legacy_lock(entries: &str) {
        let lock = paths::legacy_hub_root().join("lock.json");
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(lock, format!(r#"{{"version":5,"skills":[{entries}]}}"#)).unwrap();
    }

    #[test]
    fn the_legacy_lock_becomes_a_reinstall_list_before_it_is_deleted() {
        let _sandbox = Sandbox::production();
        write_legacy_lock(
            r#"{"name":"alpha","git_url":"https://github.com/o/r.git","tree_hash":"x","installed_at":"t","source_folder":"skills/alpha"},
               {"name":"mine","git_url":"","tree_hash":"x","installed_at":"t"}"#,
        );
        std::fs::create_dir_all(paths::legacy_hub_root().join("skills/alpha")).unwrap();

        run_once();

        assert!(!paths::legacy_hub_root().join("lock.json").exists());
        assert!(marker_path().exists());
        let pending = pending_reinstalls();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].name, "alpha");
        assert_eq!(pending[0].source_folder.as_deref(), Some("skills/alpha"));

        forget_pending("alpha").unwrap();
        assert!(pending_reinstalls().is_empty());
        assert!(!pending_path().exists());
    }

    #[test]
    fn an_unreadable_legacy_lock_is_kept_and_the_run_retries() {
        let _sandbox = Sandbox::production();
        let lock = paths::legacy_hub_root().join("lock.json");
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::write(&lock, "{ not json").unwrap();

        run_once();

        assert!(
            lock.exists(),
            "the only record of the old installs survives"
        );
        assert!(!marker_path().exists(), "no completion marker on failure");
    }
}

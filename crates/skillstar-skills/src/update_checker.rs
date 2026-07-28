//! Update detection for repo-backed skills.
//!
//! Answers "does this skill have an update?" given a way to find each skill's
//! repo root. Link resolution is not this module's business — [`crate::repo_link`]
//! owns that, and production callers hand its `repo_root_of` in.
//!
//! # Batch workflow
//!
//! For efficiency, update checks follow a two-phase pattern:
//!
//! 1. **Prefetch**: [`prefetch_unique_repos`] deduplicates skills by repo
//!    root and issues one `git fetch` per unique repo.
//! 2. **Compare**: [`check_update_local`] compares `HEAD` vs `origin/HEAD`
//!    without network access.
//!
//! This avoids N redundant fetches when N skills share the same repo.

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::git::ops as git_ops;
use crate::repo_link;
use skillstar_core::config::github_mirror;
use skillstar_core::infra::path_env::command_with_path;

// ── Batch Prefetch ──────────────────────────────────────────────────

/// Pre-fetch unique repo roots for a batch of skill paths.
///
/// Returns the set of repo roots where the fetch **failed**.
pub fn prefetch_unique_repos(skill_paths: &[PathBuf]) -> HashSet<PathBuf> {
    prefetch_unique_repos_with(skill_paths, repo_link::repo_root_of, |root| {
        fetch_tracked_ref(root).map_err(|e| {
            warn!(
                target: "update_checker",
                path = %root.display(),
                error = %e,
                "prefetch git fetch failed — will preserve existing update state"
            );
            e
        })
    })
}

pub fn prefetch_unique_repos_with<F, G>(
    skill_paths: &[PathBuf],
    repo_root_of: F,
    fetch_repo: G,
) -> HashSet<PathBuf>
where
    F: Fn(&Path) -> Option<PathBuf>,
    G: Fn(&Path) -> Result<()>,
{
    let mut fetched = HashSet::new();
    let mut failed = HashSet::new();

    for path in skill_paths {
        if let Some(root) = repo_root_of(path)
            && fetched.insert(root.clone())
            && fetch_repo(&root).is_err()
        {
            failed.insert(root);
        }
    }

    failed
}

// ── Update Detection ────────────────────────────────────────────────

/// Check if a repo-backed skill has updates available **without fetching**.
///
/// `None` means "the prefetch failed for this skill's repo, status unknown" —
/// callers must preserve the previous state rather than clearing the badge.
pub fn check_update_local(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
) -> Option<bool> {
    check_update_local_with(skill_path, failed_fetch_roots, repo_link::repo_root_of)
}

pub fn check_update_local_with<F>(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
    repo_root_of: F,
) -> Option<bool>
where
    F: Fn(&Path) -> Option<PathBuf>,
{
    // No resolvable repo root: nothing to compare against, so report "no
    // update" rather than "unknown". `None` is reserved for a failed fetch,
    // where the previous badge must survive.
    let Some(repo_root) = repo_root_of(skill_path) else {
        return Some(false);
    };
    if failed_fetch_roots.contains(&repo_root) {
        return None;
    }
    Some(compare_heads(&repo_root).unwrap_or(false))
}

fn compare_heads(repo_root: &Path) -> Option<bool> {
    let local_head = git_ops::rev_parse(repo_root, "HEAD").ok()?;
    let remote_ref = if configured_git_ref(repo_root).is_some() {
        "FETCH_HEAD"
    } else {
        "origin/HEAD"
    };
    let remote_head = git_ops::rev_parse(repo_root, remote_ref).ok()?;
    Some(!local_head.is_empty() && !remote_head.is_empty() && local_head != remote_head)
}

pub(crate) fn configured_git_ref(repo_root: &Path) -> Option<String> {
    let output = command_with_path("git")
        .current_dir(repo_root)
        .args(["config", "--get", "skillstar.ref"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub(crate) fn fetch_tracked_ref(repo_root: &Path) -> Result<()> {
    let mut fetch_cmd = command_with_path("git");
    github_mirror::apply_mirror_args(&mut fetch_cmd);
    fetch_cmd
        .current_dir(repo_root)
        .args(["fetch", "--depth", "1", "--quiet"]);
    if let Some(git_ref) = configured_git_ref(repo_root) {
        fetch_cmd.args(["origin", git_ref.as_str()]);
    }
    let output = fetch_cmd.output().map_err(anyhow::Error::from)?;
    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::fs;
    use std::process::Command;

    fn run_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init", "--initial-branch=main"]);
        run_git(dir.path(), &["config", "user.email", "test@example.com"]);
        run_git(dir.path(), &["config", "user.name", "SkillStar Tests"]);
        dir
    }

    // Unix-only: fixtures use std::os::unix::fs::symlink.
    #[cfg(unix)]
    #[test]
    fn subtree_hash_and_local_update_detection_work() {
        let remote = init_repo();
        fs::create_dir_all(remote.path().join("skills/demo")).unwrap();
        fs::write(remote.path().join("skills/demo/SKILL.md"), "v1").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "initial"]);

        let clone_parent = tempfile::tempdir().unwrap();
        let clone_path = clone_parent.path().join("clone");
        run_git(
            clone_parent.path(),
            &[
                "clone",
                remote.path().to_str().unwrap(),
                clone_path.to_str().unwrap(),
            ],
        );

        let initial_hash = git_ops::compute_subtree_hash(&clone_path, "skills/demo").unwrap();
        assert!(!initial_hash.is_empty());

        fs::write(remote.path().join("skills/demo/SKILL.md"), "v2").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "update"]);

        run_git(&clone_path, &["fetch", "--depth", "1", "--quiet"]);

        let skill_link_parent = tempfile::tempdir().unwrap();
        let skill_link = skill_link_parent.path().join("demo");
        std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();

        let result = check_update_local_with(&skill_link, &HashSet::new(), |path| {
            let real = std::fs::read_link(path).ok()?;
            Some(real.parent()?.parent()?.to_path_buf())
        });
        assert_eq!(result, Some(true));
    }

    #[test]
    fn unresolvable_repo_root_reports_no_update() {
        let dir = tempfile::tempdir().unwrap();
        let result = check_update_local_with(&dir.path().join("nope"), &HashSet::new(), |_| None);
        assert_eq!(
            result,
            Some(false),
            "None is reserved for failed fetches, not for missing repo roots"
        );
    }

    #[test]
    fn failed_prefetch_root_preserves_previous_state() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let failed = HashSet::from([repo.clone()]);
        let result = check_update_local_with(&dir.path().join("skill"), &failed, |_| {
            Some(repo.clone())
        });
        assert_eq!(result, None, "failed fetch must not clear the badge");
    }

    #[test]
    fn prefetch_unique_repos_deduplicates_and_tracks_failures() {
        let dir = tempfile::tempdir().unwrap();
        let repo_a = dir.path().join("repo_a");
        let repo_b = dir.path().join("repo_b");
        let skill_a1 = dir.path().join("skill_a1");
        let skill_a2 = dir.path().join("skill_a2");
        let skill_b = dir.path().join("skill_b");

        let repo_a_for_lookup = repo_a.clone();
        let repo_b_for_lookup = repo_b.clone();
        let skill_b_for_lookup = skill_b.clone();
        let repo_root_of = move |path: &Path| -> Option<PathBuf> {
            if path == skill_b_for_lookup {
                Some(repo_b_for_lookup.clone())
            } else {
                Some(repo_a_for_lookup.clone())
            }
        };

        let fetch_calls = std::cell::RefCell::new(Vec::new());
        let fetch_repo = |root: &Path| -> Result<()> {
            fetch_calls.borrow_mut().push(root.to_path_buf());
            if root == repo_b {
                Err(anyhow!("fetch failed"))
            } else {
                Ok(())
            }
        };

        let failed = prefetch_unique_repos_with(
            &[skill_a1, skill_a2, skill_b],
            repo_root_of,
            fetch_repo,
        );

        assert_eq!(
            fetch_calls.borrow().len(),
            2,
            "should fetch only unique repos"
        );
        assert!(fetch_calls.borrow().contains(&repo_a));
        assert!(fetch_calls.borrow().contains(&repo_b));
        assert!(failed.contains(&repo_b));
        assert!(!failed.contains(&repo_a));
    }
}

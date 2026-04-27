//! Update detection for repo-cached skills.
//!
//! Handles the "does this skill have an update?" flow for skills installed
//! from git repositories. All git operations go through `git_ops`.
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
//!
//! # Example
//!
//! ```rust,ignore
//! let failed = prefetch_unique_repos(&skill_paths);
//! for path in &skill_paths {
//!     match check_update_local(path, &failed) {
//!         Some(true)  => println!("update available"),
//!         Some(false) => println!("up to date"),
//!         None        => println!("status unknown (fetch failed)"),
//!     }
//! }
//! ```

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::core::{config::github_mirror, git::ops as git_ops, path_env::command_with_path};

// ── Symlink Resolution Helpers ──────────────────────────────────────

/// Resolve a symlink to its absolute target path.
///
/// If the symlink target is relative, it is resolved relative to the
/// symlink's parent directory.
pub(crate) fn resolve_symlink(link_path: &Path) -> Option<PathBuf> {
    skillstar_core_types::resolve_symlink(link_path)
}

/// Check whether a skill directory is a symlink into the repo cache.
pub fn is_repo_cached_skill(skill_path: &Path) -> bool {
    skillstar_core_types::is_repo_cached_skill(
        skill_path,
        &crate::core::infra::paths::repos_cache_dir(),
    )
}

fn normalize_path_for_compare(path: &Path) -> String {
    skillstar_core_types::normalize_path_for_compare(path)
}

/// Check whether a resolved path lies within the repo cache directory.
///
pub(crate) fn is_repo_cached_skill_target_path(target: &Path) -> bool {
    skillstar_core_types::is_repo_cached_skill_target_path(
        target,
        &crate::core::infra::paths::repos_cache_dir(),
    )
}

/// Resolve a repo-cached skill path to its repository root.
///
/// Returns `None` if the skill is not a repo-cached symlink or resolution fails.
pub fn resolve_skill_repo_root(skill_path: &Path) -> Option<PathBuf> {
    skillstar_core_types::resolve_skill_repo_root(
        skill_path,
        &crate::core::infra::paths::repos_cache_dir(),
        git_ops::find_repo_root,
    )
}

// ── Batch Prefetch ──────────────────────────────────────────────────

/// Pre-fetch unique repo roots for a batch of skill paths.
///
/// Walks the paths, identifies repo-cached skills, resolves their repo roots,
/// and issues a single `git fetch --depth 1` per unique repository. This
/// avoids redundant fetches when multiple skills share the same repo.
///
/// Returns the set of repo roots where the fetch **failed** (e.g. "shallow
/// file has changed since we read it"). Callers should treat skills in
/// failed-fetch repos as "update status unknown" rather than "up-to-date".
pub fn prefetch_unique_repos(skill_paths: &[PathBuf]) -> HashSet<PathBuf> {
    skillstar_core_types::prefetch_unique_repos(
        skill_paths,
        &crate::core::infra::paths::repos_cache_dir(),
        git_ops::find_repo_root,
        |root| {
            git_ops::run_git_shallow_fetch(root, &["fetch", "--depth", "1", "--quiet"])
                .map(|_| ())
                .map_err(|e| {
                    warn!(
                        target: "update_checker",
                        path = %root.display(),
                        error = %e,
                        "prefetch git fetch failed — will preserve existing update state"
                    );
                    e
                })
        },
    )
}

// ── Update Detection ────────────────────────────────────────────────

/// Check if a repo-cached skill has updates available (fetches first).
///
/// For batch checks, prefer [`prefetch_unique_repos`] +
/// [`check_update_local`] to avoid redundant fetches.
#[allow(dead_code)]
pub fn check_update(skill_path: &Path) -> bool {
    skillstar_core_types::check_update(
        skill_path,
        |repo_root| {
            let mut fetch_cmd = command_with_path("git");
            github_mirror::apply_mirror_args(&mut fetch_cmd);
            let output = fetch_cmd
                .current_dir(repo_root)
                .args(["fetch", "--depth", "1", "--quiet"])
                .output()
                .map_err(anyhow::Error::from)?;
            if output.status.success() {
                Ok(())
            } else {
                anyhow::bail!(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        },
        git_ops::find_repo_root,
    )
}

/// Check if a repo-cached skill has updates **without fetching**.
///
/// Call [`prefetch_unique_repos`] first to ensure `origin/HEAD` is up-to-date.
///
/// Returns `None` when the prefetch failed for this skill's repo, signaling
/// "unknown" so callers can preserve the previous cached state.
pub fn check_update_local(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
) -> Option<bool> {
    skillstar_core_types::check_update_local(
        skill_path,
        failed_fetch_roots,
        git_ops::find_repo_root,
    )
}

/// Compare local HEAD vs origin/HEAD, returning true if they differ.
fn compare_heads(repo_root: &Path) -> Option<bool> {
    let local_head = git_rev_parse(repo_root, "HEAD")?;
    let remote_head = git_rev_parse(repo_root, "origin/HEAD")?;
    Some(!local_head.is_empty() && !remote_head.is_empty() && local_head != remote_head)
}

fn git_rev_parse(repo_dir: &Path, rev: &str) -> Option<String> {
    command_with_path("git")
        .current_dir(repo_dir)
        .args(["rev-parse", rev])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Compute the git tree hash for a specific subfolder within a repo.
///
/// Used by `commands::update_skill` to recompute sibling lockfile hashes
/// after a shared repo pull.
#[allow(dead_code)]
pub fn compute_subtree_hash(repo_dir: &Path, folder_path: &str) -> Result<String> {
    skillstar_core_types::compute_subtree_hash(repo_dir, folder_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn app_level_is_repo_cached_skill_uses_repos_cache_dir() {
        let _guard = crate::core::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let repo_cache = crate::core::infra::paths::repos_cache_dir();
        fs::create_dir_all(&repo_cache).unwrap();
        let repo = repo_cache.join("demo-repo");
        fs::create_dir_all(&repo).unwrap();

        let link_parent = tempfile::tempdir().unwrap();
        let link = link_parent.path().join("demo");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&repo, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&repo, &link).unwrap();

        assert!(is_repo_cached_skill(&link));

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}

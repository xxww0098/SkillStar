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

use crate::core::{
    config::github_mirror,
    git::ops as git_ops,
    path_env::command_with_path,
};

// ── Symlink Resolution Helpers ──────────────────────────────────────

/// Resolve a symlink to its absolute target path.
///
/// If the symlink target is relative, it is resolved relative to the
/// symlink's parent directory.
pub(crate) fn resolve_symlink(link_path: &Path) -> Option<PathBuf> {
    crate::core::infra::fs_ops::read_link_resolved(link_path).ok()
}

/// Check whether a skill directory is a symlink into the repo cache.
pub fn is_repo_cached_skill(skill_path: &Path) -> bool {
    if !crate::core::infra::fs_ops::is_link(skill_path) {
        return false;
    }
    let Ok(target) = crate::core::infra::fs_ops::read_link_resolved(skill_path) else {
        return false;
    };
    is_repo_cached_skill_target_path(&target)
}

fn normalize_path_for_compare(path: &Path) -> String {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    #[cfg(windows)]
    {
        normalized.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        normalized
    }
}

/// Check whether a resolved path lies within the repo cache directory.
///
pub(crate) fn is_repo_cached_skill_target_path(target: &Path) -> bool {
    let target_norm = normalize_path_for_compare(target);
    let repo_root_norm = normalize_path_for_compare(&crate::core::infra::paths::repos_cache_dir());
    target_norm == repo_root_norm || target_norm.starts_with(&(repo_root_norm + "/"))
}

/// Resolve a repo-cached skill path to its repository root.
///
/// Returns `None` if the skill is not a repo-cached symlink or resolution fails.
pub fn resolve_skill_repo_root(skill_path: &Path) -> Option<PathBuf> {
    if !is_repo_cached_skill(skill_path) {
        return None;
    }
    let real_path = resolve_symlink(skill_path)?;
    git_ops::find_repo_root(&real_path)
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
    let mut fetched = HashSet::new();
    let mut failed = HashSet::new();
    for path in skill_paths {
        if let Some(root) = resolve_skill_repo_root(path) {
            if fetched.insert(root.clone()) {
                match git_ops::run_git_shallow_fetch(&root, &["fetch", "--depth", "1", "--quiet"]) {
                    Ok(_) => {}
                    Err(e) => {
                        warn!(
                            target: "update_checker",
                            path = %root.display(),
                            error = %e,
                            "prefetch git fetch failed — will preserve existing update state"
                        );
                        failed.insert(root);
                    }
                }
            }
        }
    }
    failed
}

// ── Update Detection ────────────────────────────────────────────────

/// Check if a repo-cached skill has updates available (fetches first).
///
/// For batch checks, prefer [`prefetch_unique_repos`] +
/// [`check_update_local`] to avoid redundant fetches.
#[allow(dead_code)]
pub fn check_update(skill_path: &Path) -> bool {
    let real_path = match resolve_symlink(skill_path) {
        Some(p) => p,
        None => return false,
    };

    let repo_root = match git_ops::find_repo_root(&real_path) {
        Some(root) => root,
        None => return false,
    };

    let mut fetch_cmd = command_with_path("git");
    github_mirror::apply_mirror_args(&mut fetch_cmd);
    let _ = fetch_cmd
        .current_dir(&repo_root)
        .args(["fetch", "--depth", "1", "--quiet"])
        .output();

    compare_heads(&repo_root).unwrap_or(false)
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
    let real_path = match resolve_symlink(skill_path) {
        Some(p) => p,
        None => return Some(false),
    };

    let repo_root = match git_ops::find_repo_root(&real_path) {
        Some(root) => root,
        None => return Some(false),
    };

    if failed_fetch_roots.contains(&repo_root) {
        return None;
    }

    Some(compare_heads(&repo_root).unwrap_or(false))
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
    let output = command_with_path("git")
        .current_dir(repo_dir)
        .args(["rev-parse", &format!("HEAD:{}", folder_path)])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute git rev-parse for subtree: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("git rev-parse failed: {}", err.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

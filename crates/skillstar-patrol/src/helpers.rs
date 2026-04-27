//! Pure helper logic for skill update checking.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

use skillstar_git::ops as git_ops;
use skillstar_skills::repo_scanner;

/// Check a single skill for available updates locally.
///
/// Returns `None` when the repo fetch failed and the caller should skip
/// emitting an update event.
pub fn check_skill_update_local(
    skill_name: &str,
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
) -> Option<bool> {
    if repo_scanner::is_repo_cached_skill(skill_path) {
        return repo_scanner::check_repo_skill_update_local(skill_path, failed_fetch_roots);
    }

    // Fallback for non-repo-cached hub skills.
    let _ = git_ops::ensure_worktree_checked_out(skill_path);
    match git_ops::check_update(skill_path) {
        Ok(update_available) => Some(update_available),
        Err(err) => {
            warn!(
                target: "patrol",
                skill = %skill_name,
                error = %err,
                "check failed"
            );
            Some(false)
        }
    }
}

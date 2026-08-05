use crate::git::ops as git_ops;
use crate::update_checker;
use anyhow::{Context, Result, anyhow};
use skillstar_core::infra::fs_ops;
use std::path::Path;

use super::cache::{discover_skill_dirs_from_tree, is_sparse_checkout};

/// Pull a repo-cached skill's backing repository to the tracked ref and return
/// the tree hash the skill now sits at.
///
/// When the skill declares a `folder_path`, the hash covers only that subtree,
/// so siblings sharing the repo keep their own hashes.
pub(crate) fn pull_repo_skill_update_in_session(
    skill_path: &Path,
    folder_path: Option<&str>,
    session: &crate::git::transport::GitOperationSession,
) -> Result<String> {
    let absolute_path = fs_ops::read_link_resolved(skill_path).context("Skill is not a symlink")?;

    let repo_root = git_ops::find_repo_root(&absolute_path)
        .ok_or_else(|| anyhow!("Cannot find git repo root for symlinked skill"))?;

    update_checker::fetch_tracked_ref_in_session(&repo_root, session)
        .context("Failed to fetch repo-cached update")?;

    let reset_target = if update_checker::configured_git_ref(&repo_root).is_some() {
        "FETCH_HEAD"
    } else {
        "origin/HEAD"
    };
    git_ops::checkout_in_session(&repo_root, &["reset", "--hard", reset_target], session)
        .context("Failed to execute git reset for repo-cached update")?;

    if is_sparse_checkout(&repo_root)
        && let Ok(dirs) = discover_skill_dirs_from_tree(&repo_root)
    {
        if dirs.is_empty() {
            let _ =
                git_ops::checkout_in_session(&repo_root, &["sparse-checkout", "disable"], session);
            let _ = git_ops::checkout_in_session(&repo_root, &["checkout"], session);
        } else {
            let dir_refs: Vec<&str> = dirs.iter().map(|s| s.as_str()).collect();
            let _ = git_ops::apply_sparse_checkout_in_session(&repo_root, &dir_refs, session);
        }
    }

    match folder_path {
        Some(fp) if !fp.is_empty() => git_ops::compute_subtree_hash(&repo_root, fp),
        _ => git_ops::compute_tree_hash(&repo_root),
    }
}

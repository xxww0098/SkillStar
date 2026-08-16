use crate::git::ops as git_ops;
use crate::update_checker;
use crate::{lockfile, repo_link};
use anyhow::{Context, Result, anyhow};
use skillstar_core::infra::fs_ops;
use std::path::Path;

use super::cache::{discover_skill_dirs_from_tree, is_sparse_checkout};

/// Pull a repo-cached skill's backing repository to the tracked ref and return
/// the tree hash the skill now sits at.
///
/// When the skill declares a `folder_path`, the hash covers only that subtree,
/// so siblings sharing the repo keep their own hashes.
pub fn pull_repo_skill_update_in_session(
    skill_path: &Path,
    folder_path: Option<&str>,
    session: &crate::git::transport::GitOperationSession,
) -> Result<String> {
    let absolute_path = fs_ops::read_link_resolved(skill_path).context("Skill is not a symlink")?;

    let repo_root = git_ops::find_repo_root(&absolute_path)
        .ok_or_else(|| anyhow!("Cannot find git repo root for symlinked skill"))?;
    let installed_source_folders = if is_sparse_checkout(&repo_root) {
        installed_source_folders(&repo_root)?
    } else {
        Vec::new()
    };

    update_checker::fetch_tracked_ref_in_session(&repo_root, session)
        .context("Failed to fetch repo-cached update")?;

    // The fetch is a network operation lasting up to seconds; a user edit
    // landing in that window must not be destroyed by the reset below.
    // Fail closed instead — the update caller preserves the worktree.
    if !git_ops::worktree_is_clean(&repo_root)? {
        return Err(git_ops::WorktreeDirty.into());
    }

    let reset_target = if update_checker::configured_git_ref(&repo_root).is_some() {
        "FETCH_HEAD"
    } else {
        "origin/HEAD"
    };
    git_ops::checkout_in_session(&repo_root, &["reset", "--hard", reset_target], session)
        .context("Failed to execute git reset for repo-cached update")?;

    if is_sparse_checkout(&repo_root)
        && let Ok(mut dirs) = discover_skill_dirs_from_tree(&repo_root)
    {
        if dirs.is_empty() {
            let _ =
                git_ops::checkout_in_session(&repo_root, &["sparse-checkout", "disable"], session);
            let _ = git_ops::checkout_in_session(&repo_root, &["checkout"], session);
        } else {
            // Discovery chooses one canonical provider path for each Skill
            // name. That is right for a new install, but an existing install
            // is bound to its lockfile source_folder. Keep every installed
            // source materialized even when the remote adds a higher/equal
            // priority duplicate path; otherwise its Hub link briefly dangles
            // and the update is falsely reported as source removal.
            dirs.extend(installed_source_folders);
            dirs.sort();
            dirs.dedup();
            let dir_refs: Vec<&str> = dirs.iter().map(|s| s.as_str()).collect();
            let _ = git_ops::apply_sparse_checkout_in_session(&repo_root, &dir_refs, session);
        }
    }

    match folder_path {
        Some(fp) if !fp.is_empty() => git_ops::compute_subtree_hash(&repo_root, fp),
        _ => git_ops::compute_tree_hash(&repo_root),
    }
}

fn installed_source_folders(repo_root: &Path) -> Result<Vec<String>> {
    let canonical_repo = std::fs::canonicalize(repo_root)
        .with_context(|| format!("failed to resolve repo cache '{}'", repo_root.display()))?;
    let entries = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .context("failed to read installed Skill sources before updating sparse checkout")?
        .skills;
    let hub = skillstar_core::infra::paths::hub_skills_dir();

    Ok(entries
        .into_iter()
        .filter(|entry| {
            repo_link::repo_root_of(&hub.join(&entry.name))
                .and_then(|root| std::fs::canonicalize(root).ok())
                .is_some_and(|root| root == canonical_repo)
        })
        .filter_map(|entry| entry.source_folder)
        .filter(|folder| !folder.is_empty())
        .collect())
}

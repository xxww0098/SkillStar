//! Repo-root resolution, repo-cached skill updates, and update-checker shims.

use crate::core::{
    config::github_mirror,
    git::ops as git_ops,
    infra::{fs_ops, paths},
    path_env::command_with_path,
};
use crate::core::skills::update_checker;
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

use super::cache::{discover_skill_dirs_from_tree, is_sparse_checkout};
use super::scan_install::compute_subtree_hash;

/// Check whether a skill directory is a symlink into the repo cache.
pub fn is_repo_cached_skill(skill_path: &Path) -> bool {
    if !fs_ops::is_link(skill_path) {
        return false;
    }
    let Ok(target) = fs_ops::read_link_resolved(skill_path) else {
        return false;
    };
    is_repo_cache_target_path(&target)
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

fn is_repo_cache_target_path(target: &Path) -> bool {
    let target_norm = normalize_path_for_compare(target);
    let repo_root_norm = normalize_path_for_compare(&paths::repos_cache_dir());
    target_norm == repo_root_norm || target_norm.starts_with(&(repo_root_norm + "/"))
}

/// Resolve a repo-cached skill path to its repository root.
pub fn resolve_skill_repo_root(skill_path: &Path) -> Option<PathBuf> {
    if !is_repo_cached_skill(skill_path) {
        return None;
    }
    let real_path = fs_ops::read_link_resolved(skill_path).ok()?;
    git_ops::find_repo_root(&real_path)
}

/// Pull updates for a repo-cached skill.
pub fn pull_repo_skill_update(skill_path: &Path, folder_path: Option<&str>) -> Result<String> {
    let absolute_path =
        fs_ops::read_link_resolved(skill_path).context("Skill is not a symlink")?;

    let repo_root = git_ops::find_repo_root(&absolute_path)
        .ok_or_else(|| anyhow!("Cannot find git repo root for symlinked skill"))?;

    if is_shallow_repo(&repo_root) {
        git_ops::run_git_shallow_fetch(&repo_root, &["fetch", "--depth", "1", "--quiet"])
            .context("Failed to fetch repo-cached update (shallow)")?;
    } else {
        let mut fetch_cmd = command_with_path("git");
        github_mirror::apply_mirror_args(&mut fetch_cmd);
        let output = fetch_cmd
            .current_dir(&repo_root)
            .args(["fetch", "--quiet"])
            .output()
            .context("Failed to execute git fetch for repo-cached update")?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("git fetch failed: {}", err.trim()));
        }
    }

    let mut reset_cmd = command_with_path("git");
    github_mirror::apply_mirror_args(&mut reset_cmd);
    let output = reset_cmd
        .current_dir(&repo_root)
        .args(["reset", "--hard", "origin/HEAD"])
        .output()
        .context("Failed to execute git reset for repo-cached update")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git reset failed: {}", err.trim()));
    }

    if is_sparse_checkout(&repo_root) {
        if let Ok(dirs) = discover_skill_dirs_from_tree(&repo_root) {
            if dirs.is_empty() {
                let _ = command_with_path("git")
                    .current_dir(&repo_root)
                    .args(["sparse-checkout", "disable"])
                    .output();
                let mut co_cmd = command_with_path("git");
                github_mirror::apply_mirror_args(&mut co_cmd);
                let _ = co_cmd.current_dir(&repo_root).arg("checkout").output();
            } else {
                let dir_refs: Vec<&str> = dirs.iter().map(|s| s.as_str()).collect();
                let _ = git_ops::apply_sparse_checkout(&repo_root, &dir_refs);
            }
        }
    }

    match folder_path {
        Some(fp) if !fp.is_empty() => compute_subtree_hash(&repo_root, fp),
        _ => git_ops::compute_tree_hash(&repo_root),
    }
}

fn is_shallow_repo(repo_dir: &Path) -> bool {
    repo_dir.join(".git/shallow").exists()
}

/// Pre-fetch unique repo roots for a batch of skill paths.
pub fn prefetch_unique_repos(
    skill_paths: &[PathBuf],
) -> std::collections::HashSet<PathBuf> {
    update_checker::prefetch_unique_repos(skill_paths)
}

/// Check if a repo-cached skill has updates **without fetching**.
pub fn check_repo_skill_update_local(
    skill_path: &Path,
    failed_fetch_roots: &std::collections::HashSet<PathBuf>,
) -> Option<bool> {
    update_checker::check_update_local(skill_path, failed_fetch_roots)
}

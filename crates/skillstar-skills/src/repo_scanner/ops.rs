use crate::git::ops as git_ops;
use crate::update_checker;
use anyhow::{Context, Result, anyhow};
use skillstar_core::config::github_mirror;
use skillstar_core::infra::{fs_ops, path_env::command_with_path, paths};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::cache::{discover_skill_dirs_from_tree, is_sparse_checkout};
use super::scan::compute_subtree_hash;

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

pub fn resolve_skill_repo_root(skill_path: &Path) -> Option<PathBuf> {
    if !is_repo_cached_skill(skill_path) {
        return None;
    }
    let real_path = fs_ops::read_link_resolved(skill_path).ok()?;
    git_ops::find_repo_root(&real_path)
}

pub fn pull_repo_skill_update(skill_path: &Path, folder_path: Option<&str>) -> Result<String> {
    let absolute_path = fs_ops::read_link_resolved(skill_path).context("Skill is not a symlink")?;

    let repo_root = git_ops::find_repo_root(&absolute_path)
        .ok_or_else(|| anyhow!("Cannot find git repo root for symlinked skill"))?;

    update_checker::fetch_tracked_ref(&repo_root).context("Failed to fetch repo-cached update")?;

    let mut reset_cmd = command_with_path("git");
    github_mirror::apply_mirror_args(&mut reset_cmd);
    let reset_target = if update_checker::configured_git_ref(&repo_root).is_some() {
        "FETCH_HEAD"
    } else {
        "origin/HEAD"
    };
    let output = reset_cmd
        .current_dir(&repo_root)
        .args(["reset", "--hard", reset_target])
        .output()
        .context("Failed to execute git reset for repo-cached update")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git reset failed: {}", err.trim()));
    }

    if is_sparse_checkout(&repo_root)
        && let Ok(dirs) = discover_skill_dirs_from_tree(&repo_root)
    {
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

    match folder_path {
        Some(fp) if !fp.is_empty() => compute_subtree_hash(&repo_root, fp),
        _ => git_ops::compute_tree_hash(&repo_root),
    }
}

pub fn prefetch_unique_repos(skill_paths: &[PathBuf]) -> HashSet<PathBuf> {
    // Path-bound ownership lives in update_checker (git fetch + repos_cache_dir).
    update_checker::prefetch_unique_repos(skill_paths)
}

pub fn check_repo_skill_update_local(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
) -> Option<bool> {
    update_checker::check_update_local(skill_path, failed_fetch_roots)
}

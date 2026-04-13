//! Repo cache size accounting and pruning of unreferenced checkouts.

use crate::core::infra::{fs_ops, paths};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::find_repo_root;

/// Information about the repo cache directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoCacheInfo {
    pub total_bytes: u64,
    pub repo_count: usize,
    pub unused_count: usize,
    pub unused_bytes: u64,
}

/// Collect information about the repo cache (`repos/` directory).
pub fn get_cache_info() -> RepoCacheInfo {
    let cache_dir = paths::repos_cache_dir();
    if !cache_dir.exists() {
        return RepoCacheInfo {
            total_bytes: 0,
            repo_count: 0,
            unused_count: 0,
            unused_bytes: 0,
        };
    }

    let hub_skills_dir = paths::hub_skills_dir();
    let referenced = collect_referenced_cache_dirs(&hub_skills_dir, &cache_dir);

    let mut total_bytes: u64 = 0;
    let mut repo_count: usize = 0;
    let mut unused_count: usize = 0;
    let mut unused_bytes: u64 = 0;

    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if fs_ops::is_link(&path) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            let size = dir_size(&path);
            total_bytes += size;
            repo_count += 1;

            if !referenced.contains(&path) {
                unused_count += 1;
                unused_bytes += size;
            }
        }
    }

    RepoCacheInfo {
        total_bytes,
        repo_count,
        unused_count,
        unused_bytes,
    }
}

/// Remove cached repos that are NOT referenced by any installed skill symlink.
pub fn clean_unused_cache() -> Result<usize> {
    let cache_dir = paths::repos_cache_dir();
    if !cache_dir.exists() {
        return Ok(0);
    }

    let hub_skills_dir = paths::hub_skills_dir();
    let referenced = collect_referenced_cache_dirs(&hub_skills_dir, &cache_dir);

    let mut removed: usize = 0;

    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if fs_ops::is_link(&path) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            if !referenced.contains(&path) {
                if std::fs::remove_dir_all(&path).is_ok() {
                    removed += 1;
                }
            }
        }
    }

    Ok(removed)
}

fn collect_referenced_cache_dirs(
    hub_skills_dir: &Path,
    cache_dir: &Path,
) -> std::collections::HashSet<PathBuf> {
    let mut referenced = std::collections::HashSet::new();

    let Ok(entries) = std::fs::read_dir(hub_skills_dir) else {
        return referenced;
    };

    for entry in entries.flatten() {
        let skill_path = entry.path();
        if !fs_ops::is_link(&skill_path) {
            continue;
        }

        let Ok(target) = fs_ops::read_link_resolved(&skill_path) else {
            continue;
        };

        if let Some(repo_root) = find_repo_root(&target) {
            if let Some(parent) = repo_root.parent() {
                if parent == cache_dir {
                    referenced.insert(repo_root);
                }
            }
        }
    }

    referenced
}

fn dir_size(path: &Path) -> u64 {
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if fs_ops::is_link(&entry_path) {
                continue;
            }
            let Ok(meta) = std::fs::symlink_metadata(&entry_path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry_path);
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

use crate::core::{agent_profile, gh_manager, lockfile, repo_history, repo_scanner, sync};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[tauri::command]
pub async fn check_gh_installed() -> Result<bool, String> {
    Ok(gh_manager::is_gh_installed())
}

#[tauri::command]
pub async fn check_gh_status() -> Result<gh_manager::GhStatus, String> {
    Ok(tokio::task::spawn_blocking(gh_manager::check_status)
        .await
        .map_err(|e| format!("Task join error: {}", e))?)
}

#[tauri::command]
pub async fn publish_skill_to_github(
    skill_name: String,
    description: String,
    is_public: bool,
    existing_repo_url: Option<String>,
    folder_name: String,
    repo_name: String,
) -> Result<gh_manager::PublishResult, String> {
    tokio::task::spawn_blocking(move || {
        gh_manager::publish_skill(
            &skill_name,
            &repo_name,
            &description,
            is_public,
            existing_repo_url.as_deref(),
            &folder_name,
        )
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_user_repos(limit: Option<u32>) -> Result<Vec<gh_manager::UserRepo>, String> {
    let n = limit.unwrap_or(30);
    tokio::task::spawn_blocking(move || gh_manager::list_user_repos(n))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn inspect_repo_folders(repo_full_name: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || gh_manager::inspect_repo_folders(&repo_full_name))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_github_repo(url: String) -> Result<repo_scanner::ScanResult, String> {
    tokio::task::spawn_blocking(move || repo_scanner::scan_repo(&url))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_from_scan(
    repo_url: String,
    source: String,
    skills: Vec<repo_scanner::SkillInstallTarget>,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        repo_scanner::install_from_repo(&source, &repo_url, &skills)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_repo_history() -> Result<Vec<repo_history::RepoHistoryEntry>, String> {
    Ok(repo_history::list_entries())
}

#[tauri::command]
pub async fn get_repo_cache_info() -> Result<repo_scanner::RepoCacheInfo, String> {
    Ok(tokio::task::spawn_blocking(repo_scanner::get_cache_info)
        .await
        .map_err(|e| format!("Task join error: {}", e))?)
}

#[tauri::command]
pub async fn clean_repo_cache() -> Result<usize, String> {
    tokio::task::spawn_blocking(repo_scanner::clean_unused_cache)
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

// ── Storage management ──────────────────────────────────────────────

/// Aggregated storage usage info for the Settings page.
#[derive(serde::Serialize)]
pub struct StorageOverview {
    /// App config files total bytes (ai_config, proxy, profiles, groups, projects…)
    pub config_bytes: u64,
    /// Skills hub directory total bytes (~/.agents/skills/)
    pub hub_bytes: u64,
    /// Number of installed skills
    pub hub_count: usize,
    /// Repo cache total bytes (~/.agents/.repos/)
    pub cache_bytes: u64,
    /// Number of cached repos
    pub cache_count: usize,
    /// Number of unused cached repos
    pub cache_unused_count: usize,
    /// Bytes used by unused cached repos
    pub cache_unused_bytes: u64,
    /// Number of entries in repo scan history
    pub history_count: usize,
}

#[tauri::command]
pub async fn get_storage_overview() -> Result<StorageOverview, String> {
    tokio::task::spawn_blocking(|| {
        let config_dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("skillstar");
        let config_bytes = dir_size_shallow(&config_dir, Some("repo_history.json"));

        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let hub_dir = home.join(".agents").join("skills");
        let hub_bytes = dir_size_recursive(&hub_dir);
        let hub_count = count_children(&hub_dir);

        let cache_info = repo_scanner::get_cache_info();

        let history_count = repo_history::entry_count();

        StorageOverview {
            config_bytes,
            hub_bytes,
            hub_count,
            cache_bytes: cache_info.total_bytes,
            cache_count: cache_info.repo_count,
            cache_unused_count: cache_info.unused_count,
            cache_unused_bytes: cache_info.unused_bytes,
            history_count,
        }
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))
}

/// Result of a unified cache cleanup.
#[derive(serde::Serialize)]
pub struct CacheCleanResult {
    /// Number of unused repos removed from cache
    pub repos_removed: usize,
    /// Number of repo history entries cleared
    pub history_cleared: usize,
}

#[tauri::command]
pub async fn clear_all_caches() -> Result<CacheCleanResult, String> {
    tokio::task::spawn_blocking(|| {
        let repos_removed = repo_scanner::clean_unused_cache().unwrap_or(0);
        let history_cleared = repo_history::clear_history().unwrap_or(0);

        // Clean up legacy "agenthub" data directory (old app name before rename to "skillstar")
        if let Some(data_dir) = dirs::data_dir() {
            let legacy_dir = data_dir.join("agenthub");
            if legacy_dir.exists() {
                let _ = std::fs::remove_dir_all(&legacy_dir);
            }
        }

        CacheCleanResult {
            repos_removed,
            history_cleared,
        }
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))
}

/// Force-delete all installed skills from the hub.
///
/// Returns the number of skill entries removed.
#[tauri::command]
pub async fn force_delete_installed_skills() -> Result<usize, String> {
    tokio::task::spawn_blocking(|| {
        let hub_dir = sync::get_hub_skills_dir();
        let removed_count = count_children(&hub_dir);

        // Remove all global skill symlinks from known agents to avoid dangling links.
        for profile in agent_profile::list_profiles() {
            let _ = sync::unlink_all_skills_from_agent(&profile.id);
        }

        if hub_dir.exists() {
            std::fs::remove_dir_all(&hub_dir)
                .map_err(|e| format!("Failed to remove hub dir: {}", e))?;
        }
        std::fs::create_dir_all(&hub_dir)
            .map_err(|e| format!("Failed to recreate hub dir: {}", e))?;

        // Clear lockfile entries so UI state and filesystem stay aligned.
        let lock_path = lockfile::lockfile_path();
        let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
        lf.skills.clear();
        let _ = lf.save(&lock_path);

        Ok(removed_count)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Force-delete all repo caches (including currently referenced ones).
///
/// Returns the number of cached repositories removed.
#[tauri::command]
pub async fn force_delete_repo_caches() -> Result<usize, String> {
    tokio::task::spawn_blocking(|| {
        let cache_dir = repos_cache_dir();
        let repos_removed = count_directories(&cache_dir);
        let hub_dir = sync::get_hub_skills_dir();
        let mut removed_skill_names: HashSet<String> = HashSet::new();

        // Drop hub symlinks that point into repo cache before deleting cache dirs.
        if let Ok(entries) = std::fs::read_dir(&hub_dir) {
            for entry in entries.flatten() {
                let skill_path = entry.path();
                if !skill_path.is_symlink() {
                    continue;
                }
                let Some(target) = resolve_symlink_target(&skill_path) else {
                    continue;
                };
                if target.starts_with(&cache_dir) {
                    if let Some(name) = entry.file_name().to_str() {
                        removed_skill_names.insert(name.to_string());
                    }
                    let _ = std::fs::remove_file(&skill_path);
                }
            }
        }

        // Remove linked references from agent skill dirs.
        for name in &removed_skill_names {
            let _ = sync::remove_skill_from_all_agents(name);
        }

        // Prune lockfile entries for removed cache-backed skills.
        if !removed_skill_names.is_empty() {
            let lock_path = lockfile::lockfile_path();
            let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
            lf.skills
                .retain(|entry| !removed_skill_names.contains(&entry.name));
            let _ = lf.save(&lock_path);
        }

        if cache_dir.exists() {
            std::fs::remove_dir_all(&cache_dir)
                .map_err(|e| format!("Failed to remove cache dir: {}", e))?;
        }
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to recreate cache dir: {}", e))?;

        Ok(repos_removed)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Force-delete app config files.
///
/// Returns the number of config files removed.
#[tauri::command]
pub async fn force_delete_app_config() -> Result<usize, String> {
    tokio::task::spawn_blocking(|| {
        let config_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("skillstar");
        if !config_dir.exists() {
            return Ok(0usize);
        }

        let mut removed = 0usize;
        if let Ok(entries) = std::fs::read_dir(&config_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                // Keep scan history isolated from config deletion.
                if file_name == "repo_history.json" {
                    continue;
                }
                if std::fs::remove_file(&path).is_ok() {
                    removed += 1;
                }
            }
        }

        Ok(removed)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Calculate total size of top-level files in a directory (non-recursive).
fn dir_size_shallow(path: &Path, exclude_file_name: Option<&str>) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Some(skip_name) = exclude_file_name {
                if entry.file_name().to_string_lossy() == skip_name {
                    continue;
                }
            }
            let p = entry.path();
            if p.is_file() {
                if let Ok(meta) = p.metadata() {
                    total += meta.len();
                }
            }
        }
    }
    total
}

/// Calculate total size of a directory recursively.
fn dir_size_recursive(path: &std::path::Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size_recursive(&p);
            } else if let Ok(meta) = p.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// Count immediate children (files & dirs) of a directory.
fn count_children(path: &std::path::Path) -> usize {
    if !path.exists() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| entries.count())
        .unwrap_or(0)
}

fn count_directories(path: &Path) -> usize {
    if !path.exists() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| entries.flatten().filter(|e| e.path().is_dir()).count())
        .unwrap_or(0)
}

fn resolve_symlink_target(symlink_path: &Path) -> Option<PathBuf> {
    std::fs::read_link(symlink_path).ok().map(|target| {
        if target.is_absolute() {
            target
        } else {
            symlink_path.parent().unwrap_or(Path::new(".")).join(target)
        }
    })
}

fn repos_cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agents")
        .join(".repos")
}

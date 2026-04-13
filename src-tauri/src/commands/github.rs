use crate::core::{
    ai_provider,
    git::{dismissed_skills, gh_manager, repo_history},
    infra::error::AppError,
    infra::{fs_ops, paths},
    local_skill,
    lockfile,
    projects::{agents as agent_profile, sync},
    repo_scanner,
    security_scan,
    skill_pack,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::error;

#[tauri::command]
pub async fn check_gh_installed() -> Result<bool, AppError> {
    Ok(gh_manager::is_gh_installed())
}

#[tauri::command]
pub async fn check_gh_status() -> Result<gh_manager::GhStatus, AppError> {
    Ok(tokio::task::spawn_blocking(gh_manager::check_status).await?)
}

#[tauri::command]
pub async fn check_git_status() -> Result<gh_manager::GitStatus, AppError> {
    Ok(tokio::task::spawn_blocking(gh_manager::check_git_status).await?)
}

#[tauri::command]
pub async fn check_developer_mode() -> Result<bool, AppError> {
    Ok(tokio::task::spawn_blocking(fs_ops::check_developer_mode).await?)
}

#[tauri::command]
pub async fn publish_skill_to_github(
    skill_name: String,
    description: String,
    is_public: bool,
    existing_repo_url: Option<String>,
    folder_name: String,
    repo_name: String,
) -> Result<gh_manager::PublishResult, AppError> {
    let was_local = local_skill::is_local_skill(&skill_name);
    let skill_scan_dir = resolve_skill_dir_for_publish(&skill_name, was_local)?;

    // Mandatory pre-publish security gate.
    let scan_config = ai_provider::AiConfig {
        // Keep the gate deterministic and cost-free: static mode only.
        enabled: false,
        ..ai_provider::AiConfig::default()
    };
    let scan_result = security_scan::scan_single_skill::<fn(&str, Option<&str>)>(
        &scan_config,
        &skill_name,
        &skill_scan_dir,
        security_scan::ScanMode::Static,
        Arc::new(tokio::sync::Semaphore::new(1)),
        None,
    )
    .await
    .map_err(|e| AppError::Other(format!("Pre-publish security scan failed: {}", e)))?;
    if scan_result.incomplete {
        return Err(AppError::Other(
            "Pre-publish security scan was incomplete. Resolve scan errors and retry publish."
                .to_string(),
        ));
    }
    if matches!(
        scan_result.risk_level,
        security_scan::RiskLevel::High | security_scan::RiskLevel::Critical
    ) {
        return Err(AppError::Other(format!(
            "Publish blocked by security gate: risk={} score={:.1}/10. {}",
            match scan_result.risk_level {
                security_scan::RiskLevel::Critical => "Critical",
                security_scan::RiskLevel::High => "High",
                security_scan::RiskLevel::Medium => "Medium",
                security_scan::RiskLevel::Low => "Low",
                security_scan::RiskLevel::Safe => "Safe",
            },
            scan_result.risk_score,
            scan_result.summary
        )));
    }

    let skill_name_clone = skill_name.clone();
    let folder_name_clone = folder_name.clone();

    let result = tokio::task::spawn_blocking(move || {
        gh_manager::publish_skill(
            &skill_name,
            &repo_name,
            &description,
            is_public,
            existing_repo_url.as_deref(),
            &folder_name,
        )
    })
    .await?
    .map_err(|e| AppError::Git(e.to_string()))?;

    // Post-publish graduation: local → hub
    if was_local {
        let git_url = result.git_url.clone();
        let source_folder = folder_name_clone.clone();

        tokio::task::spawn_blocking(move || {
            // 1. Delete from skills-local/ and remove hub symlink
            if let Err(e) = local_skill::graduate(&skill_name_clone) {
                error!(target: "publish", "failed to graduate local skill: {e}");
                return;
            }

            // 2. Re-clone from GitHub into .repos/ and symlink to skills/
            let scan = match repo_scanner::scan_repo_with_mode(&git_url, true) {
                Ok(s) => s,
                Err(e) => {
                    error!(target: "publish", "failed to scan repo after publish: {e}");
                    return;
                }
            };

            let target = scan
                .skills
                .iter()
                .find(|s| s.id == skill_name_clone || s.folder_path.ends_with(&source_folder))
                .cloned();

            if let Some(target) = target {
                let install_target = repo_scanner::SkillInstallTarget {
                    id: target.id,
                    folder_path: target.folder_path,
                };
                match repo_scanner::install_from_repo(&scan.source, &git_url, &[install_target]) {
                    Ok(_) => crate::core::installed_skill::invalidate_cache(),
                    Err(e) => error!(target: "publish", "failed to re-install from repo: {e}"),
                }
            }
        })
        .await?;
    }

    Ok(result)
}

fn resolve_skill_dir_for_publish(skill_name: &str, was_local: bool) -> Result<PathBuf, AppError> {
    let path = if was_local {
        paths::local_skills_dir().join(skill_name)
    } else {
        paths::hub_skills_dir().join(skill_name)
    };

    if !path.exists() && !fs_ops::is_link(&path) {
        return Err(AppError::Other(format!(
            "Skill '{}' directory not found for pre-publish scan",
            skill_name
        )));
    }

    if fs_ops::is_link(&path) {
        std::fs::canonicalize(&path).map_err(|e| {
            AppError::Other(format!(
                "Failed to resolve symlink for skill '{}': {}",
                skill_name, e
            ))
        })
    } else {
        Ok(path)
    }
}

#[tauri::command]
pub async fn list_user_repos(limit: Option<u32>) -> Result<Vec<gh_manager::UserRepo>, AppError> {
    let repo_limit = limit.unwrap_or(30);
    tokio::task::spawn_blocking(move || gh_manager::list_user_repos(repo_limit))
        .await?
        .map_err(|e| AppError::Git(e.to_string()))
}

#[tauri::command]
pub async fn inspect_repo_folders(repo_full_name: String) -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(move || gh_manager::inspect_repo_folders(&repo_full_name))
        .await?
        .map_err(|e| AppError::Git(e.to_string()))
}

#[tauri::command]
pub async fn scan_github_repo(
    url: String,
    full_depth: Option<bool>,
) -> Result<repo_scanner::ScanResult, AppError> {
    let use_full_depth = full_depth.unwrap_or(false);
    tokio::task::spawn_blocking(move || repo_scanner::scan_repo_with_mode(&url, use_full_depth))
        .await?
        .map_err(|e| AppError::Git(e.to_string()))
}

#[tauri::command]
pub async fn install_from_scan(
    repo_url: String,
    source: String,
    skills: Vec<repo_scanner::SkillInstallTarget>,
) -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(move || {
        let install_result = repo_scanner::install_from_repo(&source, &repo_url, &skills);
        crate::core::installed_skill::invalidate_cache();
        install_result
    })
    .await?
    .map_err(|e| AppError::Git(e.to_string()))
}

#[tauri::command]
pub async fn list_repo_history() -> Result<Vec<repo_history::RepoHistoryEntry>, AppError> {
    Ok(repo_history::list_entries())
}

#[tauri::command]
pub async fn get_repo_cache_info() -> Result<repo_scanner::RepoCacheInfo, AppError> {
    Ok(tokio::task::spawn_blocking(repo_scanner::get_cache_info).await?)
}

#[tauri::command]
pub async fn clean_repo_cache() -> Result<usize, AppError> {
    tokio::task::spawn_blocking(repo_scanner::clean_unused_cache)
        .await?
        .map_err(|e| AppError::Anyhow(e))
}

// ── Storage management ──────────────────────────────────────────────

/// Aggregated storage usage info for the Settings page.
#[derive(serde::Serialize)]
pub struct StorageOverview {
    /// Resolved data root path (`SKILLSTAR_DATA_DIR` or default `~/.skillstar`)
    pub data_root_path: String,
    /// Resolved hub root path (`SKILLSTAR_HUB_DIR` or default `~/.skillstar/.agents`)
    pub hub_root_path: String,
    /// Whether hub root is nested under data root.
    pub is_hub_under_data: bool,
    /// App config files total bytes (ai_config, proxy, profiles, groups, projects…)
    pub config_bytes: u64,
    /// Resolved app config directory path.
    pub config_path: String,
    /// Skills hub directory total bytes (~/.skillstar/.agents/skills/)
    pub hub_bytes: u64,
    /// Resolved installed skills directory path.
    pub hub_path: String,
    /// Number of valid installed skills
    pub hub_count: usize,
    /// Number of broken skills (broken symlinks, orphaned lockfile entries)
    pub broken_count: usize,
    /// Number of local skills in skills-local/
    pub local_count: usize,
    /// Total bytes of skills-local/ directory
    pub local_bytes: u64,
    /// Resolved local skills directory path.
    pub local_path: String,
    /// Repo cache total bytes (~/.skillstar/.agents/.repos/)
    pub cache_bytes: u64,
    /// Resolved repo cache directory path.
    pub cache_path: String,
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
pub async fn get_storage_overview() -> Result<StorageOverview, AppError> {
    Ok(tokio::task::spawn_blocking(|| {
        let data_root = paths::data_root();
        let hub_root = paths::hub_root();
        let is_hub_under_data = hub_root.starts_with(&data_root) && hub_root != data_root;

        let config_dir = paths::config_dir();
        let config_bytes = dir_size_recursive(&config_dir);

        let hub_dir = paths::hub_skills_dir();
        let hub_bytes = dir_size_recursive(&hub_dir);
        let (hub_count, broken_count) = count_hub_skills(&hub_dir);

        let local_dir = paths::local_skills_dir();
        let local_bytes = dir_size_recursive(&local_dir);
        let local_count = count_directories(&local_dir);

        let cache_info = repo_scanner::get_cache_info();

        let history_count = repo_history::entry_count();

        StorageOverview {
            data_root_path: data_root.to_string_lossy().to_string(),
            hub_root_path: hub_root.to_string_lossy().to_string(),
            is_hub_under_data,
            config_bytes,
            config_path: config_dir.to_string_lossy().to_string(),
            hub_bytes,
            hub_path: hub_dir.to_string_lossy().to_string(),
            hub_count,
            broken_count,
            local_count,
            local_bytes,
            local_path: local_dir.to_string_lossy().to_string(),
            cache_bytes: cache_info.total_bytes,
            cache_path: paths::repos_cache_dir().to_string_lossy().to_string(),
            cache_count: cache_info.repo_count,
            cache_unused_count: cache_info.unused_count,
            cache_unused_bytes: cache_info.unused_bytes,
            history_count,
        }
    })
    .await?)
}

/// Result of a unified cache cleanup.
#[derive(serde::Serialize)]
pub struct CacheCleanResult {
    /// Number of unused repos removed from cache
    pub repos_removed: usize,
    /// Number of repo history entries cleared
    pub history_cleared: usize,
    /// Number of cached translation entries cleared
    pub translation_cleared: usize,
}

#[tauri::command]
pub async fn clear_all_caches() -> Result<CacheCleanResult, AppError> {
    Ok(tokio::task::spawn_blocking(|| {
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
            // Translation cache is intentionally preserved — clearing it would
            // force expensive re-translation via AI APIs.
            translation_cleared: 0,
        }
    })
    .await?)
}

/// Force-delete all installed skills from the hub.
///
/// Returns the number of skill entries removed.
#[tauri::command]
pub async fn force_delete_installed_skills() -> Result<usize, AppError> {
    tokio::task::spawn_blocking(|| -> Result<usize, AppError> {
        let hub_dir = crate::core::infra::paths::hub_skills_dir();
        let removed_count = count_children(&hub_dir);

        // Remove all global skill symlinks from known agents to avoid dangling links.
        for profile in agent_profile::list_profiles() {
            let _ = sync::unlink_all_skills_from_agent(&profile.id);
        }

        if hub_dir.exists() {
            fs_ops::remove_dir_all_retry(&hub_dir)?;
        }
        std::fs::create_dir_all(&hub_dir)?;

        // Clear lockfile entries so UI state and filesystem stay aligned.
        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| AppError::Lockfile("Lockfile mutex poisoned".to_string()))?;
        let lock_path = lockfile::lockfile_path();
        let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
        lf.skills.clear();
        let _ = lf.save(&lock_path);
        crate::core::installed_skill::invalidate_cache();
        Ok(removed_count)
    })
    .await?
}

/// Force-delete all repo caches (including currently referenced ones).
///
/// Returns the number of cached repositories removed.
#[tauri::command]
pub async fn force_delete_repo_caches() -> Result<usize, AppError> {
    tokio::task::spawn_blocking(|| -> Result<usize, AppError> {
        let cache_dir = repos_cache_dir();
        let repos_removed = count_directories(&cache_dir);
        let hub_dir = crate::core::infra::paths::hub_skills_dir();
        let mut removed_skill_names: HashSet<String> = HashSet::new();

        // Drop hub symlinks that point into repo cache before deleting cache dirs.
        if let Ok(entries) = std::fs::read_dir(&hub_dir) {
            for entry in entries.flatten() {
                let skill_path = entry.path();
                if !fs_ops::is_link(&skill_path) {
                    continue;
                }
                let Some(target) = fs_ops::read_link_resolved(&skill_path).ok() else {
                    continue;
                };
                if target.starts_with(&cache_dir) {
                    if let Some(name) = entry.file_name().to_str() {
                        removed_skill_names.insert(name.to_string());
                    }
                    let _ = fs_ops::remove_symlink(&skill_path);
                }
            }
        }

        // Remove linked references from agent skill dirs.
        for name in &removed_skill_names {
            let _ = sync::remove_skill_from_all_agents(name);
        }

        // Prune lockfile entries for removed cache-backed skills.
        if !removed_skill_names.is_empty() {
            let _lock = lockfile::get_mutex()
                .lock()
                .map_err(|_| AppError::Lockfile("Lockfile mutex poisoned".to_string()))?;
            let lock_path = lockfile::lockfile_path();
            let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
            lf.skills
                .retain(|entry| !removed_skill_names.contains(&entry.name));
            let _ = lf.save(&lock_path);
            crate::core::installed_skill::invalidate_cache();
        }

        if cache_dir.exists() {
            fs_ops::remove_dir_all_retry(&cache_dir)?;
        }
        std::fs::create_dir_all(&cache_dir)?;

        Ok(repos_removed)
    })
    .await?
}

/// Force-delete app config files.
///
/// Returns the number of config files removed.
#[tauri::command]
pub async fn force_delete_app_config() -> Result<usize, AppError> {
    Ok(tokio::task::spawn_blocking(|| {
        let mut removed = 0usize;

        // Delete config dir contents
        let config_dir = paths::config_dir();
        if config_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&config_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if std::fs::remove_file(&path).is_ok() {
                            removed += 1;
                        }
                    }
                }
            }
        }

        // Also delete state dir contents (rebuildable)
        let state_dir = paths::state_dir();
        if state_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&state_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if std::fs::remove_file(&path).is_ok() {
                            removed += 1;
                        }
                    }
                }
            }
        }

        removed
    })
    .await?)
}

/// Calculate total size of a directory recursively.
fn dir_size_recursive(path: &std::path::Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            // Do not follow symlink/junction targets when sizing storage.
            // Following links can double-count repo cache content and can hang
            // on cyclic link graphs (especially on Windows junction-heavy setups).
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

/// Count immediate children (files & dirs) of a directory.
fn count_children(path: &std::path::Path) -> usize {
    if !path.exists() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| entries.count())
        .unwrap_or(0)
}

/// Count hub skills, returning (valid_count, broken_count).
///
/// A skill entry is "broken" if it is a symlink whose target no longer exists.
/// Uses `symlink_metadata()` to detect symlink entries that `is_dir()` would skip.
fn count_hub_skills(hub_dir: &Path) -> (usize, usize) {
    if !hub_dir.exists() {
        return (0, 0);
    }
    let mut valid: usize = 0;
    let mut broken: usize = 0;
    if let Ok(entries) = std::fs::read_dir(hub_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = path.symlink_metadata() else {
                continue;
            };
            if fs_ops::is_link(&path) {
                // Symlink: check if the target still exists
                if path.exists() {
                    valid += 1;
                } else {
                    broken += 1;
                }
            } else if meta.is_dir() {
                valid += 1;
            }
            // Skip regular files (e.g. .DS_Store)
        }
    }

    // Also count orphaned lockfile entries (in lockfile but not on disk)
    let lock_path = lockfile::lockfile_path();
    if let Ok(lf) = lockfile::Lockfile::load(&lock_path) {
        for entry in &lf.skills {
            let skill_path = hub_dir.join(&entry.name);
            // Entry exists in lockfile but has no directory/symlink at all
            if skill_path.symlink_metadata().is_err() {
                broken += 1;
            }
        }
    }

    (valid, broken)
}

/// Remove broken symlinks from the hub and prune orphaned lockfile entries.
///
/// Returns the number of issues fixed.
#[tauri::command]
pub async fn clean_broken_skills() -> Result<usize, AppError> {
    tokio::task::spawn_blocking(|| -> Result<usize, AppError> {
        let hub_dir = crate::core::infra::paths::hub_skills_dir();
        let mut fixed: usize = 0;
        let mut removed_names: HashSet<String> = HashSet::new();

        // Phase 1: Remove broken symlinks from hub
        if let Ok(entries) = std::fs::read_dir(&hub_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.symlink_metadata().is_err() {
                    continue;
                }
                if fs_ops::is_link(&path) && !path.exists() {
                    // Broken symlink — target is gone
                    if fs_ops::remove_symlink(&path).is_ok() {
                        if let Some(name) = entry.file_name().to_str() {
                            removed_names.insert(name.to_string());
                        }
                        fixed += 1;
                    }
                }
            }
        }

        // Phase 2: Clean agent-side symlinks for removed skills
        for name in &removed_names {
            let _ = sync::remove_skill_from_all_agents(name);
        }

        // Phase 3: Prune orphaned lockfile entries
        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| AppError::Lockfile("Lockfile mutex poisoned".to_string()))?;
        let lock_path = lockfile::lockfile_path();
        let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
        let before = lf.skills.len();
        lf.skills.retain(|entry| {
            let skill_path = hub_dir.join(&entry.name);
            // Keep entries that have a valid directory or valid symlink
            skill_path.symlink_metadata().is_ok()
                && (!fs_ops::is_link(&skill_path) || skill_path.exists())
        });
        let orphans_removed = before - lf.skills.len();
        if orphans_removed > 0 {
            let _ = lf.save(&lock_path);
            crate::core::installed_skill::invalidate_cache();
            fixed += orphans_removed;
        }

        Ok(fixed)
    })
    .await?
}

fn count_directories(path: &Path) -> usize {
    if !path.exists() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    let p = entry.path();
                    if fs_ops::is_link(&p) {
                        return false;
                    }
                    p.symlink_metadata().map(|m| m.is_dir()).unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

fn repos_cache_dir() -> PathBuf {
    paths::repos_cache_dir()
}

// ── Skill Pack Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn install_pack_from_url(url: String) -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(move || {
        crate::core::skill_install::install_skill_pack(url).map_err(AppError::Other)
    })
    .await?
}

#[tauri::command]
pub async fn list_installed_packs() -> Result<Vec<skill_pack::PackEntry>, AppError> {
    Ok(tokio::task::spawn_blocking(skill_pack::list_packs).await?)
}

#[tauri::command]
pub async fn remove_installed_pack(name: String) -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(move || skill_pack::remove_pack(&name))
        .await?
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn get_pack_doctor(name: String) -> Result<skill_pack::DoctorReport, AppError> {
    tokio::task::spawn_blocking(move || skill_pack::doctor_pack(&name))
        .await?
        .map_err(|e| AppError::Other(e.to_string()))
}

// ── New-skill detection commands ────────────────────────────────────

/// Manually check all cached repos for new uninstalled skills.
/// Returns the list after filtering out dismissed entries.
#[tauri::command]
pub async fn check_new_repo_skills() -> Result<Vec<repo_scanner::RepoNewSkill>, AppError> {
    let new_skills =
        tokio::task::spawn_blocking(repo_scanner::detect_new_skills_in_cached_repos).await?;

    let dismissed = dismissed_skills::load_dismissed();
    let dismissed_set: std::collections::HashSet<&str> =
        dismissed.iter().map(|s| s.as_str()).collect();

    let filtered: Vec<repo_scanner::RepoNewSkill> = new_skills
        .into_iter()
        .filter(|s| {
            let key = format!("{}/{}", s.repo_source, s.skill_id);
            !dismissed_set.contains(key.as_str())
        })
        .collect();

    Ok(filtered)
}

/// Dismiss a new-skill notification so it won't appear again.
/// Key format: "repo_source/skill_id"
#[tauri::command]
pub async fn dismiss_new_skill(key: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || dismissed_skills::dismiss(&key))
        .await?
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Load all dismissed new-skill keys.
#[tauri::command]
pub async fn get_dismissed_new_skills() -> Result<Vec<String>, AppError> {
    Ok(dismissed_skills::load_dismissed())
}

/// Batch dismiss multiple new-skill notifications.
/// Used for repo-level "dismiss all" in the ghost card group header.
#[tauri::command]
pub async fn dismiss_new_skills_batch(keys: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || dismissed_skills::dismiss_batch(&keys))
        .await?
        .map_err(|e| AppError::Other(e.to_string()))
}

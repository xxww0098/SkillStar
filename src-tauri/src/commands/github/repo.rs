//! GitHub / git CLI status, skill publishing, repo scan + install, and
//! new-skill detection commands. Thin forwarders over `skillstar_skills::git`
//! and `crate::core::repo_scanner`.

use crate::core::{local_skill, lockfile, repo_scanner};
use skillstar_core::infra::error::AppError;
use skillstar_core::infra::fs_ops;
use skillstar_skills::git::{dismissed_skills, gh_manager, repo_history};
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
            &lockfile::lockfile_path(),
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
        .map_err(AppError::Anyhow)
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

//! GitHub / git CLI status, skill publishing, repo scan + install, and
//! new-skill detection commands. Thin forwarders over `skillstar_skills::git`
//! and `skillstar_skills::repo_scanner`.

use skillstar_core::infra::error::AppError;
use skillstar_git::{dismissed_skills, repo_history};
use skillstar_skills::deployment;
use skillstar_skills::git::gh_manager;
use skillstar_skills::local_skill;
use skillstar_skills::lockfile;
use skillstar_skills::repo_scanner;
use tauri::{AppHandle, Manager, State};

use crate::core::github_auth::GitHubAuthState;

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
    Ok(tokio::task::spawn_blocking(deployment::developer_mode_available).await?)
}

#[tauri::command]
pub async fn publish_skill_to_github(
    skill_name: String,
    description: String,
    is_public: bool,
    existing_repo_url: Option<String>,
    folder_name: String,
    repo_name: String,
    app: AppHandle,
) -> Result<gh_manager::PublishResult, AppError> {
    let was_local = local_skill::is_local_skill(&skill_name);

    let skill_name_clone = skill_name.clone();

    let result = tokio::task::spawn_blocking(move || {
        let lock_path = lockfile::lockfile_path();
        let lockfile_mode = if was_local {
            gh_manager::PublishLockfileMode::ValidateOnly(&lock_path)
        } else {
            gh_manager::PublishLockfileMode::Commit(&lock_path)
        };
        gh_manager::publish_skill(
            &skill_name,
            &repo_name,
            &description,
            is_public,
            existing_repo_url.as_deref(),
            &folder_name,
            lockfile_mode,
        )
    })
    .await?
    .map_err(|e| AppError::Git(e.to_string()))?;

    // Post-publish graduation: local → hub
    if was_local {
        let auth_state = app.state::<GitHubAuthState>();
        let git_facade = auth_state
            .begin_git_operation(app.clone(), None)
            .map_err(|error| AppError::Git(error.to_string()))?;
        let session_id = git_facade.session().id().to_string();
        let git_url = result.git_url.clone();

        let graduation = tokio::task::spawn_blocking(move || -> Result<(), String> {
            // Keep the local copy intact until the published repository has
            // passed the same ownership guard as every other generic scan.
            let scan = git_facade
                .scan_repo(&git_url, true)
                .map_err(|error| error.to_string())?;

            let target = scan
                .skills
                .iter()
                .find(|skill| skill.id == skill_name_clone)
                .cloned()
                .ok_or_else(|| {
                    "Published repository does not contain the local Skill that must be graduated"
                        .to_string()
                })?;
            let install_target = repo_scanner::SkillInstallTarget {
                id: target.id,
                folder_path: target.folder_path,
            };
            git_facade
                .graduate_local_skill_from_scan(
                    &skill_name_clone,
                    &scan.source,
                    &git_url,
                    &install_target,
                )
                .map_err(|error| error.to_string())
        })
        .await;
        auth_state.finish_git_operation(&session_id);
        graduation?.map_err(AppError::Git)?;
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
    session_id: Option<String>,
    app: AppHandle,
    auth_state: State<'_, GitHubAuthState>,
) -> Result<repo_scanner::ScanResult, AppError> {
    let use_full_depth = full_depth.unwrap_or(false);
    let facade = auth_state
        .begin_git_operation(app, session_id)
        .map_err(|error| AppError::Git(error.to_string()))?;
    let session_id = facade.session().id().to_string();
    let result = tokio::task::spawn_blocking(move || facade.scan_repo(&url, use_full_depth)).await;
    auth_state.finish_git_operation(&session_id);
    result?.map_err(|e| AppError::Git(e.to_string()))
}

#[tauri::command]
pub async fn install_from_scan(
    repo_url: String,
    source: String,
    skills: Vec<repo_scanner::SkillInstallTarget>,
    session_id: Option<String>,
    app: AppHandle,
    auth_state: State<'_, GitHubAuthState>,
) -> Result<Vec<String>, AppError> {
    let facade = auth_state
        .begin_git_operation(app, session_id)
        .map_err(|error| AppError::Git(error.to_string()))?;
    let session_id = facade.session().id().to_string();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<String>> {
        let install_result = facade.install_from_scan(&source, &repo_url, &skills);
        skillstar_skills::installed_skill::invalidate_cache();
        let installed = install_result?;
        // Match the CLI: after a successful hub install, deploy the skills to
        // every Agent the user has enabled in Settings.
        skillstar_app::global_deploy::deploy_to_enabled_global_agents(&installed).map_err(
            |error| {
                tracing::warn!(
                    target: "cmd",
                    count = installed.len(),
                    error = %error,
                    "hub install succeeded but Agent deployment failed"
                );
                anyhow::anyhow!(error)
            },
        )?;
        Ok(installed)
    })
    .await;
    auth_state.finish_git_operation(&session_id);
    result?.map_err(|e| AppError::Git(e.to_string()))
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

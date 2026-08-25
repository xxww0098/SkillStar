use skillstar_core::infra::error::AppError;
use skillstar_skills::deployment;
use skillstar_skills::{Skill, installed_skill, skill_install};
use tauri::{AppHandle, State};

use crate::core::github_auth::GitHubAuthState;

pub use skillstar_skills::skill_update::{
    LocalDivergenceResolution, ResolveSkillUpdateResult, SkillUpdateReport, UpdateResult,
};

#[tauri::command]
pub async fn list_skills() -> Result<Vec<Skill>, AppError> {
    installed_skill::list_installed_skills()
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn refresh_skill_updates(
    app: AppHandle,
    auth_state: State<'_, GitHubAuthState>,
) -> Result<Vec<installed_skill::SkillUpdateState>, AppError> {
    let facade = auth_state
        .begin_git_operation(app, None)
        .map_err(|error| AppError::Git(error.to_string()))?;
    let session_id = facade.session().id().to_string();
    let result = facade.refresh_skill_updates().await;
    auth_state.finish_git_operation(&session_id);
    result.map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn install_skill(
    url: String,
    name: Option<String>,
    app: AppHandle,
    auth_state: State<'_, GitHubAuthState>,
) -> Result<Skill, AppError> {
    let facade = auth_state
        .begin_git_operation(app, None)
        .map_err(|error| AppError::Git(error.to_string()))?;
    let session_id = facade.session().id().to_string();
    let result = tokio::task::spawn_blocking(move || {
        let skill = facade.install_skill(url, name)?;
        // Match the CLI: after a successful hub install, deploy the skill to
        // every Agent the user has enabled in Settings.
        skillstar_app::global_deploy::deploy_to_enabled_global_agents(std::slice::from_ref(
            &skill.name,
        ))
        .map_err(|error| {
            tracing::warn!(
                target: "cmd",
                skill = %skill.name,
                error = %error,
                "hub install succeeded but Agent deployment failed"
            );
            error
        })?;
        Ok(skill)
    })
    .await;
    auth_state.finish_git_operation(&session_id);
    result
        .map_err(|e| AppError::Other(format!("install task panicked: {e}")))?
        .map_err(AppError::Other)
}

#[tauri::command]
pub async fn uninstall_skill(name: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || uninstall_skill_sync(name))
        .await
        .map_err(|e| AppError::Other(format!("uninstall task panicked: {e}")))?
}

fn uninstall_skill_sync(name: String) -> Result<(), AppError> {
    skill_install::uninstall_skill(&name).map_err(AppError::Other)
}

#[tauri::command]
pub async fn toggle_skill_for_agent(
    skill_name: String,
    agent_id: String,
    enable: bool,
) -> Result<(), AppError> {
    tracing::info!(
        target: "cmd",
        skill_name,
        agent_id,
        enable,
        "toggle_skill_for_agent called"
    );
    let outcome = deployment::toggle_skill_for_agent(&skill_name, &agent_id, enable).map_err(|e| {
        tracing::error!(target: "cmd", skill_name, agent_id, enable, error = %e, "toggle_skill_for_agent failed");
        AppError::Anyhow(e)
    })?;
    match outcome {
        deployment::ToggleSkillOutcome::Applied => {
            installed_skill::invalidate_cache();
            tracing::info!(target: "cmd", skill_name, agent_id, enable, "toggle_skill_for_agent completed");
            Ok(())
        }
        deployment::ToggleSkillOutcome::Skipped { reason, path, .. } => {
            tracing::warn!(
                target: "cmd",
                skill_name,
                agent_id,
                enable,
                reason = %reason,
                path = %path,
                "toggle_skill_for_agent skipped due to name collision"
            );
            Err(AppError::Other(reason))
        }
    }
}

#[tauri::command]
pub async fn update_skill(
    name: String,
    app: AppHandle,
    auth_state: State<'_, GitHubAuthState>,
) -> Result<UpdateResult, AppError> {
    let facade = auth_state
        .begin_git_operation(app, None)
        .map_err(|error| AppError::Git(error.to_string()))?;
    let session_id = facade.session().id().to_string();
    let result = tokio::task::spawn_blocking(move || facade.update_skill(&name)).await;
    auth_state.finish_git_operation(&session_id);
    result
        .map_err(|e| AppError::Other(format!("update task panicked: {e}")))?
        .map_err(AppError::Anyhow)
}

/// Update several skills at once, pulling each repository only once.
///
/// Per-skill failures ride in the report rather than failing the batch.
#[tauri::command]
pub async fn update_skills(
    names: Vec<String>,
    app: AppHandle,
    auth_state: State<'_, GitHubAuthState>,
) -> Result<SkillUpdateReport, AppError> {
    let facade = auth_state
        .begin_git_operation(app, None)
        .map_err(|error| AppError::Git(error.to_string()))?;
    let session_id = facade.session().id().to_string();
    let result = tokio::task::spawn_blocking(move || facade.update_skills(&names)).await;
    auth_state.finish_git_operation(&session_id);
    result.map_err(|e| AppError::Other(format!("update task panicked: {e}")))
}

#[tauri::command]
pub async fn resolve_skill_update(
    name: String,
    resolution: LocalDivergenceResolution,
    app: AppHandle,
    auth_state: State<'_, GitHubAuthState>,
) -> Result<ResolveSkillUpdateResult, AppError> {
    let facade = auth_state
        .begin_git_operation(app, None)
        .map_err(|error| AppError::Git(error.to_string()))?;
    let session_id = facade.session().id().to_string();
    let result =
        tokio::task::spawn_blocking(move || facade.resolve_skill_update(&name, resolution)).await;
    auth_state.finish_git_operation(&session_id);
    result
        .map_err(|e| AppError::Other(format!("update resolution task panicked: {e}")))?
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn open_skill_folder(name: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        skillstar_skills::content::open_skill_folder(&name).map_err(AppError::Anyhow)
    })
    .await
    .map_err(|e| AppError::Other(format!("open_skill_folder task panicked: {e}")))?
}

/// Install the successor an update check recorded for `name`, carry its
/// Agent/Project deployments over, and remove `name`.
#[tauri::command]
pub async fn migrate_renamed_skill(
    name: String,
    app: AppHandle,
    auth_state: State<'_, GitHubAuthState>,
) -> Result<skillstar_app::skill_migration::SkillMigrationReport, AppError> {
    let facade = auth_state
        .begin_git_operation(app, None)
        .map_err(|error| AppError::Git(error.to_string()))?;
    let session_id = facade.session().id().to_string();
    let result = tokio::task::spawn_blocking(move || {
        skillstar_app::skill_migration::migrate_renamed_skill(&name, &facade)
    })
    .await;
    auth_state.finish_git_operation(&session_id);
    result
        .map_err(|e| AppError::Other(format!("skill migration task panicked: {e}")))?
        .map_err(AppError::Anyhow)
}

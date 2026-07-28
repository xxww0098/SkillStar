use skillstar_core::infra::error::AppError;
use skillstar_skills::deployment;
use skillstar_skills::{Skill, installed_skill, skill_install, skill_update};

pub use skillstar_skills::skill_update::UpdateResult;

#[tauri::command]
pub async fn list_skills() -> Result<Vec<Skill>, AppError> {
    installed_skill::list_installed_skills()
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn refresh_skill_updates() -> Result<Vec<installed_skill::SkillUpdateState>, AppError> {
    installed_skill::refresh_skill_updates()
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn install_skill(url: String, name: Option<String>) -> Result<Skill, AppError> {
    tokio::task::spawn_blocking(move || skill_install::install_skill(url, name))
        .await
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
    deployment::toggle_skill_for_agent(&skill_name, &agent_id, enable).map_err(|e| {
        tracing::error!(target: "cmd", skill_name, agent_id, enable, error = %e, "toggle_skill_for_agent failed");
        AppError::Anyhow(e)
    })?;
    installed_skill::invalidate_cache();
    tracing::info!(target: "cmd", skill_name, agent_id, enable, "toggle_skill_for_agent completed");
    Ok(())
}

#[tauri::command]
pub async fn update_skill(name: String) -> Result<UpdateResult, AppError> {
    tokio::task::spawn_blocking(move || skill_update::update_skill(&name))
        .await
        .map_err(|e| AppError::Other(format!("update task panicked: {e}")))?
        .map_err(AppError::Anyhow)
}

//! Skill-pack (`.agd`) commands: install from URL, list, remove, doctor.
//! Thin forwarders over `crate::core::{skill_pack, skill_install}`.

use skillstar_core::infra::error::AppError;
use skillstar_skills::skill_pack;
use tauri::{AppHandle, State};

use crate::core::github_auth::GitHubAuthState;

#[tauri::command]
pub async fn install_pack_from_url(
    url: String,
    app: AppHandle,
    auth_state: State<'_, GitHubAuthState>,
) -> Result<Vec<String>, AppError> {
    let facade = auth_state
        .begin_git_operation(app, None)
        .map_err(|error| AppError::Git(error.to_string()))?;
    let session_id = facade.session().id().to_string();
    let result = tokio::task::spawn_blocking(move || {
        facade.install_skill_pack(url).map_err(AppError::Other)
    })
    .await;
    auth_state.finish_git_operation(&session_id);
    result?
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

//! Skill-pack (`.agd`) commands: install from URL, list, remove, doctor.
//! Thin forwarders over `crate::core::{skill_pack, skill_install}`.

use crate::core::skill_pack;
use skillstar_core::infra::error::AppError;

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

use crate::core::{infra::error::AppError, skill_bundle};

#[tauri::command]
pub async fn export_skill_bundle(
    name: String,
    output_path: Option<String>,
) -> Result<String, AppError> {
    let path = tokio::task::spawn_blocking(move || {
        skill_bundle::export_bundle(&name, output_path.as_deref())
            .map(|path| path.to_string_lossy().to_string())
    })
    .await?
    .map_err(|e| AppError::Bundle(e.to_string()))?;
    Ok(path)
}

#[tauri::command]
pub async fn preview_skill_bundle(
    file_path: String,
) -> Result<skill_bundle::BundleManifest, AppError> {
    tokio::task::spawn_blocking(move || skill_bundle::preview_bundle(&file_path))
        .await?
        .map_err(|e| AppError::Bundle(e.to_string()))
}

#[tauri::command]
pub async fn import_skill_bundle(
    file_path: String,
    force: bool,
) -> Result<skill_bundle::ImportBundleResult, AppError> {
    tokio::task::spawn_blocking(move || skill_bundle::import_bundle(&file_path, force))
        .await?
        .map_err(|e| AppError::Bundle(e.to_string()))
}

#[tauri::command]
pub async fn export_multi_skill_bundle(
    names: Vec<String>,
    output_path: String,
) -> Result<String, AppError> {
    let path = tokio::task::spawn_blocking(move || {
        skill_bundle::export_multi_bundle(&names, &output_path)
            .map(|path| path.to_string_lossy().to_string())
    })
    .await?
    .map_err(|e| AppError::Bundle(e.to_string()))?;
    Ok(path)
}

#[tauri::command]
pub async fn preview_multi_skill_bundle(
    file_path: String,
) -> Result<skill_bundle::MultiManifest, AppError> {
    tokio::task::spawn_blocking(move || skill_bundle::preview_multi_bundle(&file_path))
        .await?
        .map_err(|e| AppError::Bundle(e.to_string()))
}

#[tauri::command]
pub async fn import_multi_skill_bundle(
    file_path: String,
    force: bool,
) -> Result<skill_bundle::ImportMultiBundleResult, AppError> {
    tokio::task::spawn_blocking(move || skill_bundle::import_multi_bundle(&file_path, force))
        .await?
        .map_err(|e| AppError::Bundle(e.to_string()))
}

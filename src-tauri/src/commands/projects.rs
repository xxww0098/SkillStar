use crate::core::infra::error::AppError;
use crate::core::project_manifest;
use std::collections::HashMap;

pub use skillstar_commands::projects::*;

#[tauri::command]
pub async fn import_project_skills(
    project_path: String,
    project_name: String,
    targets: Vec<project_manifest::ImportTarget>,
) -> Result<project_manifest::ImportResult, AppError> {
    project_manifest::import_scanned_skills(&project_path, &project_name, &targets)
        .map_err(|e| AppError::Other(e.to_string()))
}

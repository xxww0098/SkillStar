//! Thin Tauri adapter for the local-folder adoption use case.

use skillstar_core::infra::error::AppError;
use skillstar_skills::local_skill;

pub use skillstar_skills::local_skill::AdoptLocalFolderResult;

#[tauri::command]
pub async fn adopt_local_folder(
    folder_path: String,
    names: Option<Vec<String>>,
) -> Result<AdoptLocalFolderResult, AppError> {
    tokio::task::spawn_blocking(move || local_skill::adopt_folder(&folder_path, names))
        .await
        .map_err(|error| AppError::Other(format!("adopt folder task panicked: {error}")))?
        .map_err(AppError::Anyhow)
}

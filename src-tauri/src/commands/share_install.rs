//! Thin Tauri adapter for the Skills share-code install use case.

use skillstar_core::infra::error::AppError;
use skillstar_skills::share_install;

pub use skillstar_skills::share_install::{ShareCodeInstallSummary, ShareCodeSkill};

#[tauri::command]
pub async fn install_from_share_code(
    skills: Vec<ShareCodeSkill>,
) -> Result<ShareCodeInstallSummary, AppError> {
    tokio::task::spawn_blocking(move || share_install::install_from_share_code(skills))
        .await
        .map_err(|error| AppError::Other(format!("share-code install task panicked: {error}")))
}

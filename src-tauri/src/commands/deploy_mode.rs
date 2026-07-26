//! Thin Tauri adapter for Skills deployment-status inspection.

use skillstar_core::infra::error::AppError;
use skillstar_skills::deployment;

pub use skillstar_skills::deployment::AgentDeployStatus;

#[tauri::command]
pub async fn get_skill_deploy_status(
    skill_name: String,
) -> Result<Vec<AgentDeployStatus>, AppError> {
    tokio::task::spawn_blocking(move || deployment::get_skill_deploy_status(&skill_name))
        .await
        .map_err(|error| AppError::Other(format!("deploy-status task panicked: {error}")))
}

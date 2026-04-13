use crate::core::{acp_client, infra::error::AppError};

/// Get the ACP configuration.
#[tauri::command]
pub async fn get_acp_config() -> Result<acp_client::AcpConfig, AppError> {
    Ok(acp_client::load_config())
}

/// Save ACP configuration.
#[tauri::command]
pub async fn save_acp_config(config: acp_client::AcpConfig) -> Result<(), AppError> {
    acp_client::save_config(&config)
        .map_err(|e| AppError::Other(format!("Failed to save ACP config: {}", e)))
}

use skillstar_config::acp::{AcpConfig, load_config, save_config};
use skillstar_infra::error::AppError;

#[tauri::command]
pub async fn get_acp_config() -> Result<AcpConfig, AppError> {
    Ok(load_config())
}

#[tauri::command]
pub async fn save_acp_config(config: AcpConfig) -> Result<(), AppError> {
    save_config(&config).map_err(|e| AppError::Other(format!("Failed to save ACP config: {}", e)))
}

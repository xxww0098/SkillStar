use crate::core::{
    config::{github_mirror, proxy},
    infra::error::AppError,
};

#[tauri::command]
pub async fn get_proxy_config() -> Result<proxy::ProxyConfig, AppError> {
    proxy::load_config().map_err(|e| AppError::Other(format!("Failed to read proxy config: {}", e)))
}

#[tauri::command]
pub async fn save_proxy_config(config: proxy::ProxyConfig) -> Result<(), AppError> {
    proxy::save_config(&config)
        .map_err(|e| AppError::Other(format!("Failed to write proxy config: {}", e)))
}

#[tauri::command]
pub async fn get_github_mirror_config() -> Result<github_mirror::GitHubMirrorConfig, AppError> {
    github_mirror::load_config()
        .map_err(|e| AppError::Other(format!("Failed to read mirror config: {}", e)))
}

#[tauri::command]
pub async fn save_github_mirror_config(
    config: github_mirror::GitHubMirrorConfig,
) -> Result<(), AppError> {
    github_mirror::save_config(&config)
        .map_err(|e| AppError::Other(format!("Failed to write mirror config: {}", e)))
}

#[tauri::command]
pub async fn get_github_mirror_presets() -> Result<Vec<github_mirror::MirrorPreset>, AppError> {
    Ok(github_mirror::builtin_presets())
}

#[tauri::command]
pub async fn test_github_mirror(url: String) -> Result<u64, AppError> {
    github_mirror::test_mirror(&url)
        .await
        .map_err(|e| AppError::Other(format!("Mirror test failed: {}", e)))
}

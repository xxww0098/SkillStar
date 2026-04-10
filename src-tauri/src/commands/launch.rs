//! Launch Deck Tauri commands.
//!
//! Thin wrappers around `core::launch_deck` and `core::terminal_backend`.

use crate::core::{error::AppError, launch_deck, terminal_backend};

#[tauri::command]
pub async fn get_launch_config(
    project_name: String,
) -> Result<Option<launch_deck::LaunchConfig>, AppError> {
    let name = project_name;
    tokio::task::spawn_blocking(move || Ok(launch_deck::load_config(&name))).await?
}

#[tauri::command]
pub async fn save_launch_config(config: launch_deck::LaunchConfig) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        launch_deck::save_config(&config)
            .map_err(|e| AppError::Other(format!("Failed to save launch config: {}", e)))
    })
    .await?
}

#[tauri::command]
pub async fn delete_launch_config(project_name: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        launch_deck::delete_config(&project_name)
            .map_err(|e| AppError::Other(format!("Failed to delete launch config: {}", e)))
    })
    .await?
}

/// Deploy: save current config → validate → generate script → open terminal.
///
/// Accepts the config directly from the frontend to avoid stale-read race
/// conditions (the auto-save debounce may not have flushed yet).
#[tauri::command]
pub async fn deploy_launch(
    config: launch_deck::LaunchConfig,
    project_path: String,
) -> Result<terminal_backend::DeployResult, AppError> {
    tokio::task::spawn_blocking(move || {
        // Persist first so disk state is always in sync
        launch_deck::save_config(&config)
            .map_err(|e| AppError::Other(format!("Failed to save config before deploy: {}", e)))?;
        terminal_backend::deploy(&config, &project_path)
            .map_err(|e| AppError::Other(format!("Deploy failed: {}", e)))
    })
    .await?
}

/// Check tmux availability.
#[tauri::command]
pub async fn check_tmux() -> Result<terminal_backend::TmuxStatus, AppError> {
    Ok(terminal_backend::check_tmux())
}

/// List all agent CLIs with installation status.
#[tauri::command]
pub async fn list_agent_clis() -> Result<Vec<terminal_backend::AgentCliInfo>, AppError> {
    Ok(terminal_backend::list_available_clis())
}

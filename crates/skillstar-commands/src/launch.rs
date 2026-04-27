use skillstar_infra::error::AppError;
use skillstar_terminal::config::{
    LaunchConfig, delete_config, deployable_layout, load_config, save_config, validate,
};
use skillstar_terminal::registry::list_available_clis;
use skillstar_terminal::script_builder::generate_single_script_for_current_os;
use skillstar_terminal::session::session_name;
use skillstar_terminal::terminal_launcher::open_script_in_terminal_with_kind;
use skillstar_terminal::types::{AgentCliInfo, DeployResult};

#[tauri::command]
pub async fn get_launch_config(project_name: String) -> Result<Option<LaunchConfig>, AppError> {
    let name = project_name;
    tokio::task::spawn_blocking(move || Ok(load_config(&name))).await?
}

#[tauri::command]
pub async fn save_launch_config(config: LaunchConfig) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        save_config(&config)
            .map_err(|e| AppError::Other(format!("Failed to save launch config: {}", e)))
    })
    .await?
}

#[tauri::command]
pub async fn delete_launch_config(project_name: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        delete_config(&project_name)
            .map_err(|e| AppError::Other(format!("Failed to delete launch config: {}", e)))
    })
    .await?
}

#[tauri::command]
pub async fn deploy_launch(
    config: LaunchConfig,
    project_path: String,
) -> Result<DeployResult, AppError> {
    tokio::task::spawn_blocking(move || {
        save_config(&config)
            .map_err(|e| AppError::Other(format!("Failed to save config before deploy: {}", e)))?;

        if let Err(errors) = validate(&config) {
            return Ok(DeployResult {
                success: false,
                message: errors.join("; "),
                script_path: None,
            });
        }

        let (script, extension, script_kind) =
            generate_single_script_for_current_os(deployable_layout(&config), &project_path);

        let script_path = std::env::temp_dir().join(format!(
            "ss-launch-{}.{}",
            session_name(&config.project_name),
            extension
        ));
        std::fs::write(&script_path, &script)
            .map_err(|e| AppError::Other(format!("Failed to write launch script: {}", e)))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| AppError::Other(format!("Failed to set script permissions: {}", e)))?;
        }

        open_script_in_terminal_with_kind(&script_path, script_kind)
            .map_err(|e| AppError::Other(format!("Failed to open terminal: {}", e)))?;

        Ok(DeployResult {
            success: true,
            message: format!("Launched '{}'", config.project_name),
            script_path: Some(script_path.to_string_lossy().to_string()),
        })
    })
    .await?
}

#[tauri::command]
pub async fn list_agent_clis() -> Result<Vec<AgentCliInfo>, AppError> {
    Ok(list_available_clis())
}

use crate::core::patrol::{self, PatrolManager, PatrolStatus, PatrolConfig};
use tauri::State;

#[tauri::command]
pub async fn start_patrol(
    app: tauri::AppHandle,
    state: State<'_, PatrolManager>,
    interval_secs: u64,
) -> Result<(), String> {
    state
        .start(app.clone(), interval_secs)
        .await
        .map_err(|e| e.to_string())?;
    set_dock_visible(app, true).await
}

#[tauri::command]
pub async fn stop_patrol(app: tauri::AppHandle, state: State<'_, PatrolManager>) -> Result<(), String> {
    state.stop().await;
    set_dock_visible(app, false).await
}

#[tauri::command]
pub async fn get_patrol_status(state: State<'_, PatrolManager>) -> Result<PatrolStatus, String> {
    Ok(state.status().await)
}

#[tauri::command]
pub async fn get_patrol_config() -> Result<PatrolConfig, String> {
    Ok(patrol::load_config())
}

#[tauri::command]
pub async fn save_patrol_config(config: PatrolConfig) -> Result<(), String> {
    patrol::save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_dock_visible(app: tauri::AppHandle, visible: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let policy = if visible {
            ActivationPolicy::Regular
        } else {
            ActivationPolicy::Accessory
        };
        app.set_activation_policy(policy)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

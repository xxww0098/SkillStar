use crate::{
    ExitControl,
    core::patrol::{PatrolManager, PatrolStatus},
};
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
    Ok(())
}

#[tauri::command]
pub async fn stop_patrol(
    _app: tauri::AppHandle,
    state: State<'_, PatrolManager>,
) -> Result<(), String> {
    state.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn get_patrol_status(state: State<'_, PatrolManager>) -> Result<PatrolStatus, String> {
    Ok(state.status().await)
}

#[tauri::command]
pub async fn app_quit(
    app: tauri::AppHandle,
    state: State<'_, PatrolManager>,
    exit_control: State<'_, ExitControl>,
) -> Result<(), String> {
    state.stop().await;
    exit_control.allow_next_exit();
    app.exit(0);
    Ok(())
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

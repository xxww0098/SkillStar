use crate::core::{
    app_shell::{ExitControl, refresh_tray_menu},
    infra::error::AppError,
    patrol::{PatrolManager, PatrolStatus},
};
use tauri::{Emitter, State};

#[tauri::command]
pub async fn start_patrol(
    app: tauri::AppHandle,
    state: State<'_, PatrolManager>,
    interval_secs: u64,
) -> Result<(), AppError> {
    state
        .start(app.clone(), interval_secs)
        .map_err(|e| AppError::Other(e.to_string()))?;
    let _ = app.emit("patrol://enabled-changed", true);
    refresh_tray_menu(&app).map_err(AppError::Other)?;
    Ok(())
}

#[tauri::command]
pub async fn stop_patrol(
    app: tauri::AppHandle,
    state: State<'_, PatrolManager>,
) -> Result<(), AppError> {
    state.stop();
    let _ = app.emit("patrol://enabled-changed", false);
    refresh_tray_menu(&app).map_err(AppError::Other)?;
    Ok(())
}

#[tauri::command]
pub async fn get_patrol_status(state: State<'_, PatrolManager>) -> Result<PatrolStatus, AppError> {
    Ok(state.status())
}

#[tauri::command]
pub async fn set_patrol_enabled(
    app: tauri::AppHandle,
    state: State<'_, PatrolManager>,
    enabled: bool,
) -> Result<(), AppError> {
    if enabled {
        state
            .set_enabled(true)
            .map_err(|e| AppError::Other(e.to_string()))?;
    } else {
        state.stop();
    }

    let _ = app.emit("patrol://enabled-changed", enabled);
    refresh_tray_menu(&app).map_err(AppError::Other)?;
    Ok(())
}

#[tauri::command]
pub async fn app_quit(
    app: tauri::AppHandle,
    state: State<'_, PatrolManager>,
    exit_control: State<'_, ExitControl>,
) -> Result<(), AppError> {
    state.stop();
    exit_control.allow_next_exit();
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn set_dock_visible(app: tauri::AppHandle, _visible: bool) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let policy = if _visible {
            ActivationPolicy::Regular
        } else {
            ActivationPolicy::Accessory
        };
        app.set_activation_policy(policy)
            .map_err(|e| AppError::Other(e.to_string()))?;
    }
    let _ = app;
    Ok(())
}

//! Thin adapters for desktop-app multi-instance (create / start / stop / list).

use skillstar_app::instances::{self, AppInstanceDto, DesktopAppDto};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn list_desktop_apps() -> Vec<DesktopAppDto> {
    instances::list_desktop_apps()
}

#[tauri::command]
pub fn list_app_instances(app: String) -> Result<Vec<AppInstanceDto>, AppError> {
    instances::list_instances(&app)
}

#[tauri::command]
pub fn create_app_instance(app: String, name: String) -> Result<AppInstanceDto, AppError> {
    instances::create_instance(&app, name)
}

#[tauri::command]
pub fn start_app_instance(id: String) -> Result<AppInstanceDto, AppError> {
    instances::start_instance(&id)
}

#[tauri::command]
pub fn stop_app_instance(id: String) -> Result<AppInstanceDto, AppError> {
    instances::stop_instance(&id)
}

#[tauri::command]
pub fn delete_app_instance(id: String) -> Result<(), AppError> {
    instances::delete_instance(&id)
}

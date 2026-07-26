//! Thin Tauri adapters for application storage maintenance use cases.

use skillstar_app::storage_maintenance;
use skillstar_core::infra::error::AppError;

pub use skillstar_app::storage_maintenance::{CacheCleanResult, StorageOverview};

#[tauri::command]
pub async fn get_storage_overview() -> Result<StorageOverview, AppError> {
    storage_maintenance::get_storage_overview().await
}

#[tauri::command]
pub async fn clear_all_caches() -> Result<CacheCleanResult, AppError> {
    storage_maintenance::clear_all_caches().await
}

#[tauri::command]
pub async fn force_delete_installed_skills() -> Result<usize, AppError> {
    storage_maintenance::force_delete_installed_skills().await
}

#[tauri::command]
pub async fn force_delete_repo_caches() -> Result<usize, AppError> {
    storage_maintenance::force_delete_repo_caches().await
}

#[tauri::command]
pub async fn force_delete_app_config() -> Result<usize, AppError> {
    storage_maintenance::force_delete_app_config().await
}

#[tauri::command]
pub async fn clean_broken_skills() -> Result<usize, AppError> {
    storage_maintenance::clean_broken_skills().await
}

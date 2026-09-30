//! Saved groups. The write lives in `skillstar-gateway`. This file does not
//! decide whether the member list cycles.

use skillstar_app::models::{self, SavedGroupDto};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn get_saved_groups() -> Vec<SavedGroupDto> {
    models::load_saved_groups()
}

#[tauri::command]
pub fn save_group_members(id: String, members: Vec<String>) -> Result<(), AppError> {
    models::save_group_members(&id, &members).map_err(|error| AppError::Other(error.to_string()))
}

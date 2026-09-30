//! Named profiles. The write and the apply live in `skillstar-gateway`.

use skillstar_app::models::{self, ProfileAgentDto, ProfileApplyDto};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn get_profile_names() -> Vec<String> {
    models::load_profile_names()
}

#[tauri::command]
pub fn save_profile(name: String, agents: Vec<ProfileAgentDto>) -> Result<(), AppError> {
    models::save_profile_agents(&name, &agents).map_err(|error| AppError::Other(error.to_string()))
}

#[tauri::command]
pub fn apply_profile(name: String) -> Result<ProfileApplyDto, AppError> {
    models::apply_saved_profile(&name).map_err(|error| AppError::Other(error.to_string()))
}

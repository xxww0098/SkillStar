//! Models page board. Names only; the store read lives in `skillstar-app`.

use skillstar_app::models::board::{ModelsBoardDto, load_models_board};
use skillstar_app::models::{
    ModelChoiceDto, RecentCallDto, load_model_choices, load_recent_calls, save_agent,
};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn get_models_board() -> Result<ModelsBoardDto, AppError> {
    load_models_board().map_err(|error| AppError::Other(error.to_string()))
}

#[tauri::command]
pub fn get_model_choices() -> Vec<ModelChoiceDto> {
    load_model_choices()
}

#[tauri::command]
pub fn get_recent_calls() -> Vec<RecentCallDto> {
    load_recent_calls()
}

#[tauri::command]
pub fn save_agent_model(agent_id: String, model_ref: String) -> Result<(), AppError> {
    save_agent(&agent_id, &model_ref).map_err(|error| AppError::Other(error.to_string()))
}

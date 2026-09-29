//! Models page board. Names only; the store read lives in `skillstar-app`.

use skillstar_app::models::board::{ModelsBoardDto, load_models_board};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn get_models_board() -> Result<ModelsBoardDto, AppError> {
    load_models_board().map_err(|error| AppError::Other(error.to_string()))
}

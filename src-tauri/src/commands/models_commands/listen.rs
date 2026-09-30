//! Listen mode. The file write lives in `skillstar-gateway`.

use skillstar_app::models::{self, SaveListenControlError};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn get_listen_mode() -> String {
    models::load_listen_mode()
}

#[tauri::command]
pub fn save_listen_mode(mode: String) -> Result<(), AppError> {
    models::save_listen_mode(&mode)
        .map_err(|error: SaveListenControlError| AppError::Other(error.to_string()))
}

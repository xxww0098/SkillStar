//! Model display names. The file write lives in `skillstar-gateway`.

use skillstar_app::models::{self, SaveModelNameControlError};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn save_model_name(id: String, name: String) -> Result<(), AppError> {
    models::save_model_name(&id, &name)
        .map_err(|error: SaveModelNameControlError| AppError::Other(error.to_string()))
}

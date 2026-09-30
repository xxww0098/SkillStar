//! Catalog effort levels for one model. The list is not invented here.

use skillstar_app::models;

#[tauri::command]
pub fn model_efforts(id: String) -> Vec<String> {
    models::model_efforts(&id)
}

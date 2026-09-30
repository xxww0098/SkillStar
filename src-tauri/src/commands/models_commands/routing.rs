//! Routing control. The write lives in `skillstar-gateway`. This file does
//! not open the provider store.

use skillstar_app::models::{RoutingPage, load_routing_page, save_routing_control};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn get_routing_page(provider_id: String) -> RoutingPage {
    load_routing_page(&provider_id)
}

#[tauri::command]
pub fn save_routing(
    owner: String,
    id: String,
    routing: String,
    affinity: String,
) -> Result<(), AppError> {
    save_routing_control(&owner, &id, &routing, &affinity)
        .map_err(|error| AppError::Other(error.to_string()))
}

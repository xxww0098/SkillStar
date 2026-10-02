//! Models page board. Names only; the store read lives in `skillstar-app`.

use skillstar_app::models::board::{ModelsBoardDto, load_models_board};
use skillstar_app::models::{
    LedgerQuery, ModelChoiceDto, PAGE_KEEP, RecentCallDto, load_ledger_page, load_model_choices,
    load_recent_calls, save_agent,
};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn get_models_board() -> Result<ModelsBoardDto, AppError> {
    load_models_board().map_err(|error| AppError::Other(error.to_string()))
}

#[tauri::command]
pub fn get_model_choices(agent_id: String) -> Vec<ModelChoiceDto> {
    load_model_choices(&agent_id)
}

#[tauri::command]
pub fn get_recent_calls() -> Vec<RecentCallDto> {
    load_recent_calls()
}

/// One ledger page, newest first. `None` dimensions filter nothing; the
/// limit defaults to the page size and the offset walks from the newest end.
#[tauri::command]
pub fn get_ledger_page(
    agent: Option<String>,
    session: Option<String>,
    catalog: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Vec<RecentCallDto> {
    load_ledger_page(LedgerQuery {
        agent,
        session,
        catalog,
        limit: limit.unwrap_or(PAGE_KEEP),
        skip: offset.unwrap_or(0),
    })
}

#[tauri::command]
pub fn save_agent_model(agent_id: String, model_ref: String) -> Result<(), AppError> {
    save_agent(&agent_id, &model_ref).map_err(|error| AppError::Other(error.to_string()))
}

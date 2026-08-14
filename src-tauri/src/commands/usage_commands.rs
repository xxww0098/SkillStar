//! Thin Tauri adapters for the `skillstar_app::usage` application facade.

use std::collections::HashMap;

use skillstar_app::usage;
use skillstar_core::infra::error::AppError;
use tauri::AppHandle;

pub use skillstar_app::usage::{
    CatalogEntryDto, CliAccountStateDto, CreateSubscriptionInput, OAuthStartDto,
    SubscriptionAlertDto, SubscriptionDto, SwitchOutcomeDto, UpdateSubscriptionInput, UsageSummary,
};

#[tauri::command]
pub fn list_usage_catalog() -> Vec<CatalogEntryDto> {
    usage::list_usage_catalog()
}

#[tauri::command]
pub fn list_subscriptions() -> Result<Vec<SubscriptionDto>, AppError> {
    usage::list_subscriptions()
}

#[tauri::command]
pub fn create_subscription(
    app: AppHandle,
    input: CreateSubscriptionInput,
) -> Result<SubscriptionDto, AppError> {
    let dto = usage::create_subscription(input)?;
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(dto)
}

#[tauri::command]
pub async fn update_subscription(
    app: AppHandle,
    id: String,
    input: UpdateSubscriptionInput,
) -> Result<SubscriptionDto, AppError> {
    let dto = usage::update_subscription(id, input).await?;
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(dto)
}

#[tauri::command]
pub async fn delete_subscription(app: AppHandle, id: String) -> Result<(), AppError> {
    usage::delete_subscription(id.clone()).await?;
    super::usage_windows::close_card_for_subscription(&app, &id);
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn reorder_subscriptions(app: AppHandle, ids: Vec<String>) -> Result<(), AppError> {
    usage::reorder_subscriptions(ids)?;
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(())
}

#[tauri::command]
pub async fn refresh_subscription_usage(
    app: AppHandle,
    id: String,
) -> Result<SubscriptionDto, AppError> {
    let dto = usage::refresh_subscription_usage(id).await?;
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(dto)
}

/// `catalogId` scopes the sweep to one provider (the Grok page refreshes only
/// `xai`); omitted / null refreshes everything.
#[tauri::command]
pub async fn refresh_all_subscriptions(
    app: AppHandle,
    catalog_id: Option<String>,
) -> Result<Vec<SubscriptionDto>, AppError> {
    let dtos = usage::refresh_all_subscriptions(catalog_id).await?;
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(dtos)
}

#[tauri::command]
pub fn get_subscription_alerts() -> Result<Vec<SubscriptionAlertDto>, AppError> {
    usage::get_subscription_alerts()
}

#[tauri::command]
pub fn dismiss_subscription_alert(alert_id: String) -> Result<(), AppError> {
    usage::dismiss_subscription_alert(alert_id)
}

#[tauri::command]
pub fn get_usage_summary() -> Result<UsageSummary, AppError> {
    usage::get_usage_summary()
}

#[tauri::command]
pub async fn start_oauth_login(
    catalog_id: String,
    region: Option<String>,
    subscription_id: Option<String>,
) -> Result<OAuthStartDto, AppError> {
    usage::start_oauth_login(catalog_id, region, subscription_id).await
}

#[tauri::command]
pub async fn import_subscription_from_local(
    app: AppHandle,
    catalog_id: String,
) -> Result<SubscriptionDto, AppError> {
    let dto = usage::import_subscription_from_local(catalog_id).await?;
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(dto)
}

#[tauri::command]
pub async fn await_oauth_completion(
    app: AppHandle,
    pending_id: String,
) -> Result<SubscriptionDto, AppError> {
    let dto = usage::await_oauth_completion(pending_id).await?;
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(dto)
}

#[tauri::command]
pub async fn submit_oauth_callback(
    pending_id: String,
    callback_input: String,
) -> Result<(), AppError> {
    usage::submit_oauth_callback(pending_id, callback_input).await
}

#[tauri::command]
pub fn cancel_oauth_login(pending_id: String) -> Result<(), AppError> {
    usage::cancel_oauth_login(pending_id)
}

#[tauri::command]
pub fn get_active_subscriptions() -> Result<HashMap<String, String>, AppError> {
    usage::get_active_subscriptions()
}

#[tauri::command]
pub async fn set_active_subscription(
    app: AppHandle,
    subscription_id: String,
) -> Result<SubscriptionDto, AppError> {
    let dto = usage::set_active_subscription(subscription_id).await?;
    if dto.is_active {
        super::usage_windows::emit_active_changed(&app, &dto.catalog_id, &dto.id);
    }
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(dto)
}

#[tauri::command]
pub async fn switch_active_subscription_to_cli(
    app: AppHandle,
    catalog_id: String,
) -> Result<SwitchOutcomeDto, AppError> {
    let dto = usage::switch_active_subscription_to_cli(catalog_id).await?;
    let _ = crate::core::app_shell::refresh_tray_and_dock_menu(&app);
    Ok(dto)
}

/// Which account each CLI is actually serving right now.
///
/// The companion to `get_active_subscriptions`: that one returns the pin (what
/// the user asked for), this one returns what is on disk (what the CLI will
/// read). The badge follows this; the pin only fills in for catalogs that have
/// no CLI behind them.
#[tauri::command]
pub async fn reconcile_cli_accounts() -> Result<HashMap<String, CliAccountStateDto>, AppError> {
    usage::reconcile_cli_accounts().await
}

#[tauri::command]
pub fn clear_active_subscription(catalog_id: String) -> Result<(), AppError> {
    usage::clear_active_subscription(catalog_id)
}

#[tauri::command]
pub fn get_subscription_api_key(id: String) -> Result<Option<String>, AppError> {
    usage::get_subscription_api_key(id)
}

//! Tauri commands for quota, usage, and speedtest.

use crate::core::infra::error::AppError;
use crate::core::model_config::{
    codex_accounts, speedtest,
};

use super::models_dto::*;

#[tauri::command]
pub async fn refresh_codex_quota(
    account_id: String,
) -> Result<codex_accounts::CodexQuota, AppError> {
    codex_accounts::refresh_account_quota(&account_id)
        .await
        .map_err(|e| AppError::Other(format!("Quota refresh error: {e}")))
}

#[tauri::command]
pub async fn refresh_all_codex_quotas()
-> Result<Vec<(String, Result<codex_accounts::CodexQuota, String>)>, AppError> {
    Ok(codex_accounts::refresh_all_quotas().await)
}

#[tauri::command]
pub async fn refresh_gemini_quota(
    app_id: String,
    provider_id: String,
) -> Result<crate::core::model_config::gemini_quota::GeminiQuota, AppError> {
    crate::core::model_config::gemini_quota::refresh_gemini_quota(&app_id, &provider_id)
        .await
        .map_err(|e| AppError::Other(format!("Gemini quota refresh error: {e}")))
}

#[tauri::command]
pub async fn get_provider_usage_tracker(
    app_id: String,
    history_days: Option<usize>,
    history_limit: Option<usize>,
) -> Result<AppUsageTrackerPayload, AppError> {
    let tracker = skillstar_model_config::usage_tracker::get_app_usage_tracker(
        &app_id,
        history_days.unwrap_or(7),
        history_limit.unwrap_or(20),
    )
    .map_err(|e| AppError::Other(format!("Get usage tracker error: {e}")))?;
    Ok(tracker.into())
}

#[tauri::command]
pub async fn get_provider_usage_summary(
    app_id: String,
    provider_id: String,
    history_days: Option<usize>,
    history_limit: Option<usize>,
) -> Result<Option<ProviderUsageSummaryPayload>, AppError> {
    let summary = skillstar_model_config::usage_tracker::get_provider_usage_summary(
        &app_id,
        &provider_id,
        history_days.unwrap_or(7),
        history_limit.unwrap_or(20),
    )
    .map_err(|e| AppError::Other(format!("Get provider usage summary error: {e}")))?;
    Ok(summary.map(Into::into))
}

#[tauri::command]
pub async fn test_model_endpoints(
    urls: Vec<String>,
    timeout_secs: Option<u64>,
) -> Result<Vec<speedtest::EndpointLatency>, AppError> {
    speedtest::test_endpoints(urls, timeout_secs)
        .await
        .map_err(|e| AppError::Other(format!("Endpoint test error: {e}")))
}

//! Latency, connection, model-discovery and balance diagnostics commands.
//!
//! Thin forwarders over `skillstar_models::{latency, diagnostics}` — all HTTP
//! construction and response parsing lives in the domain crate.

use super::*;

use skillstar_models::diagnostics as diag;

/// Probe multiple API endpoints in parallel and return per-URL latency.
#[tauri::command]
pub async fn test_endpoints_latency(
    urls: Vec<String>,
    api_key: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<Vec<EndpointLatencyResult>, AppError> {
    Ok(latency::test_endpoints_latency(urls, api_key, timeout_ms).await)
}

/// Test the latency of a single provider by sending a GET request to its /models endpoint.
///
/// Returns a `LatencyResult` with timing, status, and optional error info.
#[tauri::command]
pub async fn test_provider_latency(
    app_id: String,
    provider_id: String,
    base_url: String,
    api_key: String,
    timeout_ms: Option<u64>,
) -> Result<LatencyResult, AppError> {
    let result =
        latency::test_provider_latency(&provider_id, &app_id, &base_url, &api_key, timeout_ms)
            .await;
    Ok(result)
}

/// Test a provider's connection (reachability probe or minimal chat probe).
///
/// See `skillstar_models::diagnostics::test_provider_connection` for the probe
/// modes and status taxonomy.
#[tauri::command]
pub async fn test_provider_connection(
    base_url: String,
    api_key: String,
    model: String,
    format: String,
) -> Result<ConnectionTestResult, AppError> {
    diag::test_provider_connection(&base_url, &api_key, &model, &format)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Fetch available models from a provider's API endpoint.
///
/// See `skillstar_models::diagnostics::fetch_provider_models` for the request
/// shape and response contract.
#[tauri::command]
pub async fn fetch_provider_models(
    url: String,
    api_key: String,
    timeout_ms: Option<u64>,
) -> Result<Vec<String>, AppError> {
    diag::fetch_provider_models(&url, &api_key, timeout_ms)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Fetch available models plus normalized metadata for OpenCode and UI display.
#[tauri::command]
pub async fn fetch_provider_model_catalog(
    url: String,
    api_key: String,
    timeout_ms: Option<u64>,
) -> Result<ModelCatalogFetchResult, AppError> {
    diag::fetch_provider_model_catalog(&url, &api_key, timeout_ms)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Query the remaining balance/quota for a provider.
///
/// Returns the raw JSON response (frontend parses based on preset_id/balance_parser).
#[tauri::command]
pub async fn query_provider_balance(
    preset_id: String,
    api_key: String,
    _base_url: String,
) -> Result<serde_json::Value, AppError> {
    diag::query_provider_balance(&preset_id, &api_key)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

//! Latency, connection, model-discovery and balance diagnostics commands.
//!
//! Thin forwarders over `skillstar_models::{latency, diagnostics}` — all HTTP
//! construction and response parsing lives in the domain crate.
//!
//! ## Credentials are resolved here, not passed in
//!
//! Every probe below used to take the plaintext API key as a parameter, which
//! made the renderer the source of the credential: it had to hold the key, keep
//! it in a query cache, and send it back across IPC on every probe. Each of
//! those is somewhere the key can be observed, and none of it was necessary —
//! the backend owns the store the key came from.
//!
//! The commands now take `provider_id` and call
//! `providers::resolve_connection`. The endpoint URLs come from the same
//! lookup, which also closes a real drift: the frontend was assembling
//! `models_url` itself with its own fallback rule.

use super::*;

use skillstar_models::diagnostics as diag;

/// Probe a provider's endpoint candidates in parallel and return per-URL latency.
///
/// `urls` stays a parameter because the speed test is explicitly about
/// comparing *alternative* URLs the user is choosing between — several of which
/// are not what the row currently points at. The credential still comes from
/// the row.
#[tauri::command]
pub async fn test_endpoints_latency(
    urls: Vec<String>,
    provider_id: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<Vec<EndpointLatencyResult>, AppError> {
    let api_key = match provider_id.as_deref() {
        Some(id) if !id.is_empty() => {
            let conn = providers::resolve_connection(id).map_err(|e| AppError::Other(e.to_string()))?;
            Some(conn.api_key).filter(|k| !k.is_empty())
        }
        _ => None,
    };
    Ok(latency::test_endpoints_latency(urls, api_key, timeout_ms).await)
}

/// Test the latency of a single provider by sending a GET to its models endpoint.
#[tauri::command]
pub async fn test_provider_latency(
    app_id: String,
    provider_id: String,
    timeout_ms: Option<u64>,
) -> Result<LatencyResult, AppError> {
    let conn =
        providers::resolve_connection(&provider_id).map_err(|e| AppError::Other(e.to_string()))?;
    Ok(latency::test_provider_latency(
        &provider_id,
        &app_id,
        &conn.base_url_openai,
        &conn.api_key,
        timeout_ms,
    )
    .await)
}

/// Test a provider's connection (reachability probe or minimal chat probe).
///
/// See `skillstar_models::diagnostics::test_provider_connection` for the probe
/// modes and status taxonomy.
#[tauri::command]
pub async fn test_provider_connection(
    provider_id: String,
    model: String,
    format: String,
) -> Result<ConnectionTestResult, AppError> {
    let conn =
        providers::resolve_connection(&provider_id).map_err(|e| AppError::Other(e.to_string()))?;
    // The probe format decides which endpoint is even meaningful: an Anthropic
    // probe against the OpenAI base URL tests nothing.
    let base_url = if format == "anthropic" {
        &conn.base_url_anthropic
    } else {
        &conn.base_url_openai
    };
    diag::test_provider_connection(base_url, &conn.api_key, &model, &format)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Fetch available models from a provider's discovery endpoint.
#[tauri::command]
pub async fn fetch_provider_models(
    provider_id: String,
    timeout_ms: Option<u64>,
) -> Result<Vec<String>, AppError> {
    let conn =
        providers::resolve_connection(&provider_id).map_err(|e| AppError::Other(e.to_string()))?;
    diag::fetch_provider_models(&conn.models_endpoint(), &conn.api_key, timeout_ms)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Fetch available models plus normalized metadata for OpenCode and UI display.
#[tauri::command]
pub async fn fetch_provider_model_catalog(
    provider_id: String,
    timeout_ms: Option<u64>,
) -> Result<ModelCatalogFetchResult, AppError> {
    let conn =
        providers::resolve_connection(&provider_id).map_err(|e| AppError::Other(e.to_string()))?;
    diag::fetch_provider_model_catalog(&conn.models_endpoint(), &conn.api_key, timeout_ms)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Query the remaining balance/quota for a provider.
///
/// Returns the raw JSON response (the frontend parses it by `balance_parser`).
#[tauri::command]
pub async fn query_provider_balance(
    provider_id: String,
) -> Result<serde_json::Value, AppError> {
    let conn =
        providers::resolve_connection(&provider_id).map_err(|e| AppError::Other(e.to_string()))?;
    // The preset id is what selects the response parser, and it lives on the
    // row — the frontend was previously passing it back to us alongside the key.
    let preset_id = conn.preset_id.unwrap_or_default();
    diag::query_provider_balance(&preset_id, &conn.api_key)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

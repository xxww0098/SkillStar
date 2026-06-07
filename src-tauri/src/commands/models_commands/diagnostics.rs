//! Latency, connection, model-discovery and balance diagnostics commands.
//!
//! Carved out of `models_commands` mechanically — no logic changes.

use super::*;

const CLIPROXY_MODEL_REGISTRY_URL: &str = "https://raw.githubusercontent.com/router-for-me/CLIProxyAPI/main/internal/registry/models/models.json";
const MODELS_DEV_REGISTRY_URL: &str = "https://models.dev/api.json";

/// Probe multiple API endpoints in parallel and return per-URL latency.
#[tauri::command]
pub async fn test_endpoints_latency(
    urls: Vec<String>,
    api_key: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<Vec<EndpointLatencyResult>, String> {
    Ok(latency::test_endpoints_latency(urls, api_key, timeout_ms).await)
}

// ---------------------------------------------------------------------------
// Latency test commands
// ---------------------------------------------------------------------------

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
) -> Result<LatencyResult, String> {
    let result =
        latency::test_provider_latency(&provider_id, &app_id, &base_url, &api_key, timeout_ms)
            .await;
    Ok(result)
}

/// Test latency for all providers of a given app_id sequentially with 100ms delay between tests.
///
/// Reads the provider store, iterates all providers for the specified app_id,
/// and tests each one sequentially to avoid network contention.
#[tauri::command]
pub async fn test_all_providers_latency(app_id: String) -> Result<Vec<LatencyResult>, String> {
    let store = providers::read_store().map_err(|e| e.to_string())?;

    let app_providers = match app_id.as_str() {
        "claude" => &store.claude,
        "codex" => &store.codex,
        "opencode" => &store.opencode,
        "gemini" => &store.gemini,
        _ => return Err(format!("Unknown app_id: {}", app_id)),
    };

    let mut results = Vec::new();

    for (id, entry) in &app_providers.providers {
        // Parse settings_config to get base_url and api_key
        let settings: ProviderSettings = serde_json::from_value(entry.settings_config.clone())
            .map_err(|e| format!("Failed to parse settings for provider '{}': {}", id, e))?;

        let result = latency::test_provider_latency(
            id,
            &app_id,
            &settings.base_url,
            &settings.api_key,
            settings.timeout_ms,
        )
        .await;

        results.push(result);

        // 100ms delay between tests to avoid network saturation
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    Ok(results)
}

/// Test a provider's connection.
///
/// Two modes:
/// - **Reachability probe (model empty)**: sends `GET {base}/models` with both
///   `Authorization: Bearer` and `x-api-key` headers so the same request works
///   against OpenAI-compatible, Anthropic, and hybrid endpoints (e.g. DeepSeek).
///   This is what the sidebar "测试连接" button uses — it only needs to verify
///   reachability and auth.
/// - **Chat probe (model non-empty)**: sends a minimal 1-token chat completion
///   request using the specified `format` endpoint.
///
/// Distinguishes between: success (with latency), auth failure (401/403),
/// timeout, network error, and model unavailable (404).
///
/// # Arguments
/// * `base_url` - The provider's API base URL.
/// * `api_key` - API key for authentication.
/// * `model` - Model identifier to test. Empty string triggers reachability probe.
/// * `format` - `"openai"` or `"anthropic"` — determines endpoint and request format
///   for the chat probe. Ignored for the reachability probe.
#[tauri::command]
pub async fn test_provider_connection(
    base_url: String,
    api_key: String,
    model: String,
    format: String,
) -> Result<ConnectionTestResult, String> {
    use std::time::Duration;
    use tokio::time::Instant;

    let timeout = Duration::from_secs(10);

    let client = skillstar_core::infra::http_client::probe_http_client(timeout)
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let base = base_url.trim_end_matches('/');

    let start = Instant::now();

    let response = if model.trim().is_empty() {
        latency::send_reachability_probe(&client, base, &api_key).await
    } else {
        match format.as_str() {
            "anthropic" => {
                let url = format!("{base}/messages");
                let body = serde_json::json!({
                    "model": model,
                    "messages": [{"role": "user", "content": "hi"}],
                    "max_tokens": 1
                });
                client
                    .post(&url)
                    .header("x-api-key", &api_key)
                    .header("anthropic-version", "2023-06-01")
                    .header("content-type", "application/json")
                    .json(&body)
                    .send()
                    .await
            }
            _ => {
                // Default to OpenAI-compatible format
                let url = format!("{base}/chat/completions");
                let body = serde_json::json!({
                    "model": model,
                    "messages": [{"role": "user", "content": "hi"}],
                    "max_tokens": 1,
                    "temperature": 0
                });
                client
                    .post(&url)
                    .header("Authorization", format!("Bearer {api_key}"))
                    .header("content-type", "application/json")
                    .json(&body)
                    .send()
                    .await
            }
        }
    };

    match response {
        Ok(resp) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let status_code = resp.status().as_u16();

            match status_code {
                200..=299 => Ok(ConnectionTestResult {
                    status: "ok".to_string(),
                    latency_ms: Some(elapsed_ms),
                    error: None,
                }),
                401 | 403 => Ok(ConnectionTestResult {
                    status: "auth_failed".to_string(),
                    latency_ms: None,
                    error: Some(format!("HTTP {status_code}")),
                }),
                404 => Ok(ConnectionTestResult {
                    status: "model_unavailable".to_string(),
                    latency_ms: None,
                    error: Some(format!("HTTP {status_code}")),
                }),
                _ => Ok(ConnectionTestResult {
                    status: "network_error".to_string(),
                    latency_ms: None,
                    error: Some(format!("HTTP {status_code}")),
                }),
            }
        }
        Err(e) => {
            if e.is_timeout() {
                Ok(ConnectionTestResult {
                    status: "timeout".to_string(),
                    latency_ms: None,
                    error: Some("Request timed out (10s)".to_string()),
                })
            } else if e.is_connect() {
                Ok(ConnectionTestResult {
                    status: "network_error".to_string(),
                    latency_ms: None,
                    error: Some(format!("Connection failed: {e}")),
                })
            } else {
                Ok(ConnectionTestResult {
                    status: "network_error".to_string(),
                    latency_ms: None,
                    error: Some(e.to_string()),
                })
            }
        }
    }
}

// ===========================================================================
// Model Discovery, Balance Query, and Tool Detection Commands
// ===========================================================================

// ---------------------------------------------------------------------------
// Model Discovery
// ---------------------------------------------------------------------------

/// Fetch available models from a provider's API endpoint.
///
/// Sends GET `url` with the API Key as Bearer token (plus Anthropic-compatible
/// headers so servers that only accept `x-api-key` also work). `url` is the
/// provider's unique "fetch models" endpoint — typically the OpenAI-compatible
/// `.../v1/models`. The caller is expected to pass `ProviderEntryFlat.models_url`
/// (or an equivalent fallback computed on the frontend).
///
/// Parses the response as OpenAI-compatible format:
/// `{ "data": [{ "id": "model-name" }] }`, and returns the list of model IDs.
///
/// Uses a configurable timeout (default 15s).
#[tauri::command]
pub async fn fetch_provider_models(
    url: String,
    api_key: String,
    timeout_ms: Option<u64>,
) -> Result<Vec<String>, String> {
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(15_000));

    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(
            "models URL is empty — configure '获取模型 URL' in the provider settings".to_string(),
        );
    }
    let url = trimmed.to_string();

    let client = skillstar_core::infra::http_client::probe_http_client(timeout)
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "请求超时".to_string()
            } else if e.is_connect() {
                format!("网络错误: {}", e)
            } else {
                format!("请求失败: {}", e)
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }

    // Parse as OpenAI-compatible response: { "data": [{ "id": "model-name", ... }] }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("invalid response: {}", e))?;

    let models = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "invalid response: missing or invalid 'data' field".to_string())?;

    let model_ids: Vec<String> = models
        .iter()
        .filter_map(|item| item.get("id").and_then(|id| id.as_str()).map(String::from))
        .collect();

    if model_ids.is_empty() {
        return Err("invalid response: no model IDs found in response".to_string());
    }

    Ok(model_ids)
}

/// Fetch available models plus normalized metadata for OpenCode and UI display.
///
/// The provider endpoint is authoritative for model IDs. Public registries are
/// optional enrichers; if either registry is unavailable, the command still
/// returns the model IDs discovered from the provider.
#[tauri::command]
pub async fn fetch_provider_model_catalog(
    url: String,
    api_key: String,
    timeout_ms: Option<u64>,
) -> Result<ModelCatalogFetchResult, String> {
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(15_000));
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(
            "models URL is empty — configure '获取模型 URL' in the provider settings".to_string(),
        );
    }

    let client = skillstar_core::infra::http_client::probe_http_client(timeout)
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let provider_body = fetch_json_with_auth(&client, trimmed, &api_key).await?;
    let provider_catalog = providers::catalog_from_provider_models(&provider_body);
    if provider_catalog.is_empty() {
        return Err("invalid response: no model IDs found in response".to_string());
    }

    let mut registries = Vec::new();
    let mut metadata_sources = Vec::new();

    for registry_url in [CLIPROXY_MODEL_REGISTRY_URL, MODELS_DEV_REGISTRY_URL] {
        if let Ok(body) = fetch_json_public(&client, registry_url).await {
            let catalog = providers::catalog_from_registry(&body);
            if !catalog.is_empty() {
                registries.push(catalog);
                metadata_sources.push(registry_url.to_string());
            }
        }
    }

    let mut result = providers::merge_model_catalog(provider_catalog, &registries);
    result.metadata_sources = metadata_sources;
    Ok(result)
}

async fn fetch_json_with_auth(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
) -> Result<serde_json::Value, String> {
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(request_error)?;

    parse_json_response(response).await
}

async fn fetch_json_public(
    client: &reqwest::Client,
    url: &str,
) -> Result<serde_json::Value, String> {
    let response = client.get(url).send().await.map_err(request_error)?;
    parse_json_response(response).await
}

async fn parse_json_response(response: reqwest::Response) -> Result<serde_json::Value, String> {
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("invalid response: {}", e))
}

fn request_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "请求超时".to_string()
    } else if e.is_connect() {
        format!("网络错误: {}", e)
    } else {
        format!("请求失败: {}", e)
    }
}

// ---------------------------------------------------------------------------
// Balance Query
// ---------------------------------------------------------------------------

/// Query the remaining balance/quota for a provider.
///
/// Looks up the preset's `balance_endpoint` from the preset registry using `preset_id`.
/// Sends GET to the balance endpoint with the API Key as Bearer token.
/// Returns the raw JSON response (frontend parses based on preset_id/balance_parser).
///
/// Uses a 10-second timeout.
#[tauri::command]
pub async fn query_provider_balance(
    preset_id: String,
    api_key: String,
    _base_url: String,
) -> Result<serde_json::Value, String> {
    // Look up the preset's balance_endpoint
    let presets = providers::get_all_presets_flat();
    let preset = presets
        .iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| format!("Unknown preset_id: '{}'", preset_id))?;

    let balance_endpoint = preset
        .balance_endpoint
        .as_ref()
        .ok_or_else(|| format!("Preset '{}' does not support balance queries", preset_id))?;

    // Some balance endpoints may be relative to the base_url, but our presets
    // use absolute URLs. Use the endpoint as-is.
    let url = balance_endpoint.clone();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "查询超时".to_string()
            } else if e.is_connect() {
                format!("网络错误: {}", e)
            } else {
                format!("查询失败: {}", e)
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}: 余额查询失败", status.as_u16()));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    Ok(body)
}

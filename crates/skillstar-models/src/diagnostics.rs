//! Provider diagnostics beyond raw latency: connection tests (reachability /
//! minimal chat probe), model discovery against the provider's `/models`
//! endpoint plus public registries, and preset balance queries.
//!
//! All HTTP goes through [`probe_http_client`] (the workspace-wide client that
//! honours the user's proxy configuration). Error messages are user-facing and
//! surfaced verbatim by the command layer.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use skillstar_core::infra::http_client::probe_http_client;
use std::time::Duration;
use tokio::time::Instant;

use crate::latency;
use crate::providers::{self, ModelCatalogFetchResult};

/// Public registries used to enrich provider model IDs with metadata.
const CLIPROXY_MODEL_REGISTRY_URL: &str = "https://raw.githubusercontent.com/router-for-me/CLIProxyAPI/main/internal/registry/models/models.json";
const MODELS_DEV_REGISTRY_URL: &str = "https://models.dev/api.json";

/// Result of a provider connection test using a minimal chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    /// `"ok"`, `"auth_failed"`, `"timeout"`, `"network_error"`, `"model_unavailable"`
    pub status: String,
    /// Round-trip latency in milliseconds (only present when status is "ok").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Error description (present for non-ok statuses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Test a provider's connection.
///
/// Two modes:
/// - **Reachability probe (model empty)**: sends `GET {base}/models` with both
///   `Authorization: Bearer` and `x-api-key` headers so the same request works
///   against OpenAI-compatible, Anthropic, and hybrid endpoints (e.g. DeepSeek).
/// - **Chat probe (model non-empty)**: sends a minimal 1-token chat completion
///   request using the specified `format` endpoint (`"openai"` or
///   `"anthropic"`).
///
/// Distinguishes between: success (with latency), auth failure (401/403),
/// timeout, network error, and model unavailable (404). Only client
/// construction fails hard; every probe outcome is a `ConnectionTestResult`.
pub async fn test_provider_connection(
    base_url: &str,
    api_key: &str,
    model: &str,
    format: &str,
) -> Result<ConnectionTestResult> {
    let timeout = Duration::from_secs(10);

    let client = probe_http_client(timeout).map_err(|e| anyhow!("Failed to build HTTP client: {e}"))?;

    let base = base_url.trim_end_matches('/');

    let start = Instant::now();

    let response = if model.trim().is_empty() {
        latency::send_reachability_probe(&client, base, api_key).await
    } else {
        match format {
            "anthropic" => {
                let url = format!("{base}/messages");
                let body = serde_json::json!({
                    "model": model,
                    "messages": [{"role": "user", "content": "hi"}],
                    "max_tokens": 1
                });
                client
                    .post(&url)
                    .header("x-api-key", api_key)
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

    Ok(match response {
        Ok(resp) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let status_code = resp.status().as_u16();

            match status_code {
                200..=299 => ConnectionTestResult {
                    status: "ok".to_string(),
                    latency_ms: Some(elapsed_ms),
                    error: None,
                },
                401 | 403 => ConnectionTestResult {
                    status: "auth_failed".to_string(),
                    latency_ms: None,
                    error: Some(format!("HTTP {status_code}")),
                },
                404 => ConnectionTestResult {
                    status: "model_unavailable".to_string(),
                    latency_ms: None,
                    error: Some(format!("HTTP {status_code}")),
                },
                _ => ConnectionTestResult {
                    status: "network_error".to_string(),
                    latency_ms: None,
                    error: Some(format!("HTTP {status_code}")),
                },
            }
        }
        Err(e) => {
            if e.is_timeout() {
                ConnectionTestResult {
                    status: "timeout".to_string(),
                    latency_ms: None,
                    error: Some("Request timed out (10s)".to_string()),
                }
            } else if e.is_connect() {
                ConnectionTestResult {
                    status: "network_error".to_string(),
                    latency_ms: None,
                    error: Some(format!("Connection failed: {e}")),
                }
            } else {
                ConnectionTestResult {
                    status: "network_error".to_string(),
                    latency_ms: None,
                    error: Some(e.to_string()),
                }
            }
        }
    })
}

/// Fetch available models from a provider's API endpoint.
///
/// Sends GET `url` with the API Key as Bearer token (plus Anthropic-compatible
/// headers so servers that only accept `x-api-key` also work). Parses the
/// response as OpenAI-compatible format `{ "data": [{ "id": "model-name" }] }`
/// and returns the list of model IDs.
pub async fn fetch_provider_models(
    url: &str,
    api_key: &str,
    timeout_ms: Option<u64>,
) -> Result<Vec<String>> {
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(15_000));

    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(anyhow!(
            "models URL is empty — configure '获取模型 URL' in the provider settings"
        ));
    }

    let client = probe_http_client(timeout).map_err(|e| anyhow!("Failed to create HTTP client: {e}"))?;

    let body = fetch_json_with_auth(&client, trimmed, api_key).await?;

    let models = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| anyhow!("invalid response: missing or invalid 'data' field"))?;

    let model_ids: Vec<String> = models
        .iter()
        .filter_map(|item| item.get("id").and_then(|id| id.as_str()).map(String::from))
        .collect();

    if model_ids.is_empty() {
        return Err(anyhow!("invalid response: no model IDs found in response"));
    }

    Ok(model_ids)
}

/// Fetch available models plus normalized metadata for OpenCode and UI display.
///
/// The provider endpoint is authoritative for model IDs. Public registries are
/// optional enrichers; if either registry is unavailable, the result still
/// carries the model IDs discovered from the provider.
pub async fn fetch_provider_model_catalog(
    url: &str,
    api_key: &str,
    timeout_ms: Option<u64>,
) -> Result<ModelCatalogFetchResult> {
    let timeout = Duration::from_millis(timeout_ms.unwrap_or(15_000));
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(anyhow!(
            "models URL is empty — configure '获取模型 URL' in the provider settings"
        ));
    }

    let client = probe_http_client(timeout).map_err(|e| anyhow!("Failed to create HTTP client: {e}"))?;

    let provider_body = fetch_json_with_auth(&client, trimmed, api_key).await?;
    let provider_catalog = providers::catalog_from_provider_models(&provider_body);
    if provider_catalog.is_empty() {
        return Err(anyhow!("invalid response: no model IDs found in response"));
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

/// Query the remaining balance/quota for a provider preset.
///
/// Looks up the preset's `balance_endpoint` from the preset registry using
/// `preset_id`, sends GET with the API Key as Bearer token, and returns the
/// raw JSON response (the frontend parses based on preset_id/balance_parser).
pub async fn query_provider_balance(preset_id: &str, api_key: &str) -> Result<serde_json::Value> {
    let presets = providers::get_all_presets_flat();
    let preset = presets
        .iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| anyhow!("Unknown preset_id: '{preset_id}'"))?;

    let balance_endpoint = preset
        .balance_endpoint
        .as_ref()
        .ok_or_else(|| anyhow!("Preset '{preset_id}' does not support balance queries"))?;

    // Some balance endpoints may be relative to the base_url, but our presets
    // use absolute URLs. Use the endpoint as-is.
    let url = balance_endpoint.clone();

    let client = probe_http_client(Duration::from_secs(10))
        .map_err(|e| anyhow!("Failed to create HTTP client: {e}"))?;

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                anyhow!("查询超时")
            } else if e.is_connect() {
                anyhow!("网络错误: {e}")
            } else {
                anyhow!("查询失败: {e}")
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("HTTP {}: 余额查询失败", status.as_u16()));
    }

    response
        .json()
        .await
        .map_err(|e| anyhow!("解析响应失败: {e}"))
}

// ---------------------------------------------------------------------------
// Shared JSON fetch helpers
// ---------------------------------------------------------------------------

async fn fetch_json_with_auth(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
) -> Result<serde_json::Value> {
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(request_error)?;

    parse_json_response(response).await
}

async fn fetch_json_public(client: &reqwest::Client, url: &str) -> Result<serde_json::Value> {
    let response = client.get(url).send().await.map_err(request_error)?;
    parse_json_response(response).await
}

async fn parse_json_response(response: reqwest::Response) -> Result<serde_json::Value> {
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("HTTP {}", status.as_u16()));
    }

    response
        .json()
        .await
        .map_err(|e| anyhow!("invalid response: {e}"))
}

fn request_error(e: reqwest::Error) -> anyhow::Error {
    if e.is_timeout() {
        anyhow!("请求超时")
    } else if e.is_connect() {
        anyhow!("网络错误: {e}")
    } else {
        anyhow!("请求失败: {e}")
    }
}

//! Provider health check — real API request latency measurement.
//!
//! Measures end-to-end latency of a minimal chat completion request,
//! which reflects actual model inference + network path quality.
//!
//! Storage: `~/.skillstar/state/provider_health.json`
//! Log:    `~/.skillstar/logs/provider-health-YYYY-MM-DD.jsonl` (NDJSON, daily prune)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::providers::ProviderEntry;
use skillstar_infra::{daily_log, paths};

/// Health status of a provider endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// Endpoint reachable and responding quickly.
    Healthy,
    /// Endpoint reachable but slow (> 500 ms).
    Degraded,
    /// Endpoint unreachable or returned error.
    Unreachable,
    /// Not yet checked.
    Unknown,
}

impl HealthStatus {
    pub fn from_latency(latency_ms: Option<u64>, status: Option<u16>) -> Self {
        match (latency_ms, status) {
            // Healthy: reachable within 3s (real API requests are slower than /models ping)
            (Some(ms), Some(s)) if ms < 3000 && s < 500 => HealthStatus::Healthy,
            // Degraded: slow (> 3s TTFB) or server error
            (Some(ms), _) if ms >= 3000 => HealthStatus::Degraded,
            (None, _) => HealthStatus::Unreachable,
            _ => HealthStatus::Unknown,
        }
    }
}

/// Result of a single provider health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    /// Unique provider id (e.g. "openrouter_free", "claude_official").
    pub provider_id: String,
    /// App this provider belongs to (claude/codex/opencode/gemini).
    pub app_id: String,
    /// The URL that was checked (resolved from provider config).
    pub url: String,
    /// Latency in milliseconds. None if unreachable.
    pub latency_ms: Option<u64>,
    /// HTTP status code if available.
    pub status: Option<u16>,
    /// Derived health status.
    pub health_status: HealthStatus,
    /// Unix timestamp (seconds) of when this was checked.
    pub checked_at: i64,
    /// Error message if the request failed.
    pub error: Option<String>,
}

impl ProviderHealth {
    pub fn unknown(provider_id: &str, app_id: &str, url: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            app_id: app_id.to_string(),
            url: url.to_string(),
            latency_ms: None,
            status: None,
            health_status: HealthStatus::Unknown,
            checked_at: chrono::Utc::now().timestamp(),
            error: None,
        }
    }

    pub fn from_result(
        provider_id: &str,
        app_id: &str,
        url: &str,
        latency_ms: Option<u64>,
        status: Option<u16>,
        error: Option<String>,
    ) -> Self {
        let health_status = if error.is_some() {
            HealthStatus::Unreachable
        } else {
            HealthStatus::from_latency(latency_ms, status)
        };
        Self {
            provider_id: provider_id.to_string(),
            app_id: app_id.to_string(),
            url: url.to_string(),
            latency_ms,
            status,
            health_status,
            checked_at: chrono::Utc::now().timestamp(),
            error,
        }
    }
}

/// All health results keyed by `app_id/provider_id`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthStore {
    pub results: HashMap<String, ProviderHealth>,
}

fn store_path() -> PathBuf {
    paths::state_dir().join("provider_states.json")
}

fn composite_key(app_id: &str, provider_id: &str) -> String {
    format!("{}/{}", app_id, provider_id)
}

/// Load health store from disk (delegated to provider_states).
pub fn read_health_store() -> Result<HealthStore> {
    let states = crate::provider_states::load()?;
    Ok(states.health)
}

/// Persist health store to disk (delegated to provider_states).
fn write_health_store(store: &HealthStore) -> Result<()> {
    let mut states = crate::provider_states::load().unwrap_or_default();
    states.health = store.clone();
    crate::provider_states::save(&states)
}

/// Get cached health for a single provider.
pub fn get_cached_health(app_id: &str, provider_id: &str) -> Option<ProviderHealth> {
    read_health_store()
        .ok()?
        .results
        .get(&composite_key(app_id, provider_id))
        .cloned()
}

/// Get all cached health results for an app.
pub fn get_cached_health_for_app(app_id: &str) -> Vec<ProviderHealth> {
    let store = match read_health_store() {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    store
        .results
        .values()
        .filter(|h| h.app_id == app_id)
        .cloned()
        .collect()
}

/// Resolve the base URL for a provider (used for health check target).
pub fn resolve_provider_url(provider: &ProviderEntry, app_id: &str) -> Option<String> {
    let cfg = &provider.settings_config;
    match app_id {
        "claude" => {
            let env = cfg.get("env")?.as_object()?;
            env.get("ANTHROPIC_BASE_URL")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_end_matches('/').to_string())
        }
        "codex" => {
            let config_str = cfg.get("config")?.as_str()?;
            config_str
                .lines()
                .find(|l| l.starts_with("base_url = "))
                .and_then(|l| l.split('"').nth(1))
                .map(|s| s.trim_end_matches('/').to_string())
        }
        "opencode" => provider
            .meta
            .as_ref()?
            .get("baseURL")?
            .as_str()
            .map(|s| s.trim_end_matches('/').to_string()),
        "gemini" => {
            let env = cfg.get("env")?.as_object()?;
            env.get("GOOGLE_GEMINI_BASE_URL")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_end_matches('/').to_string())
        }
        _ => None,
    }
}

/// Validate the provider's base URL format synchronously (no network I/O).
/// Returns `Ok(())` on success, or `Err(reason)` on failure.
pub fn validate_provider_url_format(provider: &ProviderEntry, app_id: &str) -> Result<(), String> {
    let url = resolve_provider_url(provider, app_id)
        .ok_or_else(|| "No base URL configured for this provider".to_string())?;

    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("URL must start with https:// or http://".to_string());
    }
    if url.starts_with("http://")
        && !url.starts_with("http://localhost")
        && !url.starts_with("http://127.0.0.1")
    {
        return Err("HTTP URLs are only allowed for localhost".to_string());
    }
    // Basic character safety check
    if url.contains(' ') || url.contains('\0') || url.contains('\n') {
        return Err("URL contains invalid characters".to_string());
    }
    Ok(())
}

/// Extract API key from a provider's settings config.
fn extract_api_key(provider: &ProviderEntry, app_id: &str) -> Option<String> {
    let cfg = &provider.settings_config;
    match app_id {
        "claude" => {
            let env = cfg.get("env")?.as_object()?;
            env.get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| env.get("ANTHROPIC_API_KEY"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        }
        "codex" => {
            let auth = cfg.get("auth")?.as_object()?;
            auth.get("OPENAI_API_KEY")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        }
        "opencode" => Some(provider.meta.as_ref()?.get("apiKey")?.as_str()?.to_string()),
        "gemini" => {
            let env = cfg.get("env")?.as_object()?;
            env.get("GEMINI_API_KEY")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        }
        _ => None,
    }
}

/// Resolve the chat-completion endpoint URL for health probing.
fn resolve_chat_endpoint(provider: &ProviderEntry, app_id: &str) -> Option<String> {
    let cfg = &provider.settings_config;
    match app_id {
        "claude" => {
            let env = cfg.get("env")?.as_object()?;
            let base = env
                .get("ANTHROPIC_BASE_URL")?
                .as_str()?
                .trim_end_matches('/');
            if base.is_empty() {
                return None;
            }
            Some(format!("{}/v1/messages", base))
        }
        "codex" | "opencode" => {
            let base = provider.meta.as_ref()?.get("baseURL")?.as_str()?;
            let base = base.trim_end_matches('/');
            if base.is_empty() {
                return None;
            }
            Some(format!("{}/v1/chat/completions", base))
        }
        "gemini" => {
            let env = cfg.get("env")?.as_object()?;
            let base = env
                .get("GOOGLE_GEMINI_BASE_URL")?
                .as_str()?
                .trim_end_matches('/');
            if base.is_empty() {
                return None;
            }
            Some(format!("{}/v1/models", base))
        }
        _ => None,
    }
}

/// Known fast/lightweight models per provider for health probing.
fn probe_model(app_id: &str) -> &'static str {
    match app_id {
        "claude" => "claude-3-haiku-4-20250514",
        "codex" | "opencode" => "gpt-4o-mini",
        "gemini" => "gemini-2.0-flash",
        _ => "gpt-4o-mini",
    }
}

/// Check health of a single provider by sending a minimal real API request.
/// Measures end-to-end latency of a max_tokens=1 completion — reflects actual
/// model inference capability + network path quality.
pub async fn check_provider_health(provider: &ProviderEntry, app_id: &str) -> ProviderHealth {
    let url = match resolve_chat_endpoint(provider, app_id) {
        Some(u) => u,
        None => {
            return ProviderHealth::unknown(&provider.id, app_id, "");
        }
    };

    let api_key = match extract_api_key(provider, app_id) {
        Some(k) => k,
        None => {
            return ProviderHealth::from_result(
                &provider.id,
                app_id,
                &url,
                None,
                None,
                Some("No API key configured".to_string()),
            );
        }
    };

    // Validate URL scheme before probing
    if !url.starts_with("https://")
        && !url.starts_with("http://localhost")
        && !url.starts_with("http://127.0.0.1")
    {
        return ProviderHealth::from_result(
            &provider.id,
            app_id,
            &url,
            None,
            None,
            Some("Only HTTPS (or localhost) URLs are allowed".to_string()),
        );
    }

    let model = probe_model(app_id);
    let timeout = Duration::from_secs(30);

    let client = match reqwest::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return ProviderHealth::from_result(
                &provider.id,
                app_id,
                &url,
                None,
                None,
                Some(format!("HTTP client error: {e}")),
            );
        }
    };

    let start = Instant::now();

    // Build the minimal request body per provider type
    let (body_json, headers) = match app_id {
        "claude" => {
            let body = serde_json::json!({
                "model": model,
                "max_tokens": 1,
                "messages": [{ "role": "user", "content": "hi" }]
            });
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {api_key}").parse().unwrap(),
            );
            headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
            headers.insert(
                reqwest::header::CONTENT_TYPE,
                "application/json".parse().unwrap(),
            );
            (body, headers)
        }
        "gemini" => {
            let body = serde_json::json!({
                "contents": [{ "parts": [{ "text": "hi" }] }],
                "generationConfig": { "maxOutputTokens": 1 }
            });
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("x-goog-api-key", api_key.parse().unwrap());
            headers.insert(
                reqwest::header::CONTENT_TYPE,
                "application/json".parse().unwrap(),
            );
            // For gemini the model is in the URL path
            let url_with_model = format!("{}/{model}:generateContent", url);
            return probe_url(
                &client,
                &url_with_model,
                headers,
                body,
                provider,
                app_id,
                start,
            )
            .await;
        }
        _ => {
            // OpenAI-compatible (codex, opencode)
            let body = serde_json::json!({
                "model": model,
                "max_tokens": 1,
                "messages": [{ "role": "user", "content": "hi" }]
            });
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {api_key}").parse().unwrap(),
            );
            headers.insert(
                reqwest::header::CONTENT_TYPE,
                "application/json".parse().unwrap(),
            );
            (body, headers)
        }
    };

    probe_url(&client, &url, headers, body_json, provider, app_id, start).await
}

/// Helper: POST a JSON body to a URL and measure latency.
async fn probe_url(
    client: &reqwest::Client,
    url: &str,
    headers: reqwest::header::HeaderMap,
    body: serde_json::Value,
    provider: &ProviderEntry,
    app_id: &str,
    start: Instant,
) -> ProviderHealth {
    match client.post(url).headers(headers).json(&body).send().await {
        Ok(resp) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let status = resp.status().as_u16();
            let is_ok = resp.status().is_success() || status == 401 || status == 400;
            if is_ok {
                ProviderHealth::from_result(
                    &provider.id,
                    app_id,
                    url,
                    Some(elapsed_ms),
                    Some(status),
                    None,
                )
            } else {
                ProviderHealth::from_result(
                    &provider.id,
                    app_id,
                    url,
                    Some(elapsed_ms),
                    Some(status),
                    Some(format!("HTTP {status}")),
                )
            }
        }
        Err(e) => {
            ProviderHealth::from_result(&provider.id, app_id, url, None, None, Some(e.to_string()))
        }
    }
}

/// Check health of all providers for a given app, in parallel.
pub async fn check_all_providers_health(
    app_id: &str,
    providers: &[ProviderEntry],
) -> Vec<ProviderHealth> {
    let mut handles = Vec::with_capacity(providers.len());

    for provider in providers {
        let provider = provider.clone();
        let app_id = app_id.to_string();
        handles.push(tokio::spawn(async move {
            check_provider_health(&provider, &app_id).await
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(health) => results.push(health),
            Err(e) => {
                tracing::warn!("Health check task join error: {e}");
            }
        }
    }

    // Persist results
    if !results.is_empty() {
        let mut store = match read_health_store() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to read health store for write: {e}");
                HealthStore::default()
            }
        };

        for health in &results {
            store.results.insert(
                composite_key(&health.app_id, &health.provider_id),
                health.clone(),
            );
        }

        if let Err(e) = write_health_store(&store) {
            tracing::error!("Failed to persist health store: {e}");
        }

        // Emit NDJSON log line
        for health in &results {
            let line = serde_json::to_string(health).unwrap_or_default();
            daily_log::append_ndjson_line("provider-health", &line);
        }
    }

    results
}

//! Usage quota fetching and caching for model providers.
//!
//! Fetches quota/usage information from provider APIs where available.
//! Results are cached in `~/.skillstar/state/provider_quota.json`.
//! Log: `~/.skillstar/logs/provider-quota-YYYY-MM-DD.jsonl` (NDJSON)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::providers::ProviderEntry;
use skillstar_infra::{daily_log, paths};

/// Quota information for a provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderQuota {
    pub provider_id: String,
    pub app_id: String,
    /// Human-readable usage percentage (0-100).
    pub usage_percent: Option<i32>,
    /// Human-readable remaining quota (free-form string, e.g. "$1.50 / $5.00").
    pub remaining: Option<String>,
    /// Reset time in ISO 8601 or natural language.
    pub reset_time: Option<String>,
    /// Plan name if available (e.g. "Pro", "Enterprise").
    pub plan_name: Option<String>,
    /// Unix timestamp when this was fetched.
    pub fetched_at: i64,
    /// Error message if quota fetch failed.
    pub error: Option<String>,
}

pub(crate) fn composite_key(app_id: &str, provider_id: &str) -> String {
    format!("{}/{}", app_id, provider_id)
}

impl ProviderQuota {
    pub fn error(provider_id: &str, app_id: &str, error: String) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            app_id: app_id.to_string(),
            usage_percent: None,
            remaining: None,
            reset_time: None,
            plan_name: None,
            fetched_at: chrono::Utc::now().timestamp(),
            error: Some(error),
        }
    }
}

/// Quota store: keyed by "app_id/provider_id".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuotaStore {
    pub quotas: HashMap<String, ProviderQuota>,
}

fn store_path() -> PathBuf {
    paths::state_dir().join("provider_quota.json")
}

/// Load quota store from disk (delegated to provider_states).
pub fn read_quota_store() -> Result<QuotaStore> {
    let states = crate::provider_states::load()?;
    Ok(states.quotas)
}

/// Persist quota store (delegated to provider_states).
fn write_quota_store(store: &QuotaStore) -> Result<()> {
    let mut states = crate::provider_states::load().unwrap_or_default();
    states.quotas = store.clone();
    crate::provider_states::save(&states)
}

/// Get cached quota for a single provider.
pub fn get_cached_quota(app_id: &str, provider_id: &str) -> Option<ProviderQuota> {
    read_quota_store()
        .ok()?
        .quotas
        .get(&composite_key(app_id, provider_id))
        .cloned()
}

/// Get all cached quotas for an app.
pub fn get_cached_quotas_for_app(app_id: &str) -> Vec<ProviderQuota> {
    let store = match read_quota_store() {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    store
        .quotas
        .values()
        .filter(|q| q.app_id == app_id)
        .cloned()
        .collect()
}

/// Fetch quota from a provider endpoint.
/// Returns a ProviderQuota with whatever info was extractable.
async fn fetch_quota_from_endpoint(
    provider: &ProviderEntry,
    app_id: &str,
    api_key: Option<&str>,
) -> ProviderQuota {
    let base_url = crate::health::resolve_provider_url(provider, app_id).unwrap_or_default();

    if base_url.is_empty() {
        return ProviderQuota::error(
            &provider.id,
            app_id,
            "No base URL configured for this provider".to_string(),
        );
    }

    // For now, try a generic OpenAI-compatible `/usage` or `/quota` endpoint.
    // Many providers implement this pattern.
    let usage_urls = [
        base_url.replace("/v1/models", "/v1/usage"),
        base_url.replace("/models", "/usage"),
        format!("{}/usage", base_url.trim_end_matches('/')),
    ];

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return ProviderQuota::error(&provider.id, app_id, format!("HTTP client error: {e}"));
        }
    };

    for usage_url in &usage_urls {
        let mut req = client.get(usage_url);
        if let Some(key) = api_key {
            if !key.is_empty() {
                req = req.bearer_auth(key);
            }
        }

        match req.send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<serde_json::Value>().await {
                        Ok(data) => {
                            // Try to extract quota info from common response shapes
                            let usage_percent = data
                                .get("usage_percent")
                                .or_else(|| data.get("percentage"))
                                .or_else(|| data.get("quota_used"))
                                .and_then(|v| v.as_f64())
                                .map(|p| p as i32);

                            let remaining = data
                                .get("remaining")
                                .or_else(|| data.get("quota_remaining"))
                                .or_else(|| data.get("available_credits"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            let reset_time = data
                                .get("reset_time")
                                .or_else(|| data.get("next_reset"))
                                .or_else(|| data.get("resetAt"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            let plan_name = data
                                .get("plan_name")
                                .or_else(|| data.get("plan"))
                                .or_else(|| data.get("tier"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            return ProviderQuota {
                                provider_id: provider.id.clone(),
                                app_id: app_id.to_string(),
                                usage_percent,
                                remaining,
                                reset_time,
                                plan_name,
                                fetched_at: chrono::Utc::now().timestamp(),
                                error: None,
                            };
                        }
                        Err(_) => {
                            // Not JSON, but got a 200 — treat as success with no parseable quota
                            return ProviderQuota {
                                provider_id: provider.id.clone(),
                                app_id: app_id.to_string(),
                                usage_percent: None,
                                remaining: None,
                                reset_time: None,
                                plan_name: None,
                                fetched_at: chrono::Utc::now().timestamp(),
                                error: None,
                            };
                        }
                    }
                }
            }
            Err(_) => continue,
        }
    }

    // No usable usage endpoint found
    ProviderQuota {
        provider_id: provider.id.clone(),
        app_id: app_id.to_string(),
        usage_percent: None,
        remaining: None,
        reset_time: None,
        plan_name: None,
        fetched_at: chrono::Utc::now().timestamp(),
        error: None,
    }
}

/// Extract API key from a provider's settings_config for a given app.
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

/// Fetch quota for a single provider.
pub async fn fetch_provider_quota(provider: &ProviderEntry, app_id: &str) -> ProviderQuota {
    let api_key = extract_api_key(provider, app_id);
    let quota = fetch_quota_from_endpoint(provider, app_id, api_key.as_deref()).await;

    // Persist result
    let mut store = match read_quota_store() {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to read quota store: {e}");
            QuotaStore::default()
        }
    };
    store
        .quotas
        .insert(composite_key(app_id, &provider.id), quota.clone());

    if let Err(e) = write_quota_store(&store) {
        tracing::error!("Failed to persist quota store: {e}");
    }

    // Log NDJSON
    if let Ok(line) = serde_json::to_string(&quota) {
        daily_log::append_ndjson_line("provider-quota", &line);
    }

    quota
}

/// Fetch quota for all providers of an app in parallel.
pub async fn fetch_all_quotas(app_id: &str, providers: &[ProviderEntry]) -> Vec<ProviderQuota> {
    let mut handles = Vec::with_capacity(providers.len());

    for provider in providers {
        let provider = provider.clone();
        let app_id = app_id.to_string();
        handles.push(tokio::spawn(async move {
            fetch_provider_quota(&provider, &app_id).await
        }));
    }

    let mut quotas = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(q) => quotas.push(q),
            Err(e) => tracing::warn!("Quota fetch task join error: {e}"),
        }
    }

    quotas
}

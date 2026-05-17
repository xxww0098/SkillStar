//! Tauri commands for Models mode — Provider CRUD, activation, and tool sync.
//!
//! All write operations are serialized through a tokio Mutex to prevent
//! concurrent corruption of `model_providers.json`.
//!
//! ## Architecture
//!
//! This module contains two generations of commands:
//!
//! - **Legacy (per-app)**: `get_providers_store`, `create_provider`, etc.
//!   These operate on the v1 per-app `ProvidersStore` format and are retained
//!   for backward compatibility during the transition period.
//!
//! - **Flat store (v2)**: `get_providers_flat`, `create_provider_flat`, etc.
//!   These operate on the new flat `FlatProvidersStore` format with a unified
//!   provider list and `tool_activations` map.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;

use skillstar_ai::ai_provider;
use skillstar_models::latency::{self, LatencyResult};
use skillstar_models::provider_ref::AiProviderRef;
use skillstar_models::providers::{
    self, AppProviders, FlatProvidersStore, ProviderEntry, ProviderEntryFlat, ProviderPatch,
    ProviderPatchFlat, ProviderPreset, ProviderSettings, ProvidersStore, ToolActivation,
};
use skillstar_models::tool_sync::{self, ToolConfigTarget, ToolSyncResult, ToolSyncResultFlat};

// ---------------------------------------------------------------------------
// State: write-serialization mutex
// ---------------------------------------------------------------------------

/// Tokio Mutex used to serialize all writes to `model_providers.json`.
/// Managed as Tauri state so all commands share the same lock.
pub struct ProvidersWriteLock(pub Mutex<()>);

impl ProvidersWriteLock {
    pub fn new() -> Self {
        Self(Mutex::new(()))
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Result of switching the active provider (includes optional tool sync results).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchResult {
    pub app_id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub tools_synced: Vec<ToolSyncResult>,
}

/// Response for `get_providers_flat` — returns the full flat store contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatProvidersResponse {
    pub version: u32,
    pub providers: Vec<ProviderEntryFlat>,
    pub tool_activations: std::collections::HashMap<String, Option<ToolActivation>>,
}

// ---------------------------------------------------------------------------
// Read commands (no lock needed)
// ---------------------------------------------------------------------------

/// Returns the full ProvidersStore (all apps).
#[tauri::command]
pub async fn get_providers_store() -> Result<ProvidersStore, String> {
    providers::read_store().map_err(|e| e.to_string())
}

/// Returns providers and current active provider for a single AppId.
#[tauri::command]
pub async fn get_app_providers(app_id: String) -> Result<AppProviders, String> {
    let store = providers::read_store().map_err(|e| e.to_string())?;
    let app = match app_id.as_str() {
        "claude" => store.claude,
        "codex" => store.codex,
        "opencode" => store.opencode,
        "gemini" => store.gemini,
        _ => return Err(format!("Unknown app_id: {}", app_id)),
    };
    Ok(app)
}

/// Returns the list of built-in provider presets.
#[tauri::command]
pub async fn get_provider_presets() -> Result<Vec<ProviderPreset>, String> {
    Ok(providers::get_provider_presets())
}

// ---------------------------------------------------------------------------
// Write commands (lock required)
// ---------------------------------------------------------------------------

/// Create a new provider entry for the given app_id.
///
/// Validates name, URL, model count, and ID uniqueness.
/// Auto-activates if this is the first provider for the app.
#[tauri::command]
pub async fn create_provider(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    entry: ProviderEntry,
) -> Result<ProviderEntry, String> {
    let _guard = lock.0.lock().await;
    providers::create_provider(&app_id, entry).map_err(|e| e.to_string())
}

/// Create a provider from a built-in preset.
///
/// Only requires the API key; all other fields are pre-filled from the preset.
#[tauri::command]
pub async fn create_provider_from_preset(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    preset_id: String,
    api_key: String,
) -> Result<ProviderEntry, String> {
    let _guard = lock.0.lock().await;
    providers::create_from_preset(&app_id, &preset_id, &api_key).map_err(|e| e.to_string())
}

/// Update an existing provider with a partial patch.
#[tauri::command]
pub async fn update_provider(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    id: String,
    patch: ProviderPatch,
) -> Result<ProviderEntry, String> {
    let _guard = lock.0.lock().await;
    providers::update_provider(&app_id, &id, patch).map_err(|e| e.to_string())
}

/// Delete a provider by ID.
///
/// If the deleted provider is the currently active one, `current` is set to null.
#[tauri::command]
pub async fn delete_provider(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    id: String,
) -> Result<(), String> {
    let _guard = lock.0.lock().await;
    providers::delete_provider(&app_id, &id).map_err(|e| e.to_string())
}

/// Switch the active provider for an app.
///
/// Updates `model_providers.json`, updates `ai.json` provider_ref,
/// and optionally syncs to external tool config files.
#[tauri::command]
pub async fn switch_active_provider(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    provider_id: String,
    sync_tools: Option<Vec<String>>,
) -> Result<SwitchResult, String> {
    let _guard = lock.0.lock().await;

    // Step 1: Update providers store
    providers::switch_active_provider(&app_id, &provider_id).map_err(|e| e.to_string())?;

    // Read back the provider name for the result
    let store = providers::read_store().map_err(|e| e.to_string())?;
    let provider = match app_id.as_str() {
        "claude" => store.claude.providers.get(&provider_id),
        "codex" => store.codex.providers.get(&provider_id),
        "opencode" => store.opencode.providers.get(&provider_id),
        "gemini" => store.gemini.providers.get(&provider_id),
        _ => None,
    }
    .ok_or_else(|| format!("Provider '{}' not found after switch", provider_id))?
    .clone();

    // Step 2: Update ai.json provider_ref
    let mut ai_config = ai_provider::load_config();
    ai_config.provider_ref = Some(AiProviderRef {
        app_id: app_id.clone(),
        provider_id: provider_id.clone(),
    });
    ai_provider::save_config(&ai_config).map_err(|e| e.to_string())?;

    // Step 3: Optionally sync to external tools
    let tools_synced = if let Some(tool_ids) = sync_tools {
        if !tool_ids.is_empty() {
            tool_sync::sync_provider_to_all_tools(&provider, &tool_ids)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    Ok(SwitchResult {
        app_id,
        provider_id,
        provider_name: provider.name,
        tools_synced,
    })
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

// ---------------------------------------------------------------------------
// Tool config commands
// ---------------------------------------------------------------------------

/// Returns the list of supported external tool config targets with their paths and existence status.
#[tauri::command]
pub async fn get_tool_config_targets() -> Result<Vec<ToolConfigTarget>, String> {
    tool_sync::get_tool_config_targets().map_err(|e| e.to_string())
}

/// Sync a provider's configuration to a single external tool.
///
/// Creates a backup of the existing config file before writing.
#[tauri::command]
pub async fn sync_provider_to_tool(
    app_id: String,
    provider_id: String,
    tool_id: String,
) -> Result<ToolSyncResult, String> {
    let store = providers::read_store().map_err(|e| e.to_string())?;

    let provider = match app_id.as_str() {
        "claude" => store.claude.providers.get(&provider_id),
        "codex" => store.codex.providers.get(&provider_id),
        "opencode" => store.opencode.providers.get(&provider_id),
        "gemini" => store.gemini.providers.get(&provider_id),
        _ => return Err(format!("Unknown app_id: {}", app_id)),
    }
    .ok_or_else(|| format!("Provider '{}' not found in app '{}'", provider_id, app_id))?;

    Ok(tool_sync::sync_provider_to_tool(provider, &tool_id))
}

/// Sync a provider's configuration to all supported external tools.
///
/// Syncs to each tool independently — a failure in one tool does not prevent others.
#[tauri::command]
pub async fn sync_provider_to_all_tools(
    app_id: String,
    provider_id: String,
    tool_ids: Vec<String>,
) -> Result<Vec<ToolSyncResult>, String> {
    let store = providers::read_store().map_err(|e| e.to_string())?;

    let provider = match app_id.as_str() {
        "claude" => store.claude.providers.get(&provider_id),
        "codex" => store.codex.providers.get(&provider_id),
        "opencode" => store.opencode.providers.get(&provider_id),
        "gemini" => store.gemini.providers.get(&provider_id),
        _ => return Err(format!("Unknown app_id: {}", app_id)),
    }
    .ok_or_else(|| format!("Provider '{}' not found in app '{}'", provider_id, app_id))?;

    Ok(tool_sync::sync_provider_to_all_tools(provider, &tool_ids))
}

// ===========================================================================
// Flat Store Commands (v2 architecture)
// ===========================================================================
//
// These commands operate on the new flat `FlatProvidersStore` format.
// They coexist with the legacy per-app commands above during the transition.

// ---------------------------------------------------------------------------
// Flat store: Read commands (no lock needed)
// ---------------------------------------------------------------------------

/// Returns the full flat provider store (version + providers + tool_activations).
///
/// Performs v1→v2 migration on first access if needed.
#[tauri::command]
pub async fn get_providers_flat() -> Result<FlatProvidersResponse, String> {
    let path = providers::flat_store_path();
    let store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;
    Ok(FlatProvidersResponse {
        version: store.version,
        providers: store.providers,
        tool_activations: store.tool_activations,
    })
}

/// Returns the current tool activations map.
///
/// This is a lightweight read that only returns which provider + model each
/// tool is currently using, without the full provider list.
#[tauri::command]
pub async fn get_tool_activations() -> Result<std::collections::HashMap<String, Option<ToolActivation>>, String> {
    let path = providers::flat_store_path();
    let store = providers::read_flat_store(&path).map_err(|e| e.to_string())?;
    Ok(store.tool_activations)
}

// ---------------------------------------------------------------------------
// Flat store: Write commands (lock required)
// ---------------------------------------------------------------------------

/// Create a new provider in the flat store.
///
/// Validates the entry (name non-empty, URL format), generates a UUID,
/// sets `created_at` and `sort_index`, then persists atomically.
#[tauri::command]
pub async fn create_provider_flat(
    lock: State<'_, ProvidersWriteLock>,
    entry: ProviderEntryFlat,
) -> Result<ProviderEntryFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    let created = providers::create_provider_flat(&mut store, entry).map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    Ok(created)
}

/// Update an existing provider with a partial patch.
///
/// Only non-None fields in the patch are applied. If the provider is currently
/// active for any tools, those tools are automatically re-synced with the
/// updated credentials (preserving each tool's individually selected model).
#[tauri::command]
pub async fn update_provider_flat(
    lock: State<'_, ProvidersWriteLock>,
    id: String,
    patch: ProviderPatchFlat,
) -> Result<ProviderEntryFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    let updated =
        providers::update_provider_flat(&mut store, &id, patch).map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    // Re-sync active tools that use this provider (per Requirement 3.10)
    let _sync_results = tool_sync::resync_active_tools(&store, &id);

    Ok(updated)
}

/// Delete a provider from the flat store.
///
/// Also clears any `tool_activations` entries that reference this provider.
/// The caller should handle tool config file restoration (deactivation) before
/// calling this command if needed.
#[tauri::command]
pub async fn delete_provider_flat(
    lock: State<'_, ProvidersWriteLock>,
    id: String,
) -> Result<(), String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    providers::delete_provider_flat(&mut store, &id).map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    Ok(())
}

/// Reorder providers by assigning new `sort_index` values based on the given ID list.
///
/// Each ID in `ordered_ids` gets `sort_index = position` (0-based).
/// Providers not in the list keep their existing `sort_index`.
#[tauri::command]
pub async fn reorder_providers(
    lock: State<'_, ProvidersWriteLock>,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    providers::reorder_providers(&mut store, &ordered_ids).map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    Ok(())
}

/// Activate a provider for a specific Agent tool.
///
/// Updates the `tool_activations` map and syncs the provider's credentials
/// to the tool's config file. Only one provider can be active per tool —
/// activating a new provider replaces any previous activation.
///
/// If `model` is None, the provider's `default_model` is used.
#[tauri::command]
pub async fn activate_tool(
    lock: State<'_, ProvidersWriteLock>,
    provider_id: String,
    tool_id: String,
    model: Option<String>,
) -> Result<ToolSyncResultFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    // 1. Update the tool_activations map
    let activation = providers::activate_tool(
        &mut store,
        &provider_id,
        &tool_id,
        model.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    // 2. Persist the updated store
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    // 3. Sync the provider's credentials to the tool's config file
    let provider = store
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .ok_or_else(|| format!("Provider '{}' not found after activation", provider_id))?;

    let sync_result = match tool_id.as_str() {
        "claude-code" => tool_sync::sync_to_claude_code(provider, &activation.model)
            .map_err(|e| e.to_string())?,
        "codex" => tool_sync::sync_to_codex(provider, &activation.model)
            .map_err(|e| e.to_string())?,
        _ => ToolSyncResultFlat {
            tool_id: tool_id.clone(),
            success: false,
            config_path: None,
            error: Some(format!("Unknown tool_id '{}'. Only 'claude-code' and 'codex' are supported.", tool_id)),
            backup_path: None,
        },
    };

    Ok(sync_result)
}

// ---------------------------------------------------------------------------
// Connection test command (minimal chat completion request)
// ---------------------------------------------------------------------------

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

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let base = base_url.trim_end_matches('/');

    let start = Instant::now();

    let response = if model.trim().is_empty() {
        // Reachability probe: GET /models with both auth styles. Works for
        // OpenAI-compatible and Anthropic-compatible endpoints; servers ignore
        // the header they don't understand.
        let url = format!("{base}/models");
        client
            .get(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
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

/// Deactivate a tool by removing its activation entry and restoring its config.
///
/// Clears the tool's entry in `tool_activations` and calls the appropriate
/// unsync function to remove managed fields from the tool's config file.
#[tauri::command]
pub async fn deactivate_tool(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
) -> Result<(), String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    // 1. Remove the activation from the map
    providers::deactivate_tool(&mut store, &tool_id).map_err(|e| e.to_string())?;

    // 2. Persist the updated store
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    // 3. Unsync the tool's config file (remove managed fields)
    match tool_id.as_str() {
        "claude-code" => {
            tool_sync::unsync_claude_code().map_err(|e| e.to_string())?;
        }
        "codex" => {
            tool_sync::unsync_codex().map_err(|e| e.to_string())?;
        }
        _ => {
            // Unknown tool — nothing to unsync, but not an error
        }
    }

    Ok(())
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
        return Err("models URL is empty — configure '获取模型 URL' in the provider settings".to_string());
    }
    let url = trimmed.to_string();

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
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

// ---------------------------------------------------------------------------
// Tool Installation Detection
// ---------------------------------------------------------------------------

/// Detect whether an Agent tool (CLI) is installed on the system.
///
/// Checks:
/// 1. Whether the CLI binary exists in PATH (e.g., `claude` for claude-code, `codex` for codex)
/// 2. Whether the tool's config directory exists (e.g., `~/.claude` for claude-code, `~/.codex` for codex)
///
/// Returns a JSON object: `{ "installed": bool, "binary_found": bool, "config_dir_found": bool }`
///
/// A tool is considered "installed" if the binary is found in PATH.
/// The config_dir_found field provides additional context (a tool may be installed
/// but not yet configured, or config may exist from a previous installation).
#[tauri::command]
pub async fn detect_tool_installation(tool_id: String) -> Result<serde_json::Value, String> {
    let (binary_name, config_dir_name) = match tool_id.as_str() {
        "claude-code" => ("claude", ".claude"),
        "codex" => ("codex", ".codex"),
        _ => return Err(format!("Unknown tool_id: '{}'. Only 'claude-code' and 'codex' are supported.", tool_id)),
    };

    // Check if binary exists in PATH
    let binary_found = which::which(binary_name).is_ok();

    // Check if config directory exists
    let config_dir_found = dirs::home_dir()
        .map(|home| home.join(config_dir_name).is_dir())
        .unwrap_or(false);

    // A tool is considered installed if the binary is found in PATH
    let installed = binary_found;

    Ok(serde_json::json!({
        "installed": installed,
        "binary_found": binary_found,
        "config_dir_found": config_dir_found
    }))
}

// ---------------------------------------------------------------------------
// Environment Conflict Detection
// ---------------------------------------------------------------------------

/// Detect shell environment variable conflicts that may override tool config files.
///
/// Delegates to `tool_sync::detect_env_conflicts()` which checks for:
/// - Anthropic/Claude-related env vars (ANTHROPIC_API_KEY, ANTHROPIC_BASE_URL, etc.)
/// - OpenAI/Codex-related env vars (OPENAI_API_KEY, OPENAI_BASE_URL, etc.)
///
/// Returns a list of detected conflicts as serialized JSON values.
#[tauri::command]
pub async fn detect_env_conflicts() -> Result<Vec<serde_json::Value>, String> {
    let conflicts = tool_sync::detect_env_conflicts();
    let serialized: Vec<serde_json::Value> = conflicts
        .into_iter()
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();
    Ok(serialized)
}

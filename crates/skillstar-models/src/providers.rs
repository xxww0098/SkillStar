//! Provider store for AI provider resolution and CRUD operations.
//!
//! Reads/writes `~/.skillstar/config/model_providers.json` to manage provider configurations.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;
use url::Url;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single model mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelMapping {
    pub source_model: String,
    pub target_model: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Provider settings (nested inside ProviderEntry.settings_config).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderSettings {
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    pub models: Vec<ModelMapping>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
}

/// A single named provider entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub settings_config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Per-app provider collection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppProviders {
    pub providers: HashMap<String, ProviderEntry>,
    pub current: Option<String>,
}

/// Root structure stored in model_providers.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersStore {
    #[serde(default)]
    pub claude: AppProviders,
    #[serde(default)]
    pub codex: AppProviders,
    #[serde(default)]
    pub opencode: AppProviders,
    #[serde(default)]
    pub gemini: AppProviders,
}

/// A built-in provider preset template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreset {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key_url: String,
    pub icon_color: String,
    pub models: Vec<String>,
}

// ---------------------------------------------------------------------------
// Flat provider store types (v2 architecture)
// ---------------------------------------------------------------------------

/// Root structure for the flat provider store (`model_providers.json` v2).
///
/// Stores providers as a flat array with a separate `tool_activations` map
/// that records which provider + model each Agent tool is currently using.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatProvidersStore {
    pub version: u32,
    pub providers: Vec<ProviderEntryFlat>,
    pub tool_activations: HashMap<String, Option<ToolActivation>>,
}

impl Default for FlatProvidersStore {
    fn default() -> Self {
        Self {
            version: 2,
            providers: Vec::new(),
            tool_activations: HashMap::new(),
        }
    }
}

/// A single provider entry in the flat store.
///
/// Each provider has dual base URLs: one for OpenAI-compatible endpoints and
/// one for Anthropic-compatible endpoints. This allows a single provider config
/// to serve both Claude Code (Anthropic format) and Codex (OpenAI format).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderEntryFlat {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub base_url_openai: String,
    #[serde(default)]
    pub base_url_anthropic: String,
    /// Unique "fetch available models" endpoint for this provider.
    ///
    /// All agent configurations (Claude, Codex, …) call this same URL to
    /// populate their model pickers. Typically an OpenAI-compatible
    /// `.../v1/models` endpoint — the response is parsed as
    /// `{ "data": [{ "id": "<model>" }] }`.
    #[serde(default)]
    pub models_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub sort_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

/// Records which provider and model a specific Agent tool is currently using.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolActivation {
    pub provider_id: String,
    pub model: String,
}

/// Partial update patch for a flat provider entry.
///
/// All fields are optional — only non-None fields are applied during update.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProviderPatchFlat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url_openai: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url_anthropic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

/// Partial update patch for a provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_config: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn store_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".skillstar")
        .join("config")
        .join("model_providers.json")
}

/// Returns the default path for the flat provider store (`model_providers.json`).
///
/// This is the same file as the legacy store — the format is detected by the
/// presence of a `version` field.
pub fn flat_store_path() -> PathBuf {
    store_path()
}

// ---------------------------------------------------------------------------
// Flat store read/write (v2 architecture)
// ---------------------------------------------------------------------------

/// Read the flat provider store (v2) from a specific path.
///
/// Returns a default empty store if:
/// - The file doesn't exist
/// - The file cannot be read (permission error, etc.)
/// - The file contains invalid JSON
///
/// This ensures the application can always start with a valid state.
pub fn read_flat_store(path: &Path) -> Result<FlatProvidersStore> {
    if !path.exists() {
        return Ok(FlatProvidersStore::default());
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            warn!(
                "Failed to read flat store at {}: {e}. Returning default store.",
                path.display()
            );
            return Ok(FlatProvidersStore::default());
        }
    };
    // Strip BOM if present
    let text = text.trim_start_matches('\u{FEFF}');
    match serde_json::from_str::<FlatProvidersStore>(text) {
        Ok(store) => Ok(store),
        Err(e) => {
            warn!(
                "Malformed JSON in flat store {}: {e}. Returning default empty store.",
                path.display()
            );
            Ok(FlatProvidersStore::default())
        }
    }
}

/// Write the flat provider store (v2) to a specific path atomically.
///
/// Uses a temp file + rename strategy to prevent partial writes:
/// 1. Creates parent directories if they don't exist
/// 2. Serializes the store to pretty JSON
/// 3. Writes to a temporary file (`.json.tmp`) in the same directory
/// 4. Renames the temp file to the target path (atomic on most filesystems)
pub fn write_flat_store(store: &FlatProvidersStore, path: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Serialize to pretty JSON
    let json =
        serde_json::to_string_pretty(store).context("Failed to serialize FlatProvidersStore")?;

    // Write to a temp file in the same directory, then rename for atomicity
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json.as_bytes())
        .with_context(|| format!("Failed to write temp file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Migration: v1 → v2
// ---------------------------------------------------------------------------

/// Detect store version and migrate if needed.
///
/// - If the file does not exist, returns a default empty v2 store.
/// - If the file is already v2 (has `version: 2`), parses and returns it directly.
/// - Otherwise, parses as v1 `ProvidersStore`, converts to v2, backs up the original
///   file as `.json.bak`, writes the new v2 store, and returns it.
pub fn migrate_store_if_needed(path: &Path) -> Result<FlatProvidersStore> {
    // Edge case: file not found → return default
    if !path.exists() {
        return Ok(FlatProvidersStore::default());
    }

    // Read raw JSON
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read store file {}", path.display()))?;
    let raw = raw.trim_start_matches('\u{FEFF}');

    // Parse as generic JSON Value
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                "Failed to parse {} as JSON: {e}. Returning default store.",
                path.display()
            );
            return Ok(FlatProvidersStore::default());
        }
    };

    // Check if already v2
    if value.get("version").and_then(|v| v.as_u64()) == Some(2) {
        // Already v2 — parse directly
        let store: FlatProvidersStore = serde_json::from_value(value)
            .context("Failed to parse v2 FlatProvidersStore")?;
        return Ok(store);
    }

    // Parse as v1 ProvidersStore
    let old: ProvidersStore = serde_json::from_value(value)
        .context("Failed to parse v1 ProvidersStore during migration")?;

    // Convert v1 → v2
    let new_store = convert_v1_to_v2(&old);

    // Backup original file before overwriting
    let backup_path = path.with_extension("json.bak");
    if let Err(e) = std::fs::copy(path, &backup_path) {
        warn!(
            "Failed to create backup at {}: {e}. Proceeding with migration anyway.",
            backup_path.display()
        );
    }

    // Write the new v2 store
    write_flat_store(&new_store, path)?;

    Ok(new_store)
}

/// Convert a v1 `ProvidersStore` (per-app) to a v2 `FlatProvidersStore` (flat).
///
/// Deduplication strategy: providers with the same (base_url, api_key) pair are
/// merged into a single entry, combining their model lists.
fn convert_v1_to_v2(old: &ProvidersStore) -> FlatProvidersStore {
    use std::collections::HashSet;

    // Collect all providers from all apps with their app context
    struct CollectedProvider {
        entry: ProviderEntry,
        base_url: String,
        api_key: String,
        models: Vec<String>,
    }

    let apps: &[(&str, &AppProviders)] = &[
        ("claude", &old.claude),
        ("codex", &old.codex),
        ("opencode", &old.opencode),
        ("gemini", &old.gemini),
    ];

    let mut collected: Vec<CollectedProvider> = Vec::new();

    for (_app_id, app) in apps {
        for (_id, entry) in &app.providers {
            // Try to extract settings from settings_config
            let (base_url, api_key, models) = extract_v1_settings(&entry.settings_config);
            collected.push(CollectedProvider {
                entry: entry.clone(),
                base_url,
                api_key,
                models,
            });
        }
    }

    // Deduplicate by (base_url, api_key) — merge models
    // Key: (base_url, api_key) → index in deduped vec
    let mut dedup_map: HashMap<(String, String), usize> = HashMap::new();
    let mut deduped: Vec<ProviderEntryFlat> = Vec::new();

    for cp in &collected {
        let key = (cp.base_url.clone(), cp.api_key.clone());
        if let Some(&idx) = dedup_map.get(&key) {
            // Merge models into existing entry
            let existing = &mut deduped[idx];
            let mut existing_models: HashSet<String> =
                existing.models.iter().cloned().collect();
            for model in &cp.models {
                if existing_models.insert(model.clone()) {
                    existing.models.push(model.clone());
                }
            }
        } else {
            let idx = deduped.len();
            // Try to inherit models_url from the matching preset, if any.
            let models_url = cp
                .entry
                .preset_id
                .as_ref()
                .and_then(|preset_id| {
                    get_all_presets_flat()
                        .into_iter()
                        .find(|p| &p.id == preset_id)
                        .map(|p| p.models_url)
                })
                .unwrap_or_default();
            let flat_entry = ProviderEntryFlat {
                id: Uuid::new_v4().to_string(),
                name: cp.entry.name.clone(),
                // Map v1 base_url to base_url_openai (most v1 providers use OpenAI-compatible format)
                base_url_openai: cp.base_url.clone(),
                // base_url_anthropic left empty — can be derived later from presets
                base_url_anthropic: String::new(),
                models_url,
                api_key: cp.api_key.clone(),
                models: cp.models.clone(),
                default_model: cp.models.first().cloned().unwrap_or_default(),
                sort_index: idx as u32,
                preset_id: cp.entry.preset_id.clone(),
                icon_color: cp.entry.icon_color.clone(),
                notes: cp.entry.notes.clone(),
                created_at: cp.entry.created_at,
                meta: cp.entry.meta.clone(),
            };
            dedup_map.insert(key, idx);
            deduped.push(flat_entry);
        }
    }

    // Build tool_activations from each app's `current` field
    let mut tool_activations: HashMap<String, Option<ToolActivation>> = HashMap::new();

    // Map app_id to tool_id: claude.current → tool_activations["claude-code"]
    //                         codex.current → tool_activations["codex"]
    let app_to_tool: &[(&str, &AppProviders)] = &[
        ("claude-code", &old.claude),
        ("codex", &old.codex),
    ];

    for &(tool_id, app) in app_to_tool {
        if let Some(ref current_id) = app.current {
            // Find the provider in the app's providers
            if let Some(entry) = app.providers.get(current_id) {
                let (base_url, api_key, models) = extract_v1_settings(&entry.settings_config);
                let key = (base_url, api_key);

                // Find the corresponding flat provider by dedup key
                if let Some(&idx) = dedup_map.get(&key) {
                    let flat_provider = &deduped[idx];
                    let model = models.first().cloned().unwrap_or_default();
                    tool_activations.insert(
                        tool_id.to_string(),
                        Some(ToolActivation {
                            provider_id: flat_provider.id.clone(),
                            model,
                        }),
                    );
                }
            }
        }
    }

    FlatProvidersStore {
        version: 2,
        providers: deduped,
        tool_activations,
    }
}

/// Extract base_url, api_key, and model names from a v1 settings_config Value.
fn extract_v1_settings(settings_config: &Value) -> (String, String, Vec<String>) {
    // Try to parse as ProviderSettings
    if let Ok(settings) = serde_json::from_value::<ProviderSettings>(settings_config.clone()) {
        let models: Vec<String> = settings
            .models
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.source_model.clone())
            .collect();
        return (settings.base_url, settings.api_key, models);
    }

    // Fallback: try to extract fields directly from the JSON object
    let base_url = settings_config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let api_key = settings_config
        .get("api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let models = settings_config
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    m.get("source_model")
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    (base_url, api_key, models)
}

// ---------------------------------------------------------------------------
// Store read/write (v1 — legacy)
// ---------------------------------------------------------------------------

/// Read the providers store from a specific path.
/// Returns a default empty store if the file doesn't exist or contains malformed JSON.
pub fn read_store_from(path: &Path) -> Result<ProvidersStore> {
    if !path.exists() {
        return Ok(ProvidersStore::default());
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            warn!("Failed to read {}: {e}. Returning default store.", path.display());
            return Ok(ProvidersStore::default());
        }
    };
    let text = text.trim_start_matches('\u{FEFF}');
    match serde_json::from_str::<ProvidersStore>(text) {
        Ok(store) => Ok(store),
        Err(e) => {
            warn!(
                "Malformed JSON in {}: {e}. Returning default empty store.",
                path.display()
            );
            Ok(ProvidersStore::default())
        }
    }
}

/// Read the providers store from the default path.
/// Returns a default empty store if the file doesn't exist or contains malformed JSON.
pub fn read_store() -> Result<ProvidersStore> {
    read_store_from(&store_path())
}

/// Write the providers store to a specific path atomically (write to temp file, then rename).
pub fn write_store_to(store: &ProvidersStore, path: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Serialize to pretty JSON
    let json =
        serde_json::to_string_pretty(store).context("Failed to serialize ProvidersStore")?;

    // Write to a temp file in the same directory, then rename for atomicity
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json.as_bytes())
        .with_context(|| format!("Failed to write temp file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to rename {} to {}", temp_path.display(), path.display()))?;

    Ok(())
}

/// Write the providers store to the default path atomically.
pub fn write_store(store: &ProvidersStore) -> Result<()> {
    write_store_to(store, &store_path())
}

// ---------------------------------------------------------------------------
// Flat provider preset types (v2 architecture)
// ---------------------------------------------------------------------------

/// A built-in provider preset template for the flat store (v2).
///
/// Each preset defines both OpenAI and Anthropic endpoints plus optional
/// metadata for balance queries and API key acquisition. Models are fetched
/// from the provider after creation rather than baked into presets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPresetFlat {
    pub id: String,
    pub name: String,
    /// Category: "domestic", "relay", or "openai_compatible"
    pub category: String,
    pub base_url_openai: String,
    pub base_url_anthropic: String,
    /// Unique "fetch available models" URL for this provider.
    ///
    /// Shared by every agent config (Claude, Codex, …). Most providers expose
    /// an OpenAI-compatible `.../v1/models` endpoint; when empty the frontend
    /// falls back to `base_url_openai + "/models"`.
    #[serde(default)]
    pub models_url: String,
    pub models: Vec<String>,
    pub icon_color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_parser: Option<String>,
}

/// Returns all built-in flat provider presets.
///
/// Includes domestic Chinese model providers, relay/proxy services, and
/// OpenAI-compatible endpoints.
pub fn get_all_presets_flat() -> Vec<ProviderPresetFlat> {
    vec![
        // ── 国内模型 (Domestic) ──
        ProviderPresetFlat {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.deepseek.com/v1".to_string(),
            base_url_anthropic: "https://api.deepseek.com/anthropic".to_string(),
            models_url: "https://api.deepseek.com/v1/models".to_string(),
            models: vec![],
            icon_color: "#4D6BFE".to_string(),
            api_key_url: Some("https://platform.deepseek.com/api_keys".to_string()),
            balance_endpoint: Some("https://api.deepseek.com/user/balance".to_string()),
            balance_parser: Some("deepseek".to_string()),
        },
        ProviderPresetFlat {
            id: "kimi".to_string(),
            name: "Kimi".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.moonshot.cn/v1".to_string(),
            base_url_anthropic: "https://api.moonshot.cn/anthropic".to_string(),
            models_url: "https://api.moonshot.cn/v1/models".to_string(),
            models: vec![],
            icon_color: "#5B45E0".to_string(),
            api_key_url: Some("https://platform.moonshot.cn/console/api-keys".to_string()),
            balance_endpoint: Some("https://api.moonshot.cn/v1/users/me/balance".to_string()),
            balance_parser: Some("kimi".to_string()),
        },
        ProviderPresetFlat {
            id: "kimi-coding".to_string(),
            name: "Kimi For Coding".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.kimi.com/coding/v1".to_string(),
            base_url_anthropic: "https://api.kimi.com/coding/".to_string(),
            models_url: "https://api.moonshot.cn/v1/models".to_string(),
            models: vec![],
            icon_color: "#5B45E0".to_string(),
            api_key_url: Some("https://platform.moonshot.cn/console/api-keys".to_string()),
            balance_endpoint: None,
            balance_parser: None,
        },
        ProviderPresetFlat {
            id: "minimax".to_string(),
            name: "MiniMax".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.minimax.chat/v1".to_string(),
            base_url_anthropic: "https://api.minimax.chat/anthropic".to_string(),
            models_url: "https://api.minimax.chat/v1/models".to_string(),
            models: vec![],
            icon_color: "#FF6B35".to_string(),
            api_key_url: Some("https://platform.minimaxi.com/user-center/basic-information/interface-key".to_string()),
            balance_endpoint: None,
            balance_parser: None,
        },
        ProviderPresetFlat {
            id: "qwen".to_string(),
            name: "通义千问".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            base_url_anthropic: "https://dashscope.aliyuncs.com/api/v2/apps/anthropic".to_string(),
            models_url: "https://dashscope.aliyuncs.com/compatible-mode/v1/models".to_string(),
            models: vec![],
            icon_color: "#6236FF".to_string(),
            api_key_url: Some("https://dashscope.console.aliyun.com/apiKey".to_string()),
            balance_endpoint: None,
            balance_parser: None,
        },
        ProviderPresetFlat {
            id: "qwen-coding".to_string(),
            name: "通义千问 Coding Plan".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://coding-intl.dashscope.aliyuncs.com/v1".to_string(),
            base_url_anthropic: "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic".to_string(),
            models_url: "https://coding-intl.dashscope.aliyuncs.com/v1/models".to_string(),
            models: vec![],
            icon_color: "#6236FF".to_string(),
            api_key_url: Some("https://dashscope.console.aliyun.com/apiKey".to_string()),
            balance_endpoint: None,
            balance_parser: None,
        },
        ProviderPresetFlat {
            id: "glm".to_string(),
            name: "智谱 GLM".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            base_url_anthropic: "https://open.bigmodel.cn/api/anthropic".to_string(),
            models_url: "https://open.bigmodel.cn/api/paas/v4/models".to_string(),
            models: vec![],
            icon_color: "#3366FF".to_string(),
            api_key_url: Some("https://open.bigmodel.cn/usercenter/apikeys".to_string()),
            balance_endpoint: None,
            balance_parser: None,
        },
        ProviderPresetFlat {
            id: "glm-coding".to_string(),
            name: "智谱 GLM Coding Plan".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://api.z.ai/api/coding/paas/v4".to_string(),
            base_url_anthropic: "https://api.z.ai/api/anthropic".to_string(),
            models_url: "https://api.z.ai/api/coding/paas/v4/models".to_string(),
            models: vec![],
            icon_color: "#3366FF".to_string(),
            api_key_url: Some("https://open.bigmodel.cn/usercenter/apikeys".to_string()),
            balance_endpoint: None,
            balance_parser: None,
        },
        ProviderPresetFlat {
            id: "volcengine".to_string(),
            name: "火山方舟".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://ark.cn-beijing.volces.com/api/v3".to_string(),
            base_url_anthropic: "https://ark.cn-beijing.volces.com/api/v3/anthropic".to_string(),
            models_url: "https://ark.cn-beijing.volces.com/api/v3/models".to_string(),
            models: vec![],
            icon_color: "#FF4D4F".to_string(),
            api_key_url: Some("https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey".to_string()),
            balance_endpoint: None,
            balance_parser: None,
        },
        ProviderPresetFlat {
            id: "mimo".to_string(),
            name: "小米 MiMo".to_string(),
            category: "domestic".to_string(),
            base_url_openai: "https://platform.xiaomimimo.com/v1".to_string(),
            base_url_anthropic: "https://platform.xiaomimimo.com/anthropic".to_string(),
            models_url: "https://platform.xiaomimimo.com/v1/models".to_string(),
            models: vec![],
            icon_color: "#FF6900".to_string(),
            api_key_url: Some("https://platform.xiaomimimo.com".to_string()),
            balance_endpoint: None,
            balance_parser: None,
        },
        // ── 官方中转站 (Relay) ──
        ProviderPresetFlat {
            id: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            category: "relay".to_string(),
            base_url_openai: "https://openrouter.ai/api/v1".to_string(),
            base_url_anthropic: String::new(),
            models_url: "https://openrouter.ai/api/v1/models".to_string(),
            models: vec![],
            icon_color: "#6366F1".to_string(),
            api_key_url: Some("https://openrouter.ai/keys".to_string()),
            balance_endpoint: Some("https://openrouter.ai/api/v1/credits".to_string()),
            balance_parser: Some("openrouter".to_string()),
        },
        ProviderPresetFlat {
            id: "siliconflow".to_string(),
            name: "SiliconFlow".to_string(),
            category: "relay".to_string(),
            base_url_openai: "https://api.siliconflow.cn/v1".to_string(),
            base_url_anthropic: String::new(),
            models_url: "https://api.siliconflow.cn/v1/models".to_string(),
            models: vec![],
            icon_color: "#00D4AA".to_string(),
            api_key_url: Some("https://cloud.siliconflow.cn/account/ak".to_string()),
            balance_endpoint: Some("https://api.siliconflow.cn/v1/user/info".to_string()),
            balance_parser: Some("siliconflow".to_string()),
        },
    ]
}

/// Create a new flat provider entry from a built-in preset.
///
/// Looks up the preset by ID, generates a UUID, sets the current timestamp,
/// and copies all relevant fields from the preset template.
///
/// # Arguments
/// * `preset_id` - The ID of the preset to use (e.g., "deepseek", "kimi")
/// * `api_key` - The user's API key for this provider
///
/// # Returns
/// A fully populated `ProviderEntryFlat` ready to be inserted into the store.
///
/// # Errors
/// Returns an error if the `preset_id` is not found in the preset registry.
pub fn create_from_preset_flat(preset_id: &str, api_key: &str) -> Result<ProviderEntryFlat> {
    let presets = get_all_presets_flat();
    let preset = presets
        .into_iter()
        .find(|p| p.id == preset_id)
        .with_context(|| format!("Preset '{}' not found in flat preset registry", preset_id))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    Ok(ProviderEntryFlat {
        id: Uuid::new_v4().to_string(),
        name: preset.name,
        base_url_openai: preset.base_url_openai,
        base_url_anthropic: preset.base_url_anthropic,
        models_url: preset.models_url,
        api_key: api_key.to_string(),
        models: vec![],
        default_model: String::new(),
        sort_index: 0,
        preset_id: Some(preset.id),
        icon_color: Some(preset.icon_color),
        notes: None,
        created_at: Some(now),
        meta: None,
    })
}

// ---------------------------------------------------------------------------
// Flat store CRUD operations (v2 architecture)
// ---------------------------------------------------------------------------

/// Validate that a URL string is a valid HTTP/HTTPS URL.
///
/// Returns Ok(()) if the URL is valid, or an error describing the issue.
fn validate_url(url_str: &str) -> Result<()> {
    if url_str.is_empty() {
        return Ok(()); // Empty URLs are allowed (e.g., base_url_anthropic may be empty)
    }
    let parsed = Url::parse(url_str)
        .with_context(|| format!("Invalid URL format: '{}'", url_str))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => bail!("URL must use http or https scheme, got: '{}'", scheme),
    }
}

/// Create a new provider in the flat store.
///
/// - Validates that `name` is non-empty
/// - Validates URL format for `base_url_openai` and `base_url_anthropic`
/// - Generates a new UUID for the `id` field (overwrites any provided id)
/// - Sets `created_at` to the current timestamp if not already set
/// - Sets `sort_index` to max existing + 1
/// - Pushes the entry to `store.providers`
///
/// # Errors
/// Returns an error if:
/// - `name` is empty
/// - `base_url_openai` or `base_url_anthropic` has an invalid URL format
pub fn create_provider_flat(
    store: &mut FlatProvidersStore,
    mut entry: ProviderEntryFlat,
) -> Result<ProviderEntryFlat> {
    // Validate name is non-empty
    if entry.name.trim().is_empty() {
        bail!("Provider name must not be empty");
    }

    // Validate URL formats
    validate_url(&entry.base_url_openai)?;
    validate_url(&entry.base_url_anthropic)?;
    validate_url(&entry.models_url)?;

    // Generate new UUID (overwrite any provided id)
    entry.id = Uuid::new_v4().to_string();

    // Set created_at to current timestamp if not set
    if entry.created_at.is_none() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        entry.created_at = Some(now);
    }

    // Set sort_index to max existing + 1
    let max_sort_index = store
        .providers
        .iter()
        .map(|p| p.sort_index)
        .max()
        .unwrap_or(0);
    entry.sort_index = if store.providers.is_empty() {
        0
    } else {
        max_sort_index + 1
    };

    // Push to store
    store.providers.push(entry.clone());

    Ok(entry)
}

/// Update an existing provider in the flat store with a partial patch.
///
/// Finds the provider by `id` and applies all non-None fields from the patch.
///
/// # Errors
/// Returns an error if:
/// - No provider with the given `id` exists in the store
pub fn update_provider_flat(
    store: &mut FlatProvidersStore,
    id: &str,
    patch: ProviderPatchFlat,
) -> Result<ProviderEntryFlat> {
    let provider = store
        .providers
        .iter_mut()
        .find(|p| p.id == id)
        .with_context(|| format!("Provider '{}' not found", id))?;

    // Apply non-None fields from patch
    if let Some(name) = patch.name {
        provider.name = name;
    }
    if let Some(base_url_openai) = patch.base_url_openai {
        provider.base_url_openai = base_url_openai;
    }
    if let Some(base_url_anthropic) = patch.base_url_anthropic {
        provider.base_url_anthropic = base_url_anthropic;
    }
    if let Some(models_url) = patch.models_url {
        provider.models_url = models_url;
    }
    if let Some(api_key) = patch.api_key {
        provider.api_key = api_key;
    }
    if let Some(models) = patch.models {
        provider.models = models;
    }
    if let Some(default_model) = patch.default_model {
        provider.default_model = default_model;
    }
    if let Some(sort_index) = patch.sort_index {
        provider.sort_index = sort_index;
    }
    if let Some(preset_id) = patch.preset_id {
        provider.preset_id = Some(preset_id);
    }
    if let Some(icon_color) = patch.icon_color {
        provider.icon_color = Some(icon_color);
    }
    if let Some(notes) = patch.notes {
        provider.notes = Some(notes);
    }
    if let Some(meta) = patch.meta {
        provider.meta = Some(meta);
    }

    Ok(provider.clone())
}

/// Delete a provider from the flat store by ID.
///
/// Also cleans up `tool_activations`: any activation referencing this provider
/// is set to `None`.
///
/// # Errors
/// Returns an error if no provider with the given `id` exists in the store.
pub fn delete_provider_flat(store: &mut FlatProvidersStore, id: &str) -> Result<()> {
    let idx = store
        .providers
        .iter()
        .position(|p| p.id == id)
        .with_context(|| format!("Provider '{}' not found", id))?;

    // Remove the provider
    store.providers.remove(idx);

    // Clean up tool_activations: set any activation referencing this provider to None
    for activation in store.tool_activations.values_mut() {
        if let Some(act) = activation {
            if act.provider_id == id {
                *activation = None;
            }
        }
    }

    Ok(())
}

/// Reorder providers by assigning new `sort_index` values based on the given ID list.
///
/// For each ID in `ordered_ids`, assigns `sort_index = position index` (0-based).
/// Providers not in `ordered_ids` keep their existing `sort_index`.
///
/// # Errors
/// Returns an error if any ID in `ordered_ids` doesn't exist in the store.
pub fn reorder_providers(store: &mut FlatProvidersStore, ordered_ids: &[String]) -> Result<()> {
    // Validate all IDs exist
    for id in ordered_ids {
        if !store.providers.iter().any(|p| p.id == *id) {
            bail!("Provider '{}' not found in store", id);
        }
    }

    // Assign sort_index based on position in ordered_ids
    for (index, id) in ordered_ids.iter().enumerate() {
        if let Some(provider) = store.providers.iter_mut().find(|p| p.id == *id) {
            provider.sort_index = index as u32;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tool activation/deactivation (v2 architecture)
// ---------------------------------------------------------------------------

/// Activate a provider for a specific Agent tool.
///
/// This updates the `tool_activations` map to record which provider and model
/// a given tool should use. Only one provider can be active per tool at a time —
/// activating a new provider automatically replaces any previous activation.
///
/// # Validation
/// - The provider must exist in the store
/// - The required URL must be non-empty based on the tool:
///   - `"claude-code"` requires `base_url_anthropic` to be non-empty
///   - `"codex"` requires `base_url_openai` to be non-empty
///   - Other tools: require `base_url_openai` to be non-empty (default)
///
/// # Model Resolution
/// Uses the provided `model` if given, otherwise falls back to the provider's `default_model`.
///
/// # Returns
/// The `ToolActivation` that was inserted into the map.
///
/// # Errors
/// - Provider not found
/// - Required URL is empty for the target tool
pub fn activate_tool(
    store: &mut FlatProvidersStore,
    provider_id: &str,
    tool_id: &str,
    model: Option<&str>,
) -> Result<ToolActivation> {
    // 1. Find provider by provider_id
    let provider = store
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .with_context(|| format!("Provider '{}' not found", provider_id))?;

    // 2. Validate the required URL is non-empty based on tool_id
    match tool_id {
        "claude-code" => {
            if provider.base_url_anthropic.trim().is_empty() {
                bail!(
                    "Provider '{}' has no Anthropic-compatible endpoint (base_url_anthropic is empty). \
                     Claude Code requires an Anthropic-compatible URL.",
                    provider.name
                );
            }
        }
        "codex" => {
            if provider.base_url_openai.trim().is_empty() {
                bail!(
                    "Provider '{}' has no OpenAI-compatible endpoint (base_url_openai is empty). \
                     Codex requires an OpenAI-compatible URL.",
                    provider.name
                );
            }
        }
        _ => {
            // Default: require base_url_openai
            if provider.base_url_openai.trim().is_empty() {
                bail!(
                    "Provider '{}' has no OpenAI-compatible endpoint (base_url_openai is empty). \
                     Tool '{}' requires an OpenAI-compatible URL.",
                    provider.name,
                    tool_id
                );
            }
        }
    }

    // 3. Determine model: use provided model, or fall back to provider's default_model
    let resolved_model = match model {
        Some(m) if !m.trim().is_empty() => m.to_string(),
        _ => provider.default_model.clone(),
    };

    // 4. Create ToolActivation
    let activation = ToolActivation {
        provider_id: provider_id.to_string(),
        model: resolved_model,
    };

    // 5. Insert into store.tool_activations (replaces any previous activation for this tool)
    store
        .tool_activations
        .insert(tool_id.to_string(), Some(activation.clone()));

    // 6. Return the activation
    Ok(activation)
}

/// Deactivate a tool by removing its activation entry.
///
/// Sets the tool's entry in `tool_activations` to `None`, effectively clearing
/// the active provider for that tool.
///
/// # Returns
/// The previous `ToolActivation` (if any) so the caller can use it for backup
/// restoration or undo operations.
pub fn deactivate_tool(
    store: &mut FlatProvidersStore,
    tool_id: &str,
) -> Result<Option<ToolActivation>> {
    // Remove or set to None the tool_id entry in tool_activations
    let previous = store
        .tool_activations
        .insert(tool_id.to_string(), None)
        .flatten();

    Ok(previous)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the mutable AppProviders for a given app_id.
fn get_app_mut<'a>(store: &'a mut ProvidersStore, app_id: &str) -> Result<&'a mut AppProviders> {
    match app_id {
        "claude" => Ok(&mut store.claude),
        "codex" => Ok(&mut store.codex),
        "opencode" => Ok(&mut store.opencode),
        "gemini" => Ok(&mut store.gemini),
        _ => bail!("Unknown app_id: {app_id}"),
    }
}

/// Get the immutable AppProviders for a given app_id.
fn get_app<'a>(store: &'a ProvidersStore, app_id: &str) -> &'a AppProviders {
    match app_id {
        "claude" => &store.claude,
        "codex" => &store.codex,
        "opencode" => &store.opencode,
        "gemini" => &store.gemini,
        _ => &store.claude,
    }
}

/// Validate a provider entry before creation.
fn validate_entry(entry: &ProviderEntry) -> Result<()> {
    // Name: non-empty, max 64 chars
    if entry.name.is_empty() {
        bail!("Provider name must not be empty");
    }
    if entry.name.len() > 64 {
        bail!(
            "Provider name must be at most 64 characters (got {})",
            entry.name.len()
        );
    }

    // Base URL: must be a valid URL
    let settings: ProviderSettings = serde_json::from_value(entry.settings_config.clone())
        .context("Invalid settings_config structure")?;

    if Url::parse(&settings.base_url).is_err() {
        bail!("Invalid base_url: {}", settings.base_url);
    }

    // Models: at least one
    if settings.models.is_empty() {
        bail!("At least one model mapping is required");
    }

    Ok(())
}

pub fn get_providers(app_id: &str) -> Result<(HashMap<String, ProviderEntry>, Option<String>)> {
    let store = read_store()?;
    let app = get_app(&store, app_id);
    Ok((app.providers.clone(), app.current.clone()))
}

// ---------------------------------------------------------------------------
// CRUD operations
// ---------------------------------------------------------------------------

/// Create a new provider entry for the given app_id.
///
/// Validates name length, URL format, model count, and ID uniqueness.
/// If this is the first provider for the app, it becomes the current active provider.
pub fn create_provider(app_id: &str, entry: ProviderEntry) -> Result<ProviderEntry> {
    create_provider_at(app_id, entry, &store_path())
}

/// Create a new provider entry, writing to a specific store path.
pub fn create_provider_at(
    app_id: &str,
    mut entry: ProviderEntry,
    path: &Path,
) -> Result<ProviderEntry> {
    validate_entry(&entry)?;

    let mut store = read_store_from(path)?;
    let app = get_app_mut(&mut store, app_id)?;

    // Check ID uniqueness
    if app.providers.contains_key(&entry.id) {
        bail!(
            "Provider ID '{}' already exists for app '{}'",
            entry.id,
            app_id
        );
    }

    // Assign metadata
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    entry.created_at = Some(now);
    entry.sort_index = Some(app.providers.len() as u32);

    // Auto-activate if first provider
    if app.current.is_none() {
        app.current = Some(entry.id.clone());
    }

    // Insert
    app.providers.insert(entry.id.clone(), entry.clone());

    // Persist
    write_store_to(&store, path)?;

    Ok(entry)
}

/// Update an existing provider with a partial patch.
///
/// Only non-None fields in the patch are applied.
pub fn update_provider(app_id: &str, id: &str, patch: ProviderPatch) -> Result<ProviderEntry> {
    update_provider_at(app_id, id, patch, &store_path())
}

/// Update an existing provider, writing to a specific store path.
pub fn update_provider_at(
    app_id: &str,
    id: &str,
    patch: ProviderPatch,
    path: &Path,
) -> Result<ProviderEntry> {
    let mut store = read_store_from(path)?;
    let app = get_app_mut(&mut store, app_id)?;

    let entry = app
        .providers
        .get_mut(id)
        .with_context(|| format!("Provider '{}' not found in app '{}'", id, app_id))?;

    // Apply patch fields
    if let Some(name) = &patch.name {
        if name.is_empty() {
            bail!("Provider name must not be empty");
        }
        if name.len() > 64 {
            bail!("Provider name must be at most 64 characters");
        }
        entry.name = name.clone();
    }
    if let Some(category) = patch.category {
        entry.category = category;
    }
    if let Some(settings_config) = patch.settings_config {
        // Validate the new settings if provided
        let settings: ProviderSettings = serde_json::from_value(settings_config.clone())
            .context("Invalid settings_config structure")?;
        if Url::parse(&settings.base_url).is_err() {
            bail!("Invalid base_url: {}", settings.base_url);
        }
        if settings.models.is_empty() {
            bail!("At least one model mapping is required");
        }
        entry.settings_config = settings_config;
    }
    if let Some(website_url) = patch.website_url {
        entry.website_url = Some(website_url);
    }
    if let Some(api_key_url) = patch.api_key_url {
        entry.api_key_url = Some(api_key_url);
    }
    if let Some(icon_color) = patch.icon_color {
        entry.icon_color = Some(icon_color);
    }
    if let Some(notes) = patch.notes {
        entry.notes = Some(notes);
    }
    if let Some(sort_index) = patch.sort_index {
        entry.sort_index = Some(sort_index);
    }
    if let Some(meta) = patch.meta {
        entry.meta = Some(meta);
    }

    let updated = entry.clone();

    // Persist
    write_store_to(&store, path)?;

    Ok(updated)
}

/// Delete a provider by ID.
///
/// If the deleted provider is the currently active one, `current` is set to None.
pub fn delete_provider(app_id: &str, id: &str) -> Result<()> {
    delete_provider_at(app_id, id, &store_path())
}

/// Delete a provider by ID, writing to a specific store path.
pub fn delete_provider_at(app_id: &str, id: &str, path: &Path) -> Result<()> {
    let mut store = read_store_from(path)?;
    let app = get_app_mut(&mut store, app_id)?;

    if app.providers.remove(id).is_none() {
        bail!("Provider '{}' not found in app '{}'", id, app_id);
    }

    // Nullify current if we just deleted the active provider
    if app.current.as_deref() == Some(id) {
        app.current = None;
    }

    // Persist
    write_store_to(&store, path)?;

    Ok(())
}

/// Switch the active provider for an app.
///
/// Updates the `current` field to the given provider_id.
pub fn switch_active_provider(app_id: &str, provider_id: &str) -> Result<()> {
    switch_active_provider_at(app_id, provider_id, &store_path())
}

/// Switch the active provider, writing to a specific store path.
pub fn switch_active_provider_at(app_id: &str, provider_id: &str, path: &Path) -> Result<()> {
    let mut store = read_store_from(path)?;
    let app = get_app_mut(&mut store, app_id)?;

    if !app.providers.contains_key(provider_id) {
        bail!(
            "Provider '{}' not found in app '{}', cannot activate",
            provider_id,
            app_id
        );
    }

    app.current = Some(provider_id.to_string());

    // Persist
    write_store_to(&store, path)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

/// Returns the list of built-in provider presets.
pub fn get_provider_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "official".to_string(),
            name: "Official (Anthropic)".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key_url: "https://console.anthropic.com/settings/keys".to_string(),
            icon_color: "#D97757".to_string(),
            models: vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-opus-4-20250514".to_string(),
            ],
        },
        ProviderPreset {
            id: "official".to_string(),
            name: "Official (OpenAI)".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key_url: "https://platform.openai.com/api-keys".to_string(),
            icon_color: "#10A37F".to_string(),
            models: vec![
                "codex-mini-latest".to_string(),
                "o3".to_string(),
                "o4-mini".to_string(),
            ],
        },
        ProviderPreset {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key_url: "https://platform.deepseek.com/api_keys".to_string(),
            icon_color: "#4D6BFE".to_string(),
            models: vec![
                "deepseek-chat".to_string(),
                "deepseek-reasoner".to_string(),
            ],
        },
        ProviderPreset {
            id: "kimi".to_string(),
            name: "Kimi".to_string(),
            base_url: "https://api.moonshot.cn/v1".to_string(),
            api_key_url: "https://platform.moonshot.cn/console/api-keys".to_string(),
            icon_color: "#5B45E0".to_string(),
            models: vec![
                "moonshot-v1-128k".to_string(),
                "moonshot-v1-32k".to_string(),
            ],
        },
        ProviderPreset {
            id: "glm".to_string(),
            name: "GLM".to_string(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            api_key_url: "https://open.bigmodel.cn/usercenter/apikeys".to_string(),
            icon_color: "#3366FF".to_string(),
            models: vec![
                "glm-4-plus".to_string(),
                "glm-4-flash".to_string(),
            ],
        },
    ]
}

/// Resolve the correct preset for a given app_id and preset_id.
///
/// For "official" preset: Claude uses Anthropic, Codex uses OpenAI.
/// For other presets (deepseek, kimi, glm): same preset for any app.
fn resolve_preset(app_id: &str, preset_id: &str) -> Result<ProviderPreset> {
    let presets = get_provider_presets();

    match preset_id {
        "official" => {
            let target_name = match app_id {
                "claude" => "Official (Anthropic)",
                "codex" => "Official (OpenAI)",
                _ => "Official (Anthropic)",
            };
            presets
                .into_iter()
                .find(|p| p.id == "official" && p.name == target_name)
                .with_context(|| format!("Official preset not found for app '{}'", app_id))
        }
        _ => presets
            .into_iter()
            .find(|p| p.id == preset_id)
            .with_context(|| format!("Preset '{}' not found", preset_id)),
    }
}

/// Create a provider from a built-in preset.
///
/// The user only needs to provide an API key; all other fields are pre-filled from the preset.
pub fn create_from_preset(app_id: &str, preset_id: &str, api_key: &str) -> Result<ProviderEntry> {
    create_from_preset_at(app_id, preset_id, api_key, &store_path())
}

/// Create a provider from a built-in preset, writing to a specific store path.
pub fn create_from_preset_at(
    app_id: &str,
    preset_id: &str,
    api_key: &str,
    path: &Path,
) -> Result<ProviderEntry> {
    let preset = resolve_preset(app_id, preset_id)?;

    let models: Vec<ModelMapping> = preset
        .models
        .iter()
        .map(|m| ModelMapping {
            source_model: m.clone(),
            target_model: m.clone(),
            enabled: true,
        })
        .collect();

    let settings = ProviderSettings {
        base_url: preset.base_url.clone(),
        api_key: api_key.to_string(),
        models,
        timeout_ms: None,
        max_retries: None,
    };

    let entry = ProviderEntry {
        id: Uuid::new_v4().to_string(),
        name: preset.name.clone(),
        category: "cloud".to_string(),
        settings_config: serde_json::to_value(&settings)
            .context("Failed to serialize preset settings")?,
        preset_id: Some(preset.id.clone()),
        website_url: None,
        api_key_url: Some(preset.api_key_url.clone()),
        icon_color: Some(preset.icon_color.clone()),
        notes: None,
        created_at: None, // Will be set by create_provider_at
        sort_index: None, // Will be set by create_provider_at
        meta: None,
    };

    create_provider_at(app_id, entry, path)
}


// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    /// Helper: create a temp directory with a store file path inside it.
    fn setup_temp_store() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("model_providers.json");
        (tmp, path)
    }

    fn make_valid_settings() -> Value {
        serde_json::to_value(ProviderSettings {
            base_url: "https://api.example.com/v1".to_string(),
            api_key: "sk-test-key-12345".to_string(),
            models: vec![ModelMapping {
                source_model: "model-a".to_string(),
                target_model: "model-a".to_string(),
                enabled: true,
            }],
            timeout_ms: None,
            max_retries: None,
        })
        .unwrap()
    }

    fn make_valid_entry(id: &str, name: &str) -> ProviderEntry {
        ProviderEntry {
            id: id.to_string(),
            name: name.to_string(),
            category: "cloud".to_string(),
            settings_config: make_valid_settings(),
            preset_id: None,
            website_url: None,
            api_key_url: None,
            icon_color: None,
            notes: None,
            created_at: None,
            sort_index: None,
            meta: None,
        }
    }

    // -----------------------------------------------------------------------
    // Flat store read/write tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_flat_store_missing_file() {
        let (_tmp, path) = setup_temp_store();
        let store = read_flat_store(&path).unwrap();
        assert_eq!(store.version, 2);
        assert!(store.providers.is_empty());
        assert!(store.tool_activations.is_empty());
    }

    #[test]
    fn test_read_flat_store_malformed_json() {
        let (_tmp, path) = setup_temp_store();
        std::fs::write(&path, "not valid json {{{").unwrap();
        let store = read_flat_store(&path).unwrap();
        assert_eq!(store.version, 2);
        assert!(store.providers.is_empty());
        assert!(store.tool_activations.is_empty());
    }

    #[test]
    fn test_read_flat_store_with_bom() {
        let (_tmp, path) = setup_temp_store();
        let store = FlatProvidersStore {
            version: 2,
            providers: vec![ProviderEntryFlat {
                id: "test-id".to_string(),
                name: "Test".to_string(),
                base_url_openai: "https://api.example.com/v1".to_string(),
                base_url_anthropic: String::new(),
                models_url: String::new(),
                api_key: "sk-key".to_string(),
                models: vec!["model-a".to_string()],
                default_model: "model-a".to_string(),
                sort_index: 0,
                preset_id: None,
                icon_color: None,
                notes: None,
                created_at: None,
                meta: None,
            }],
            tool_activations: HashMap::new(),
        };
        let json = serde_json::to_string_pretty(&store).unwrap();
        let content = format!("\u{FEFF}{}", json);
        std::fs::write(&path, content).unwrap();

        let loaded = read_flat_store(&path).unwrap();
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].id, "test-id");
    }

    #[test]
    fn test_write_and_read_flat_store() {
        let (_tmp, path) = setup_temp_store();
        let store = FlatProvidersStore {
            version: 2,
            providers: vec![ProviderEntryFlat {
                id: "p1".to_string(),
                name: "Provider 1".to_string(),
                base_url_openai: "https://api.deepseek.com/v1".to_string(),
                base_url_anthropic: "https://api.deepseek.com/anthropic".to_string(),
                models_url: "https://api.deepseek.com/v1/models".to_string(),
                api_key: "sk-test".to_string(),
                models: vec!["deepseek-chat".to_string()],
                default_model: "deepseek-chat".to_string(),
                sort_index: 0,
                preset_id: Some("deepseek".to_string()),
                icon_color: Some("#4D6BFE".to_string()),
                notes: None,
                created_at: Some(1719000000000),
                meta: None,
            }],
            tool_activations: {
                let mut map = HashMap::new();
                map.insert(
                    "claude-code".to_string(),
                    Some(ToolActivation {
                        provider_id: "p1".to_string(),
                        model: "deepseek-chat".to_string(),
                    }),
                );
                map.insert("codex".to_string(), None);
                map
            },
        };

        write_flat_store(&store, &path).unwrap();
        let loaded = read_flat_store(&path).unwrap();

        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].id, "p1");
        assert_eq!(loaded.providers[0].name, "Provider 1");
        assert_eq!(loaded.providers[0].base_url_openai, "https://api.deepseek.com/v1");
        assert_eq!(loaded.providers[0].base_url_anthropic, "https://api.deepseek.com/anthropic");
        assert_eq!(loaded.providers[0].api_key, "sk-test");
        assert_eq!(loaded.providers[0].models, vec!["deepseek-chat"]);
        assert_eq!(loaded.providers[0].default_model, "deepseek-chat");
        assert_eq!(loaded.providers[0].sort_index, 0);
        assert_eq!(loaded.providers[0].preset_id, Some("deepseek".to_string()));
        assert_eq!(loaded.providers[0].icon_color, Some("#4D6BFE".to_string()));
        assert_eq!(loaded.providers[0].created_at, Some(1719000000000));

        // Check tool_activations
        let claude_activation = loaded.tool_activations.get("claude-code").unwrap();
        assert!(claude_activation.is_some());
        let activation = claude_activation.as_ref().unwrap();
        assert_eq!(activation.provider_id, "p1");
        assert_eq!(activation.model, "deepseek-chat");

        let codex_activation = loaded.tool_activations.get("codex").unwrap();
        assert!(codex_activation.is_none());
    }

    #[test]
    fn test_write_flat_store_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("dir").join("store.json");
        let store = FlatProvidersStore::default();
        write_flat_store(&store, &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_write_flat_store_atomic_no_temp_file_left() {
        let (_tmp, path) = setup_temp_store();
        let store = FlatProvidersStore::default();
        write_flat_store(&store, &path).unwrap();

        // The temp file should not exist after a successful write
        let temp_path = path.with_extension("json.tmp");
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_read_flat_store_empty_file() {
        let (_tmp, path) = setup_temp_store();
        std::fs::write(&path, "").unwrap();
        let store = read_flat_store(&path).unwrap();
        assert_eq!(store.version, 2);
        assert!(store.providers.is_empty());
    }

    // -----------------------------------------------------------------------
    // V1 store tests (existing)
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_store_missing_file() {
        let (_tmp, path) = setup_temp_store();
        let store = read_store_from(&path).unwrap();
        assert!(store.claude.providers.is_empty());
        assert!(store.codex.providers.is_empty());
    }

    #[test]
    fn test_read_store_malformed_json() {
        let (_tmp, path) = setup_temp_store();
        std::fs::write(&path, "not valid json {{{").unwrap();
        let store = read_store_from(&path).unwrap();
        assert!(store.claude.providers.is_empty());
        assert!(store.codex.providers.is_empty());
    }

    #[test]
    fn test_read_store_with_bom() {
        let (_tmp, path) = setup_temp_store();
        let json = r#"{"claude":{"providers":{},"current":null},"codex":{"providers":{},"current":null},"opencode":{"providers":{},"current":null},"gemini":{"providers":{},"current":null}}"#;
        let content = format!("\u{FEFF}{}", json);
        std::fs::write(&path, content).unwrap();
        let store = read_store_from(&path).unwrap();
        assert!(store.claude.providers.is_empty());
    }

    #[test]
    fn test_write_and_read_store() {
        let (_tmp, path) = setup_temp_store();
        let mut store = ProvidersStore::default();
        store.claude.current = Some("test-id".to_string());
        write_store_to(&store, &path).unwrap();

        let loaded = read_store_from(&path).unwrap();
        assert_eq!(loaded.claude.current, Some("test-id".to_string()));
    }

    #[test]
    fn test_atomic_write_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nested").join("dir").join("store.json");
        let store = ProvidersStore::default();
        write_store_to(&store, &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_create_provider_valid() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "My Provider");
        let result = create_provider_at("claude", entry, &path).unwrap();
        assert_eq!(result.id, "p1");
        assert!(result.created_at.is_some());
        assert_eq!(result.sort_index, Some(0));

        // Should be auto-activated (first provider)
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("p1".to_string()));
    }

    #[test]
    fn test_create_provider_empty_name() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "");
        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name must not be empty"));
    }

    #[test]
    fn test_create_provider_name_too_long() {
        let (_tmp, path) = setup_temp_store();
        let long_name = "a".repeat(65);
        let entry = make_valid_entry("p1", &long_name);
        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at most 64 characters"));
    }

    #[test]
    fn test_create_provider_name_exactly_64_chars() {
        let (_tmp, path) = setup_temp_store();
        let name = "a".repeat(64);
        let entry = make_valid_entry("p1", &name);
        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_invalid_url() {
        let (_tmp, path) = setup_temp_store();
        let mut entry = make_valid_entry("p1", "Test");
        let mut settings: ProviderSettings =
            serde_json::from_value(entry.settings_config.clone()).unwrap();
        settings.base_url = "not-a-url".to_string();
        entry.settings_config = serde_json::to_value(&settings).unwrap();

        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid base_url"));
    }

    #[test]
    fn test_create_provider_no_models() {
        let (_tmp, path) = setup_temp_store();
        let mut entry = make_valid_entry("p1", "Test");
        let mut settings: ProviderSettings =
            serde_json::from_value(entry.settings_config.clone()).unwrap();
        settings.models = vec![];
        entry.settings_config = serde_json::to_value(&settings).unwrap();

        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("At least one model"));
    }

    #[test]
    fn test_create_provider_duplicate_id() {
        let (_tmp, path) = setup_temp_store();
        let entry1 = make_valid_entry("p1", "Provider 1");
        create_provider_at("claude", entry1, &path).unwrap();

        let entry2 = make_valid_entry("p1", "Provider 2");
        let result = create_provider_at("claude", entry2, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_first_provider_auto_activation() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("first", "First Provider");
        create_provider_at("codex", entry, &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert_eq!(store.codex.current, Some("first".to_string()));

        // Second provider should NOT change current
        let entry2 = make_valid_entry("second", "Second Provider");
        create_provider_at("codex", entry2, &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert_eq!(store.codex.current, Some("first".to_string()));
    }

    #[test]
    fn test_update_provider() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Original");
        create_provider_at("claude", entry, &path).unwrap();

        let patch = ProviderPatch {
            name: Some("Updated Name".to_string()),
            ..Default::default()
        };
        let updated = update_provider_at("claude", "p1", patch, &path).unwrap();
        assert_eq!(updated.name, "Updated Name");

        // Verify persistence
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.providers["p1"].name, "Updated Name");
    }

    #[test]
    fn test_update_provider_settings() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Test");
        create_provider_at("claude", entry, &path).unwrap();

        let new_settings = ProviderSettings {
            base_url: "https://new-api.example.com/v1".to_string(),
            api_key: "new-key".to_string(),
            models: vec![ModelMapping {
                source_model: "new-model".to_string(),
                target_model: "new-model".to_string(),
                enabled: true,
            }],
            timeout_ms: Some(5000),
            max_retries: None,
        };
        let patch = ProviderPatch {
            settings_config: Some(serde_json::to_value(&new_settings).unwrap()),
            ..Default::default()
        };
        let updated = update_provider_at("claude", "p1", patch, &path).unwrap();
        let loaded_settings: ProviderSettings =
            serde_json::from_value(updated.settings_config).unwrap();
        assert_eq!(loaded_settings.base_url, "https://new-api.example.com/v1");
        assert_eq!(loaded_settings.api_key, "new-key");
    }

    #[test]
    fn test_update_provider_not_found() {
        let (_tmp, path) = setup_temp_store();
        let patch = ProviderPatch {
            name: Some("New".to_string()),
            ..Default::default()
        };
        let result = update_provider_at("claude", "nonexistent", patch, &path);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_provider_invalid_name() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Original");
        create_provider_at("claude", entry, &path).unwrap();

        let patch = ProviderPatch {
            name: Some("".to_string()),
            ..Default::default()
        };
        let result = update_provider_at("claude", "p1", patch, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name must not be empty"));
    }

    #[test]
    fn test_delete_provider() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "To Delete");
        create_provider_at("claude", entry, &path).unwrap();

        delete_provider_at("claude", "p1", &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert!(!store.claude.providers.contains_key("p1"));
    }

    #[test]
    fn test_delete_active_provider_nullifies_current() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("active", "Active Provider");
        create_provider_at("claude", entry, &path).unwrap();

        // Verify it's active
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("active".to_string()));

        // Delete it
        delete_provider_at("claude", "active", &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, None);
    }

    #[test]
    fn test_delete_non_active_provider_keeps_current() {
        let (_tmp, path) = setup_temp_store();
        let entry1 = make_valid_entry("p1", "Provider 1");
        let entry2 = make_valid_entry("p2", "Provider 2");
        create_provider_at("claude", entry1, &path).unwrap();
        create_provider_at("claude", entry2, &path).unwrap();

        // p1 is current (first created)
        delete_provider_at("claude", "p2", &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("p1".to_string()));
    }

    #[test]
    fn test_delete_provider_not_found() {
        let (_tmp, path) = setup_temp_store();
        let result = delete_provider_at("claude", "nonexistent", &path);
        assert!(result.is_err());
    }

    #[test]
    fn test_switch_active_provider() {
        let (_tmp, path) = setup_temp_store();
        let entry1 = make_valid_entry("p1", "Provider 1");
        let entry2 = make_valid_entry("p2", "Provider 2");
        create_provider_at("claude", entry1, &path).unwrap();
        create_provider_at("claude", entry2, &path).unwrap();

        // p1 is auto-activated as first
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("p1".to_string()));

        // Switch to p2
        switch_active_provider_at("claude", "p2", &path).unwrap();
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("p2".to_string()));
    }

    #[test]
    fn test_switch_active_provider_not_found() {
        let (_tmp, path) = setup_temp_store();
        let result = switch_active_provider_at("claude", "nonexistent", &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_get_provider_presets_count() {
        let presets = get_provider_presets();
        assert_eq!(presets.len(), 5);
    }

    #[test]
    fn test_get_provider_presets_official_anthropic() {
        let presets = get_provider_presets();
        let anthropic = presets.iter().find(|p| p.name == "Official (Anthropic)").unwrap();
        assert_eq!(anthropic.id, "official");
        assert_eq!(anthropic.base_url, "https://api.anthropic.com");
        assert_eq!(anthropic.icon_color, "#D97757");
        assert_eq!(anthropic.models.len(), 2);
    }

    #[test]
    fn test_get_provider_presets_official_openai() {
        let presets = get_provider_presets();
        let openai = presets.iter().find(|p| p.name == "Official (OpenAI)").unwrap();
        assert_eq!(openai.id, "official");
        assert_eq!(openai.base_url, "https://api.openai.com/v1");
        assert_eq!(openai.icon_color, "#10A37F");
        assert_eq!(openai.models.len(), 3);
    }

    #[test]
    fn test_create_from_preset_claude_official() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("claude", "official", "sk-test-key", &path).unwrap();
        assert_eq!(result.name, "Official (Anthropic)");
        assert_eq!(result.preset_id, Some("official".to_string()));
        assert_eq!(result.icon_color, Some("#D97757".to_string()));
        assert_eq!(
            result.api_key_url,
            Some("https://console.anthropic.com/settings/keys".to_string())
        );

        let settings: ProviderSettings =
            serde_json::from_value(result.settings_config).unwrap();
        assert_eq!(settings.base_url, "https://api.anthropic.com");
        assert_eq!(settings.api_key, "sk-test-key");
        assert_eq!(settings.models.len(), 2);
    }

    #[test]
    fn test_create_from_preset_codex_official() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("codex", "official", "sk-openai-key", &path).unwrap();
        assert_eq!(result.name, "Official (OpenAI)");
        assert_eq!(result.preset_id, Some("official".to_string()));
        assert_eq!(result.icon_color, Some("#10A37F".to_string()));

        let settings: ProviderSettings =
            serde_json::from_value(result.settings_config).unwrap();
        assert_eq!(settings.base_url, "https://api.openai.com/v1");
        assert_eq!(settings.api_key, "sk-openai-key");
        assert_eq!(settings.models.len(), 3);
    }

    #[test]
    fn test_create_from_preset_deepseek() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("claude", "deepseek", "ds-key", &path).unwrap();
        assert_eq!(result.name, "DeepSeek");
        assert_eq!(result.preset_id, Some("deepseek".to_string()));
        assert_eq!(result.icon_color, Some("#4D6BFE".to_string()));

        let settings: ProviderSettings =
            serde_json::from_value(result.settings_config).unwrap();
        assert_eq!(settings.base_url, "https://api.deepseek.com/v1");
        assert_eq!(settings.models.len(), 2);
    }

    #[test]
    fn test_create_from_preset_kimi() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("codex", "kimi", "kimi-key", &path).unwrap();
        assert_eq!(result.name, "Kimi");
        assert_eq!(result.preset_id, Some("kimi".to_string()));
        assert_eq!(result.icon_color, Some("#5B45E0".to_string()));
    }

    #[test]
    fn test_create_from_preset_glm() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("claude", "glm", "glm-key", &path).unwrap();
        assert_eq!(result.name, "GLM");
        assert_eq!(result.preset_id, Some("glm".to_string()));
        assert_eq!(result.icon_color, Some("#3366FF".to_string()));
    }

    #[test]
    fn test_create_from_preset_invalid() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("claude", "nonexistent", "key", &path);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Flat preset registry tests (v2)
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_all_presets_flat_count() {
        let presets = get_all_presets_flat();
        assert_eq!(presets.len(), 12);
    }

    #[test]
    fn test_get_all_presets_flat_unique_ids() {
        let presets = get_all_presets_flat();
        let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
        let mut unique_ids = ids.clone();
        unique_ids.sort();
        unique_ids.dedup();
        assert_eq!(ids.len(), unique_ids.len(), "All preset IDs must be unique");
    }

    #[test]
    fn test_get_all_presets_flat_categories() {
        let presets = get_all_presets_flat();
        let domestic: Vec<_> = presets.iter().filter(|p| p.category == "domestic").collect();
        let relay: Vec<_> = presets.iter().filter(|p| p.category == "relay").collect();
        assert_eq!(domestic.len(), 10);
        assert_eq!(relay.len(), 2);
    }

    #[test]
    fn test_get_all_presets_flat_deepseek() {
        let presets = get_all_presets_flat();
        let ds = presets.iter().find(|p| p.id == "deepseek").unwrap();
        assert_eq!(ds.name, "DeepSeek");
        assert_eq!(ds.base_url_openai, "https://api.deepseek.com/v1");
        assert_eq!(ds.base_url_anthropic, "https://api.deepseek.com/anthropic");
        assert!(ds.models.is_empty());
        assert_eq!(ds.icon_color, "#4D6BFE");
        assert!(ds.balance_endpoint.is_some());
        assert!(ds.balance_parser.is_some());
    }

    #[test]
    fn test_get_all_presets_flat_kimi_coding() {
        let presets = get_all_presets_flat();
        let kc = presets.iter().find(|p| p.id == "kimi-coding").unwrap();
        assert_eq!(kc.name, "Kimi For Coding");
        assert_eq!(kc.base_url_openai, "https://api.kimi.com/coding/v1");
        assert_eq!(kc.base_url_anthropic, "https://api.kimi.com/coding/");
        assert!(kc.models.is_empty());
    }

    #[test]
    fn test_get_all_presets_flat_openrouter() {
        let presets = get_all_presets_flat();
        let or = presets.iter().find(|p| p.id == "openrouter").unwrap();
        assert_eq!(or.name, "OpenRouter");
        assert_eq!(or.category, "relay");
        assert_eq!(or.base_url_openai, "https://openrouter.ai/api/v1");
        assert!(or.base_url_anthropic.is_empty());
        assert!(or.models.is_empty());
        assert!(or.balance_endpoint.is_some());
    }

    #[test]
    fn test_get_all_presets_flat_siliconflow() {
        let presets = get_all_presets_flat();
        let sf = presets.iter().find(|p| p.id == "siliconflow").unwrap();
        assert_eq!(sf.name, "SiliconFlow");
        assert_eq!(sf.category, "relay");
        assert_eq!(sf.base_url_openai, "https://api.siliconflow.cn/v1");
        assert!(sf.base_url_anthropic.is_empty());
        assert!(sf.models.is_empty());
    }

    #[test]
    fn test_create_from_preset_flat_deepseek() {
        let result = create_from_preset_flat("deepseek", "sk-test-key-123").unwrap();
        assert_eq!(result.name, "DeepSeek");
        assert_eq!(result.base_url_openai, "https://api.deepseek.com/v1");
        assert_eq!(result.base_url_anthropic, "https://api.deepseek.com/anthropic");
        assert_eq!(result.api_key, "sk-test-key-123");
        assert!(result.models.is_empty());
        assert_eq!(result.default_model, "");
        assert_eq!(result.preset_id, Some("deepseek".to_string()));
        assert_eq!(result.icon_color, Some("#4D6BFE".to_string()));
        assert!(result.created_at.is_some());
        // ID should be a valid UUID
        assert!(uuid::Uuid::parse_str(&result.id).is_ok());
    }

    #[test]
    fn test_create_from_preset_flat_relay_empty_models() {
        let result = create_from_preset_flat("openrouter", "or-key").unwrap();
        assert_eq!(result.name, "OpenRouter");
        assert!(result.models.is_empty());
        assert_eq!(result.default_model, "");
        assert_eq!(result.base_url_openai, "https://openrouter.ai/api/v1");
        assert!(result.base_url_anthropic.is_empty());
    }

    #[test]
    fn test_create_from_preset_flat_invalid_preset_id() {
        let result = create_from_preset_flat("nonexistent-preset", "key");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_create_from_preset_flat_all_presets_succeed() {
        let presets = get_all_presets_flat();
        for preset in &presets {
            let result = create_from_preset_flat(&preset.id, "test-api-key");
            assert!(
                result.is_ok(),
                "Failed to create provider from preset '{}': {:?}",
                preset.id,
                result.err()
            );
            let entry = result.unwrap();
            assert_eq!(entry.name, preset.name);
            assert_eq!(entry.base_url_openai, preset.base_url_openai);
            assert_eq!(entry.base_url_anthropic, preset.base_url_anthropic);
            assert!(entry.models.is_empty());
            assert!(entry.default_model.is_empty());
            assert_eq!(entry.icon_color, Some(preset.icon_color.clone()));
            assert_eq!(entry.preset_id, Some(preset.id.clone()));
            assert!(entry.created_at.is_some());
            assert!(uuid::Uuid::parse_str(&entry.id).is_ok());
        }
    }

    // -----------------------------------------------------------------------
    // Flat store CRUD tests (v2)
    // -----------------------------------------------------------------------

    fn make_flat_entry(name: &str) -> ProviderEntryFlat {
        ProviderEntryFlat {
            id: String::new(), // Will be overwritten by create
            name: name.to_string(),
            base_url_openai: "https://api.example.com/v1".to_string(),
            base_url_anthropic: "https://api.example.com/anthropic".to_string(),
            models_url: "https://api.example.com/v1/models".to_string(),
            api_key: "sk-test-key".to_string(),
            models: vec!["model-a".to_string()],
            default_model: "model-a".to_string(),
            sort_index: 0,
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: None,
            meta: None,
        }
    }

    #[test]
    fn test_create_provider_flat_basic() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("My Provider");
        let result = create_provider_flat(&mut store, entry).unwrap();

        assert_eq!(result.name, "My Provider");
        assert!(uuid::Uuid::parse_str(&result.id).is_ok());
        assert!(result.created_at.is_some());
        assert_eq!(result.sort_index, 0);
        assert_eq!(store.providers.len(), 1);
        assert_eq!(store.providers[0].id, result.id);
    }

    #[test]
    fn test_create_provider_flat_empty_name() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("");
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name must not be empty"));
    }

    #[test]
    fn test_create_provider_flat_whitespace_name() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("   ");
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name must not be empty"));
    }

    #[test]
    fn test_create_provider_flat_invalid_url() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.base_url_openai = "not-a-url".to_string();
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid URL"));
    }

    #[test]
    fn test_create_provider_flat_invalid_scheme() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.base_url_openai = "ftp://api.example.com/v1".to_string();
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("http or https"));
    }

    #[test]
    fn test_create_provider_flat_empty_url_allowed() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.base_url_anthropic = String::new();
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_flat_generates_uuid() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.id = "user-provided-id".to_string();
        let result = create_provider_flat(&mut store, entry).unwrap();
        // ID should be overwritten with a valid UUID
        assert_ne!(result.id, "user-provided-id");
        assert!(uuid::Uuid::parse_str(&result.id).is_ok());
    }

    #[test]
    fn test_create_provider_flat_sets_created_at() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Test");
        let result = create_provider_flat(&mut store, entry).unwrap();
        assert!(result.created_at.is_some());
        assert!(result.created_at.unwrap() > 0);
    }

    #[test]
    fn test_create_provider_flat_preserves_existing_created_at() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.created_at = Some(1719000000000);
        let result = create_provider_flat(&mut store, entry).unwrap();
        assert_eq!(result.created_at, Some(1719000000000));
    }

    #[test]
    fn test_create_provider_flat_sort_index_increments() {
        let mut store = FlatProvidersStore::default();

        let entry1 = make_flat_entry("First");
        let result1 = create_provider_flat(&mut store, entry1).unwrap();
        assert_eq!(result1.sort_index, 0);

        let entry2 = make_flat_entry("Second");
        let result2 = create_provider_flat(&mut store, entry2).unwrap();
        assert_eq!(result2.sort_index, 1);

        let entry3 = make_flat_entry("Third");
        let result3 = create_provider_flat(&mut store, entry3).unwrap();
        assert_eq!(result3.sort_index, 2);
    }

    #[test]
    fn test_update_provider_flat_basic() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Original");
        let created = create_provider_flat(&mut store, entry).unwrap();

        let patch = ProviderPatchFlat {
            name: Some("Updated".to_string()),
            ..Default::default()
        };
        let updated = update_provider_flat(&mut store, &created.id, patch).unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.id, created.id);
        // Other fields unchanged
        assert_eq!(updated.base_url_openai, "https://api.example.com/v1");
    }

    #[test]
    fn test_update_provider_flat_multiple_fields() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Original");
        let created = create_provider_flat(&mut store, entry).unwrap();

        let patch = ProviderPatchFlat {
            name: Some("New Name".to_string()),
            base_url_openai: Some("https://new-api.com/v1".to_string()),
            api_key: Some("new-key".to_string()),
            models: Some(vec!["new-model".to_string()]),
            default_model: Some("new-model".to_string()),
            icon_color: Some("#FF0000".to_string()),
            notes: Some("Some notes".to_string()),
            ..Default::default()
        };
        let updated = update_provider_flat(&mut store, &created.id, patch).unwrap();
        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.base_url_openai, "https://new-api.com/v1");
        assert_eq!(updated.api_key, "new-key");
        assert_eq!(updated.models, vec!["new-model"]);
        assert_eq!(updated.default_model, "new-model");
        assert_eq!(updated.icon_color, Some("#FF0000".to_string()));
        assert_eq!(updated.notes, Some("Some notes".to_string()));
    }

    #[test]
    fn test_update_provider_flat_not_found() {
        let mut store = FlatProvidersStore::default();
        let patch = ProviderPatchFlat {
            name: Some("New".to_string()),
            ..Default::default()
        };
        let result = update_provider_flat(&mut store, "nonexistent", patch);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_delete_provider_flat_basic() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("To Delete");
        let created = create_provider_flat(&mut store, entry).unwrap();
        assert_eq!(store.providers.len(), 1);

        delete_provider_flat(&mut store, &created.id).unwrap();
        assert_eq!(store.providers.len(), 0);
    }

    #[test]
    fn test_delete_provider_flat_not_found() {
        let mut store = FlatProvidersStore::default();
        let result = delete_provider_flat(&mut store, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_delete_provider_flat_cleans_tool_activations() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Active Provider");
        let created = create_provider_flat(&mut store, entry).unwrap();

        // Set up tool_activations referencing this provider
        store.tool_activations.insert(
            "claude-code".to_string(),
            Some(ToolActivation {
                provider_id: created.id.clone(),
                model: "model-a".to_string(),
            }),
        );
        store.tool_activations.insert(
            "codex".to_string(),
            Some(ToolActivation {
                provider_id: created.id.clone(),
                model: "model-a".to_string(),
            }),
        );

        delete_provider_flat(&mut store, &created.id).unwrap();

        // Both activations should be cleared
        assert_eq!(store.tool_activations.get("claude-code").unwrap(), &None);
        assert_eq!(store.tool_activations.get("codex").unwrap(), &None);
    }

    #[test]
    fn test_delete_provider_flat_preserves_other_activations() {
        let mut store = FlatProvidersStore::default();
        let entry1 = make_flat_entry("Provider 1");
        let entry2 = make_flat_entry("Provider 2");
        let created1 = create_provider_flat(&mut store, entry1).unwrap();
        let created2 = create_provider_flat(&mut store, entry2).unwrap();

        // Set up tool_activations: claude-code → provider1, codex → provider2
        store.tool_activations.insert(
            "claude-code".to_string(),
            Some(ToolActivation {
                provider_id: created1.id.clone(),
                model: "model-a".to_string(),
            }),
        );
        store.tool_activations.insert(
            "codex".to_string(),
            Some(ToolActivation {
                provider_id: created2.id.clone(),
                model: "model-a".to_string(),
            }),
        );

        // Delete provider1 — only claude-code should be cleared
        delete_provider_flat(&mut store, &created1.id).unwrap();

        assert_eq!(store.tool_activations.get("claude-code").unwrap(), &None);
        let codex_act = store.tool_activations.get("codex").unwrap().as_ref().unwrap();
        assert_eq!(codex_act.provider_id, created2.id);
    }

    #[test]
    fn test_reorder_providers_basic() {
        let mut store = FlatProvidersStore::default();
        let entry1 = make_flat_entry("First");
        let entry2 = make_flat_entry("Second");
        let entry3 = make_flat_entry("Third");
        let created1 = create_provider_flat(&mut store, entry1).unwrap();
        let created2 = create_provider_flat(&mut store, entry2).unwrap();
        let created3 = create_provider_flat(&mut store, entry3).unwrap();

        // Reorder: Third, First, Second
        let ordered_ids = vec![
            created3.id.clone(),
            created1.id.clone(),
            created2.id.clone(),
        ];
        reorder_providers(&mut store, &ordered_ids).unwrap();

        // Verify sort_index assignments
        let p1 = store.providers.iter().find(|p| p.id == created1.id).unwrap();
        let p2 = store.providers.iter().find(|p| p.id == created2.id).unwrap();
        let p3 = store.providers.iter().find(|p| p.id == created3.id).unwrap();
        assert_eq!(p3.sort_index, 0);
        assert_eq!(p1.sort_index, 1);
        assert_eq!(p2.sort_index, 2);
    }

    #[test]
    fn test_reorder_providers_partial() {
        let mut store = FlatProvidersStore::default();
        let entry1 = make_flat_entry("First");
        let entry2 = make_flat_entry("Second");
        let entry3 = make_flat_entry("Third");
        let created1 = create_provider_flat(&mut store, entry1).unwrap();
        let created2 = create_provider_flat(&mut store, entry2).unwrap();
        let created3 = create_provider_flat(&mut store, entry3).unwrap();

        // Only reorder two of three — Third keeps its existing sort_index
        let ordered_ids = vec![created2.id.clone(), created1.id.clone()];
        reorder_providers(&mut store, &ordered_ids).unwrap();

        let p1 = store.providers.iter().find(|p| p.id == created1.id).unwrap();
        let p2 = store.providers.iter().find(|p| p.id == created2.id).unwrap();
        let p3 = store.providers.iter().find(|p| p.id == created3.id).unwrap();
        assert_eq!(p2.sort_index, 0);
        assert_eq!(p1.sort_index, 1);
        // Third keeps its original sort_index (2)
        assert_eq!(p3.sort_index, 2);
    }

    #[test]
    fn test_reorder_providers_invalid_id() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Test");
        create_provider_flat(&mut store, entry).unwrap();

        let ordered_ids = vec!["nonexistent-id".to_string()];
        let result = reorder_providers(&mut store, &ordered_ids);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_reorder_providers_empty_list() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Test");
        let created = create_provider_flat(&mut store, entry).unwrap();

        // Empty reorder list — no changes
        reorder_providers(&mut store, &[]).unwrap();

        let p = store.providers.iter().find(|p| p.id == created.id).unwrap();
        assert_eq!(p.sort_index, 0); // Unchanged
    }

    #[test]
    fn test_app_id_isolation() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Claude Provider");
        create_provider_at("claude", entry, &path).unwrap();

        // Codex should be unaffected
        let store = read_store_from(&path).unwrap();
        assert!(store.codex.providers.is_empty());
        assert_eq!(store.codex.current, None);
    }

    #[test]
    fn test_app_id_isolation_bidirectional() {
        let (_tmp, path) = setup_temp_store();
        let entry1 = make_valid_entry("p1", "Claude Provider");
        let entry2 = make_valid_entry("p2", "Codex Provider");
        create_provider_at("claude", entry1, &path).unwrap();
        create_provider_at("codex", entry2, &path).unwrap();

        // Delete from claude should not affect codex
        delete_provider_at("claude", "p1", &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert!(store.claude.providers.is_empty());
        assert_eq!(store.codex.providers.len(), 1);
        assert_eq!(store.codex.current, Some("p2".to_string()));
    }

    #[test]
    fn test_unknown_app_id() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Test");
        let result = create_provider_at("unknown_app", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown app_id"));
    }

    // -----------------------------------------------------------------------
    // Property 14: Concurrent Write Serialization
    //
    // Spawn multiple concurrent create_provider calls, assert final store is
    // consistent with no corruption.
    //
    // **Validates: Requirement 7.2**
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn prop_concurrent_write_serialization() {
        use std::sync::Arc;

        let (_tmp, path) = setup_temp_store();
        let path = Arc::new(path);
        let num_tasks = 10;

        // Spawn multiple concurrent create_provider_at calls with unique IDs
        let mut handles = Vec::new();
        for i in 0..num_tasks {
            let p = Arc::clone(&path);
            handles.push(tokio::spawn(async move {
                let id = format!("concurrent-provider-{}", i);
                let name = format!("Provider {}", i);
                let entry = ProviderEntry {
                    id: id.clone(),
                    name,
                    category: "cloud".to_string(),
                    settings_config: serde_json::to_value(ProviderSettings {
                        base_url: "https://api.example.com/v1".to_string(),
                        api_key: format!("sk-key-{}", i),
                        models: vec![ModelMapping {
                            source_model: format!("model-{}", i),
                            target_model: format!("model-{}", i),
                            enabled: true,
                        }],
                        timeout_ms: None,
                        max_retries: None,
                    })
                    .unwrap(),
                    preset_id: None,
                    website_url: None,
                    api_key_url: None,
                    icon_color: None,
                    notes: None,
                    created_at: None,
                    sort_index: None,
                    meta: None,
                };
                let result = create_provider_at("claude", entry, &p);
                (id, result.is_ok())
            }));
        }

        // Collect results
        let mut successful_ids: Vec<String> = Vec::new();
        for handle in handles {
            let (id, ok) = handle.await.unwrap();
            if ok {
                successful_ids.push(id);
            }
        }

        // Assertion 1: The store file is valid JSON (no corruption)
        let raw_content = std::fs::read_to_string(path.as_ref())
            .expect("Store file should exist after concurrent writes");
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&raw_content);
        assert!(
            parsed.is_ok(),
            "Store file must be valid JSON after concurrent writes, but got parse error: {:?}",
            parsed.err()
        );

        // Assertion 2: The store deserializes to a valid ProvidersStore
        let store = read_store_from(path.as_ref())
            .expect("Store should be readable after concurrent writes");

        // Assertion 3: All successfully created providers are present in the store
        for id in &successful_ids {
            assert!(
                store.claude.providers.contains_key(id),
                "Successfully created provider '{}' should be present in the store",
                id
            );
        }

        // Assertion 4: The store is internally consistent
        // If current is set, it must reference a valid provider
        if let Some(ref current_id) = store.claude.current {
            assert!(
                store.claude.providers.contains_key(current_id),
                "current '{}' must reference an existing provider. Existing: {:?}",
                current_id,
                store.claude.providers.keys().collect::<Vec<_>>()
            );
        }

        // Assertion 5: At least one provider was created successfully
        // (demonstrates the race condition may cause some to fail, but not all)
        assert!(
            !successful_ids.is_empty(),
            "At least one concurrent create should succeed"
        );

        // Note: Without external locking (like the Tauri Mutex), some creates may
        // fail due to read-modify-write races. The key property is that the final
        // state is still valid JSON with no corruption, even if not all creates
        // succeeded. This demonstrates why the Mutex is needed at the command layer.
    }

    // -----------------------------------------------------------------------
    // Migration tests (v1 → v2)
    // -----------------------------------------------------------------------

    #[test]
    fn test_migrate_store_if_needed_file_not_found() {
        let (_tmp, path) = setup_temp_store();
        let store = migrate_store_if_needed(&path).unwrap();
        assert_eq!(store.version, 2);
        assert!(store.providers.is_empty());
        assert!(store.tool_activations.is_empty());
    }

    #[test]
    fn test_migrate_store_if_needed_already_v2() {
        let (_tmp, path) = setup_temp_store();
        let original = FlatProvidersStore {
            version: 2,
            providers: vec![ProviderEntryFlat {
                id: "existing-id".to_string(),
                name: "Existing Provider".to_string(),
                base_url_openai: "https://api.example.com/v1".to_string(),
                base_url_anthropic: String::new(),
                models_url: String::new(),
                api_key: "sk-key".to_string(),
                models: vec!["model-a".to_string()],
                default_model: "model-a".to_string(),
                sort_index: 0,
                preset_id: None,
                icon_color: None,
                notes: None,
                created_at: Some(1719000000000),
                meta: None,
            }],
            tool_activations: {
                let mut map = HashMap::new();
                map.insert(
                    "claude-code".to_string(),
                    Some(ToolActivation {
                        provider_id: "existing-id".to_string(),
                        model: "model-a".to_string(),
                    }),
                );
                map
            },
        };
        write_flat_store(&original, &path).unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        assert_eq!(result.version, 2);
        assert_eq!(result.providers.len(), 1);
        assert_eq!(result.providers[0].id, "existing-id");
        assert_eq!(result.providers[0].name, "Existing Provider");
        assert_eq!(
            result.tool_activations.get("claude-code").unwrap().as_ref().unwrap().provider_id,
            "existing-id"
        );
    }

    #[test]
    fn test_migrate_store_if_needed_v1_basic() {
        let (_tmp, path) = setup_temp_store();

        // Write a v1 store
        let mut store = ProvidersStore::default();
        let entry = make_valid_entry("p1", "DeepSeek");
        store.claude.providers.insert("p1".to_string(), entry);
        store.claude.current = Some("p1".to_string());
        write_store_to(&store, &path).unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        assert_eq!(result.version, 2);
        assert_eq!(result.providers.len(), 1);
        assert_eq!(result.providers[0].name, "DeepSeek");
        assert_eq!(result.providers[0].base_url_openai, "https://api.example.com/v1");
        assert_eq!(result.providers[0].api_key, "sk-test-key-12345");
        assert_eq!(result.providers[0].models, vec!["model-a"]);

        // tool_activations should map claude → claude-code
        let claude_activation = result.tool_activations.get("claude-code");
        assert!(claude_activation.is_some());
        let activation = claude_activation.unwrap().as_ref().unwrap();
        assert_eq!(activation.provider_id, result.providers[0].id);
        assert_eq!(activation.model, "model-a");
    }

    #[test]
    fn test_migrate_store_if_needed_v1_deduplication() {
        let (_tmp, path) = setup_temp_store();

        // Create a v1 store with the same provider in both claude and codex
        let mut store = ProvidersStore::default();
        let entry_claude = make_valid_entry("p1", "Shared Provider");
        let entry_codex = make_valid_entry("p2", "Shared Provider");
        // Both have the same base_url and api_key (from make_valid_entry)
        store.claude.providers.insert("p1".to_string(), entry_claude);
        store.claude.current = Some("p1".to_string());
        store.codex.providers.insert("p2".to_string(), entry_codex);
        store.codex.current = Some("p2".to_string());
        write_store_to(&store, &path).unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        // Should be deduplicated to 1 provider (same base_url + api_key)
        assert_eq!(result.providers.len(), 1);

        // Both tool_activations should point to the same provider
        let claude_act = result.tool_activations.get("claude-code")
            .unwrap().as_ref().unwrap();
        let codex_act = result.tool_activations.get("codex")
            .unwrap().as_ref().unwrap();
        assert_eq!(claude_act.provider_id, codex_act.provider_id);
    }

    #[test]
    fn test_migrate_store_if_needed_v1_different_providers() {
        let (_tmp, path) = setup_temp_store();

        // Create a v1 store with different providers in claude and codex
        let mut store = ProvidersStore::default();

        let settings_claude = ProviderSettings {
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-deepseek".to_string(),
            models: vec![ModelMapping {
                source_model: "deepseek-chat".to_string(),
                target_model: "deepseek-chat".to_string(),
                enabled: true,
            }],
            timeout_ms: None,
            max_retries: None,
        };
        let entry_claude = ProviderEntry {
            id: "p1".to_string(),
            name: "DeepSeek".to_string(),
            category: "cloud".to_string(),
            settings_config: serde_json::to_value(&settings_claude).unwrap(),
            preset_id: Some("deepseek".to_string()),
            website_url: None,
            api_key_url: None,
            icon_color: Some("#4D6BFE".to_string()),
            notes: None,
            created_at: Some(1719000000000),
            sort_index: Some(0),
            meta: None,
        };

        let settings_codex = ProviderSettings {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-openai".to_string(),
            models: vec![ModelMapping {
                source_model: "gpt-4".to_string(),
                target_model: "gpt-4".to_string(),
                enabled: true,
            }],
            timeout_ms: None,
            max_retries: None,
        };
        let entry_codex = ProviderEntry {
            id: "p2".to_string(),
            name: "OpenAI".to_string(),
            category: "cloud".to_string(),
            settings_config: serde_json::to_value(&settings_codex).unwrap(),
            preset_id: Some("official".to_string()),
            website_url: None,
            api_key_url: None,
            icon_color: Some("#10A37F".to_string()),
            notes: None,
            created_at: Some(1719000000000),
            sort_index: Some(0),
            meta: None,
        };

        store.claude.providers.insert("p1".to_string(), entry_claude);
        store.claude.current = Some("p1".to_string());
        store.codex.providers.insert("p2".to_string(), entry_codex);
        store.codex.current = Some("p2".to_string());
        write_store_to(&store, &path).unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        // Should have 2 distinct providers (different base_url + api_key)
        assert_eq!(result.providers.len(), 2);

        // Verify tool_activations point to different providers
        let claude_act = result.tool_activations.get("claude-code")
            .unwrap().as_ref().unwrap();
        let codex_act = result.tool_activations.get("codex")
            .unwrap().as_ref().unwrap();
        assert_ne!(claude_act.provider_id, codex_act.provider_id);

        // Verify the correct models
        assert_eq!(claude_act.model, "deepseek-chat");
        assert_eq!(codex_act.model, "gpt-4");
    }

    #[test]
    fn test_migrate_store_if_needed_creates_backup() {
        let (_tmp, path) = setup_temp_store();

        // Write a v1 store
        let mut store = ProvidersStore::default();
        let entry = make_valid_entry("p1", "Test");
        store.claude.providers.insert("p1".to_string(), entry);
        write_store_to(&store, &path).unwrap();

        // Read original content for comparison
        let original_content = std::fs::read_to_string(&path).unwrap();

        // Migrate
        migrate_store_if_needed(&path).unwrap();

        // Verify backup was created
        let backup_path = path.with_extension("json.bak");
        assert!(backup_path.exists(), "Backup file should be created");

        // Verify backup content matches original
        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, original_content);
    }

    #[test]
    fn test_migrate_store_if_needed_malformed_json() {
        let (_tmp, path) = setup_temp_store();
        std::fs::write(&path, "not valid json {{{").unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        assert_eq!(result.version, 2);
        assert!(result.providers.is_empty());
    }

    #[test]
    fn test_migrate_store_if_needed_model_merging() {
        let (_tmp, path) = setup_temp_store();

        // Create a v1 store where the same provider (same base_url + api_key)
        // appears in both apps but with different models
        let mut store = ProvidersStore::default();

        let settings1 = ProviderSettings {
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-shared".to_string(),
            models: vec![ModelMapping {
                source_model: "deepseek-chat".to_string(),
                target_model: "deepseek-chat".to_string(),
                enabled: true,
            }],
            timeout_ms: None,
            max_retries: None,
        };
        let entry1 = ProviderEntry {
            id: "p1".to_string(),
            name: "DeepSeek (Claude)".to_string(),
            category: "cloud".to_string(),
            settings_config: serde_json::to_value(&settings1).unwrap(),
            preset_id: None,
            website_url: None,
            api_key_url: None,
            icon_color: None,
            notes: None,
            created_at: None,
            sort_index: None,
            meta: None,
        };

        let settings2 = ProviderSettings {
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: "sk-shared".to_string(),
            models: vec![
                ModelMapping {
                    source_model: "deepseek-chat".to_string(),
                    target_model: "deepseek-chat".to_string(),
                    enabled: true,
                },
                ModelMapping {
                    source_model: "deepseek-reasoner".to_string(),
                    target_model: "deepseek-reasoner".to_string(),
                    enabled: true,
                },
            ],
            timeout_ms: None,
            max_retries: None,
        };
        let entry2 = ProviderEntry {
            id: "p2".to_string(),
            name: "DeepSeek (Codex)".to_string(),
            category: "cloud".to_string(),
            settings_config: serde_json::to_value(&settings2).unwrap(),
            preset_id: None,
            website_url: None,
            api_key_url: None,
            icon_color: None,
            notes: None,
            created_at: None,
            sort_index: None,
            meta: None,
        };

        store.claude.providers.insert("p1".to_string(), entry1);
        store.codex.providers.insert("p2".to_string(), entry2);
        write_store_to(&store, &path).unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        // Should be deduplicated to 1 provider
        assert_eq!(result.providers.len(), 1);
        // Models should be merged (deepseek-chat + deepseek-reasoner)
        assert!(result.providers[0].models.contains(&"deepseek-chat".to_string()));
        assert!(result.providers[0].models.contains(&"deepseek-reasoner".to_string()));
    }

    #[test]
    fn test_migrate_store_if_needed_no_current() {
        let (_tmp, path) = setup_temp_store();

        // Write a v1 store with no current set
        let mut store = ProvidersStore::default();
        let entry = make_valid_entry("p1", "Test");
        store.claude.providers.insert("p1".to_string(), entry);
        // current is None
        write_store_to(&store, &path).unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        assert_eq!(result.providers.len(), 1);
        // No tool_activations should be set for claude-code
        let claude_act = result.tool_activations.get("claude-code");
        assert!(claude_act.is_none() || claude_act.unwrap().is_none());
    }

    // -----------------------------------------------------------------------
    // Property-based tests
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Property-based test strategies
    // -----------------------------------------------------------------------

    fn arb_provider_name() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 _-]{1,64}"
    }

    fn arb_app_id() -> impl Strategy<Value = String> {
        prop_oneof![Just("claude".to_string()), Just("codex".to_string())]
    }

    fn arb_provider_count() -> impl Strategy<Value = usize> {
        1usize..=5
    }

    // -----------------------------------------------------------------------
    // Property 7: Active Provider Validity Invariant
    //
    // For any store state, if `current` is not null, it must reference an
    // existing provider ID. Delete the active provider, assert `current`
    // becomes null.
    //
    // **Validates: Requirements 2.8, 4.1**
    // -----------------------------------------------------------------------
    proptest! {
        #[test]
        fn prop_active_provider_validity_invariant(
            app_id in arb_app_id(),
            count in arb_provider_count(),
            names in prop::collection::vec(arb_provider_name(), 1..=5),
        ) {
            let (_tmp, path) = setup_temp_store();

            // Create `count` providers (capped by available names)
            let actual_count = count.min(names.len());
            let mut created_ids: Vec<String> = Vec::new();

            for i in 0..actual_count {
                let id = format!("provider-{}", i);
                let entry = make_valid_entry(&id, &names[i]);
                let result = create_provider_at(&app_id, entry, &path);
                prop_assert!(result.is_ok(), "Failed to create provider {}: {:?}", i, result.err());
                created_ids.push(id);
            }

            // Read the store and verify the invariant:
            // If current is Some, it must reference an existing provider ID
            let store = read_store_from(&path).unwrap();
            let app = get_app(&store, &app_id);

            if let Some(ref current_id) = app.current {
                prop_assert!(
                    app.providers.contains_key(current_id),
                    "current '{}' does not reference an existing provider. Existing IDs: {:?}",
                    current_id,
                    app.providers.keys().collect::<Vec<_>>()
                );
            }

            // The first provider should have been auto-activated
            prop_assert_eq!(app.current.as_deref(), Some(created_ids[0].as_str()));

            // Now delete the active provider
            let active_id = app.current.clone().unwrap();
            let delete_result = delete_provider_at(&app_id, &active_id, &path);
            prop_assert!(delete_result.is_ok(), "Failed to delete active provider: {:?}", delete_result.err());

            // After deleting the active provider, current must become None
            let store_after = read_store_from(&path).unwrap();
            let app_after = get_app(&store_after, &app_id);
            prop_assert_eq!(
                app_after.current.clone(), None,
                "current should be None after deleting the active provider, but got {:?}",
                app_after.current
            );

            // Verify the invariant still holds: if current is Some, it references an existing ID
            // (In this case current is None, so the invariant trivially holds)
            if let Some(ref current_id) = app_after.current {
                prop_assert!(
                    app_after.providers.contains_key(current_id),
                    "After deletion, current '{}' does not reference an existing provider",
                    current_id
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Tool activation/deactivation tests (v2)
    // -----------------------------------------------------------------------

    #[test]
    fn test_activate_tool_success_claude_code() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("DeepSeek");
        let created = create_provider_flat(&mut store, entry).unwrap();

        let activation = activate_tool(&mut store, &created.id, "claude-code", Some("deepseek-chat")).unwrap();
        assert_eq!(activation.provider_id, created.id);
        assert_eq!(activation.model, "deepseek-chat");

        // Verify it's in the store
        let stored = store.tool_activations.get("claude-code").unwrap().as_ref().unwrap();
        assert_eq!(stored.provider_id, created.id);
        assert_eq!(stored.model, "deepseek-chat");
    }

    #[test]
    fn test_activate_tool_success_codex() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("DeepSeek");
        let created = create_provider_flat(&mut store, entry).unwrap();

        let activation = activate_tool(&mut store, &created.id, "codex", Some("deepseek-reasoner")).unwrap();
        assert_eq!(activation.provider_id, created.id);
        assert_eq!(activation.model, "deepseek-reasoner");

        let stored = store.tool_activations.get("codex").unwrap().as_ref().unwrap();
        assert_eq!(stored.provider_id, created.id);
        assert_eq!(stored.model, "deepseek-reasoner");
    }

    #[test]
    fn test_activate_tool_falls_back_to_default_model() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("DeepSeek");
        entry.default_model = "deepseek-chat".to_string();
        let created = create_provider_flat(&mut store, entry).unwrap();

        // No model provided — should use default_model
        let activation = activate_tool(&mut store, &created.id, "codex", None).unwrap();
        assert_eq!(activation.model, "deepseek-chat");
    }

    #[test]
    fn test_activate_tool_empty_model_falls_back_to_default() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("DeepSeek");
        entry.default_model = "deepseek-chat".to_string();
        let created = create_provider_flat(&mut store, entry).unwrap();

        // Empty string model — should use default_model
        let activation = activate_tool(&mut store, &created.id, "codex", Some("")).unwrap();
        assert_eq!(activation.model, "deepseek-chat");
    }

    #[test]
    fn test_activate_tool_whitespace_model_falls_back_to_default() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("DeepSeek");
        entry.default_model = "deepseek-chat".to_string();
        let created = create_provider_flat(&mut store, entry).unwrap();

        let activation = activate_tool(&mut store, &created.id, "codex", Some("   ")).unwrap();
        assert_eq!(activation.model, "deepseek-chat");
    }

    #[test]
    fn test_activate_tool_provider_not_found() {
        let mut store = FlatProvidersStore::default();
        let result = activate_tool(&mut store, "nonexistent-id", "claude-code", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_activate_tool_claude_code_empty_anthropic_url() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("OpenRouter");
        entry.base_url_anthropic = String::new(); // No Anthropic endpoint
        let created = create_provider_flat(&mut store, entry).unwrap();

        let result = activate_tool(&mut store, &created.id, "claude-code", None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Anthropic-compatible endpoint"));
        assert!(err_msg.contains("base_url_anthropic is empty"));
    }

    #[test]
    fn test_activate_tool_claude_code_whitespace_anthropic_url() {
        let mut store = FlatProvidersStore::default();
        // Directly insert a provider with whitespace-only anthropic URL
        // (bypassing create_provider_flat validation to test activate_tool's own validation)
        store.providers.push(ProviderEntryFlat {
            id: "test-id".to_string(),
            name: "Test".to_string(),
            base_url_openai: "https://api.example.com/v1".to_string(),
            base_url_anthropic: "   ".to_string(),
            models_url: String::new(),
            api_key: "sk-key".to_string(),
            models: vec!["model-a".to_string()],
            default_model: "model-a".to_string(),
            sort_index: 0,
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: None,
            meta: None,
        });

        let result = activate_tool(&mut store, "test-id", "claude-code", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Anthropic-compatible endpoint"));
    }

    #[test]
    fn test_activate_tool_codex_empty_openai_url() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.base_url_openai = String::new(); // No OpenAI endpoint
        entry.base_url_anthropic = "https://api.example.com/anthropic".to_string();
        let created = create_provider_flat(&mut store, entry).unwrap();

        let result = activate_tool(&mut store, &created.id, "codex", None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("OpenAI-compatible endpoint"));
        assert!(err_msg.contains("base_url_openai is empty"));
    }

    #[test]
    fn test_activate_tool_other_tool_empty_openai_url() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.base_url_openai = String::new();
        entry.base_url_anthropic = "https://api.example.com/anthropic".to_string();
        let created = create_provider_flat(&mut store, entry).unwrap();

        // Unknown tool defaults to requiring base_url_openai
        let result = activate_tool(&mut store, &created.id, "some-other-tool", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("OpenAI-compatible endpoint"));
    }

    #[test]
    fn test_activate_tool_replaces_previous_activation() {
        let mut store = FlatProvidersStore::default();
        let entry1 = make_flat_entry("Provider A");
        let entry2 = make_flat_entry("Provider B");
        let created1 = create_provider_flat(&mut store, entry1).unwrap();
        let created2 = create_provider_flat(&mut store, entry2).unwrap();

        // Activate provider A for claude-code
        activate_tool(&mut store, &created1.id, "claude-code", Some("model-a")).unwrap();
        let stored = store.tool_activations.get("claude-code").unwrap().as_ref().unwrap();
        assert_eq!(stored.provider_id, created1.id);

        // Activate provider B for claude-code — should replace A
        activate_tool(&mut store, &created2.id, "claude-code", Some("model-b")).unwrap();
        let stored = store.tool_activations.get("claude-code").unwrap().as_ref().unwrap();
        assert_eq!(stored.provider_id, created2.id);
        assert_eq!(stored.model, "model-b");

        // Only one activation for claude-code
        let activations_for_claude: Vec<_> = store
            .tool_activations
            .iter()
            .filter(|(k, v)| *k == "claude-code" && v.is_some())
            .collect();
        assert_eq!(activations_for_claude.len(), 1);
    }

    #[test]
    fn test_deactivate_tool_returns_previous() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("DeepSeek");
        let created = create_provider_flat(&mut store, entry).unwrap();

        // Activate first
        activate_tool(&mut store, &created.id, "claude-code", Some("deepseek-chat")).unwrap();

        // Deactivate — should return the previous activation
        let previous = deactivate_tool(&mut store, "claude-code").unwrap();
        assert!(previous.is_some());
        let prev = previous.unwrap();
        assert_eq!(prev.provider_id, created.id);
        assert_eq!(prev.model, "deepseek-chat");

        // Verify it's now None in the store
        let stored = store.tool_activations.get("claude-code").unwrap();
        assert!(stored.is_none());
    }

    #[test]
    fn test_deactivate_tool_no_previous_activation() {
        let mut store = FlatProvidersStore::default();

        // Deactivate a tool that was never activated
        let previous = deactivate_tool(&mut store, "claude-code").unwrap();
        assert!(previous.is_none());

        // The entry should now exist as None
        let stored = store.tool_activations.get("claude-code").unwrap();
        assert!(stored.is_none());
    }

    #[test]
    fn test_deactivate_tool_already_none() {
        let mut store = FlatProvidersStore::default();
        store.tool_activations.insert("codex".to_string(), None);

        // Deactivate a tool that's already None
        let previous = deactivate_tool(&mut store, "codex").unwrap();
        assert!(previous.is_none());
    }

    #[test]
    fn test_activate_deactivate_round_trip() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Provider");
        let created = create_provider_flat(&mut store, entry).unwrap();

        // Activate
        let activation = activate_tool(&mut store, &created.id, "codex", Some("model-x")).unwrap();
        assert_eq!(activation.provider_id, created.id);
        assert_eq!(activation.model, "model-x");

        // Deactivate
        let previous = deactivate_tool(&mut store, "codex").unwrap();
        assert_eq!(previous.unwrap().provider_id, created.id);

        // Verify tool is deactivated
        let stored = store.tool_activations.get("codex").unwrap();
        assert!(stored.is_none());
    }

    #[test]
    fn test_activate_multiple_tools_same_provider() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("DeepSeek");
        let created = create_provider_flat(&mut store, entry).unwrap();

        // Activate same provider for both tools
        activate_tool(&mut store, &created.id, "claude-code", Some("deepseek-chat")).unwrap();
        activate_tool(&mut store, &created.id, "codex", Some("deepseek-reasoner")).unwrap();

        // Both should be active
        let claude = store.tool_activations.get("claude-code").unwrap().as_ref().unwrap();
        assert_eq!(claude.provider_id, created.id);
        assert_eq!(claude.model, "deepseek-chat");

        let codex = store.tool_activations.get("codex").unwrap().as_ref().unwrap();
        assert_eq!(codex.provider_id, created.id);
        assert_eq!(codex.model, "deepseek-reasoner");
    }
}

// ---------------------------------------------------------------------------
// Property-based tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    /// **Validates: Requirements 2.6, 2.7**

    /// Strategy: generate a valid provider name (1..=64 non-empty ASCII chars).
    fn valid_name_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 _-]{1,64}".prop_filter("name must not be empty", |s| !s.is_empty())
    }

    /// Strategy: generate a valid base URL.
    fn valid_base_url_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("https://api.example.com/v1".to_string()),
            Just("https://api.deepseek.com/v1".to_string()),
            Just("https://api.openai.com/v1".to_string()),
            Just("https://custom.provider.io/api".to_string()),
            Just("http://localhost:8080".to_string()),
        ]
    }

    /// Strategy: generate a valid category.
    fn valid_category_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("cloud".to_string()),
            Just("local".to_string()),
            Just("proxy".to_string()),
        ]
    }

    /// Strategy: generate a valid model mapping list (at least one entry).
    fn valid_models_strategy() -> impl Strategy<Value = Vec<ModelMapping>> {
        prop::collection::vec(
            "[a-z0-9-]{1,30}".prop_map(|s| ModelMapping {
                source_model: s.clone(),
                target_model: s,
                enabled: true,
            }),
            1..=5,
        )
    }

    /// Strategy: generate a valid ProviderSettings value.
    fn valid_settings_strategy() -> impl Strategy<Value = Value> {
        (valid_base_url_strategy(), valid_models_strategy()).prop_map(|(base_url, models)| {
            serde_json::to_value(ProviderSettings {
                base_url,
                api_key: "sk-test-key-12345".to_string(),
                models,
                timeout_ms: None,
                max_retries: None,
            })
            .unwrap()
        })
    }

    /// Strategy: generate a valid ProviderEntry with a unique ID.
    fn valid_entry_strategy() -> impl Strategy<Value = ProviderEntry> {
        (
            valid_name_strategy(),
            valid_category_strategy(),
            valid_settings_strategy(),
        )
            .prop_map(|(name, category, settings_config)| ProviderEntry {
                id: Uuid::new_v4().to_string(),
                name,
                category,
                settings_config,
                preset_id: None,
                website_url: None,
                api_key_url: None,
                icon_color: None,
                notes: None,
                created_at: None,
                sort_index: None,
                meta: None,
            })
    }

    /// Strategy: generate a valid app_id.
    fn valid_app_id_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("claude".to_string()),
            Just("codex".to_string()),
            Just("opencode".to_string()),
            Just("gemini".to_string()),
        ]
    }

    /// Helper: create a temp directory with a store file path inside it.
    fn setup_temp_store() -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("model_providers.json");
        (tmp, path)
    }

    proptest! {
        /// **Property 6: Provider CRUD Round-Trip**
        ///
        /// Create a provider, read it back, assert equivalence (ignoring metadata fields).
        /// Update a field, read back, assert the update is reflected.
        ///
        /// **Validates: Requirements 2.6, 2.7**
        #[test]
        fn prop_crud_round_trip(
            entry in valid_entry_strategy(),
            app_id in valid_app_id_strategy(),
            new_name in valid_name_strategy(),
        ) {
            let (_tmp, path) = setup_temp_store();

            // Step 1: Create the provider
            let created = create_provider_at(&app_id, entry.clone(), &path).unwrap();

            // Step 2: Read the store back and find the provider
            let store = read_store_from(&path).unwrap();
            let app = match app_id.as_str() {
                "claude" => &store.claude,
                "codex" => &store.codex,
                "opencode" => &store.opencode,
                "gemini" => &store.gemini,
                _ => unreachable!(),
            };
            let read_back = app.providers.get(&created.id).expect("Provider should exist in store after creation");

            // Step 3: Assert equivalence ignoring metadata fields (created_at, sort_index)
            prop_assert_eq!(&read_back.id, &entry.id);
            prop_assert_eq!(&read_back.name, &entry.name);
            prop_assert_eq!(&read_back.category, &entry.category);
            prop_assert_eq!(&read_back.settings_config, &entry.settings_config);
            prop_assert_eq!(&read_back.preset_id, &entry.preset_id);
            prop_assert_eq!(&read_back.website_url, &entry.website_url);
            prop_assert_eq!(&read_back.api_key_url, &entry.api_key_url);
            prop_assert_eq!(&read_back.icon_color, &entry.icon_color);
            prop_assert_eq!(&read_back.notes, &entry.notes);
            // created_at and sort_index are metadata assigned by the system
            prop_assert!(read_back.created_at.is_some());
            prop_assert!(read_back.sort_index.is_some());

            // Step 4: Update the provider's name
            let patch = ProviderPatch {
                name: Some(new_name.clone()),
                ..Default::default()
            };
            let updated = update_provider_at(&app_id, &created.id, patch, &path).unwrap();
            prop_assert_eq!(&updated.name, &new_name);

            // Step 5: Read back and assert the update is reflected
            let store_after = read_store_from(&path).unwrap();
            let app_after = match app_id.as_str() {
                "claude" => &store_after.claude,
                "codex" => &store_after.codex,
                "opencode" => &store_after.opencode,
                "gemini" => &store_after.gemini,
                _ => unreachable!(),
            };
            let read_after_update = app_after.providers.get(&created.id).expect("Provider should still exist after update");
            prop_assert_eq!(&read_after_update.name, &new_name);
            // Other fields should remain unchanged
            prop_assert_eq!(&read_after_update.category, &entry.category);
            prop_assert_eq!(&read_after_update.settings_config, &entry.settings_config);
        }
    }
}

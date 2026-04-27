//! Provider list management — stores named provider configurations per app.
//!
//! Data file: `~/.skillstar/config/model_providers.json`
//!
//! Structure:
//! ```json
//! {
//!   "claude": { "providers": { "<id>": { ... } }, "current": "<id>" },
//!   "codex":  { ... },
//!   "opencode": { ... }
//! }
//! ```
//!
//! When a provider is "switched to", its `settingsConfig` is written to the
//! app's live config file via the existing claude/codex/opencode modules.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{atomic_write, claude, codex, opencode};

const CLAUDE_PROVIDER_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_REASONING_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "GOOGLE_GEMINI_BASE_URL",
];

/// A single named provider entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub settings_config: Value,
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
    /// Flexible metadata bag for frontend-defined extensions
    /// (e.g. apiFormat, apiKeyField, isFullUrl, customEndpoints).
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

fn store_path() -> PathBuf {
    let base = crate::home_dir().join(".skillstar").join("config");
    base.join("model_providers.json")
}

pub fn read_store() -> Result<ProvidersStore> {
    let path = store_path();
    if !path.exists() {
        return Ok(ProvidersStore::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    // Be tolerant of UTF-8 BOM (for example when files are rewritten by PowerShell).
    let text = text.trim_start_matches('\u{FEFF}');
    let mut store: ProvidersStore = serde_json::from_str(text)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    // Data Migration: Clean up legacy / stale settings from user's persisted state
    let mut needs_migration_save = false;

    // Migrate Codex Custom Providers: remove legacy `env_key`
    for provider in store.codex.providers.values_mut() {
        if provider.category == "custom" {
            if let Some(config_val) = provider.settings_config.get_mut("config") {
                if let Some(config_str) = config_val.as_str() {
                    if config_str.contains("env_key = \"OPENAI_API_KEY\"") {
                        let sanitized = config_str
                            .lines()
                            .filter(|line| !line.trim().starts_with("env_key"))
                            .collect::<Vec<_>>()
                            .join("\n");
                        *config_val = serde_json::Value::String(sanitized);
                        needs_migration_save = true;
                    }
                }
            }
        }
    }

    if needs_migration_save {
        let json = serde_json::to_string_pretty(&store).unwrap_or_default();
        if !json.is_empty() {
            let _ = atomic_write(&path, json.as_bytes());
        }
    }

    Ok(store)
}

pub fn write_store(store: &ProvidersStore) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(store)?;
    atomic_write(&path, json.as_bytes())
}

fn get_app_mut<'a>(store: &'a mut ProvidersStore, app_id: &str) -> &'a mut AppProviders {
    match app_id {
        "claude" => &mut store.claude,
        "codex" => &mut store.codex,
        "opencode" => &mut store.opencode,
        "gemini" => &mut store.gemini,
        _ => &mut store.claude,
    }
}

fn get_app<'a>(store: &'a ProvidersStore, app_id: &str) -> &'a AppProviders {
    match app_id {
        "claude" => &store.claude,
        "codex" => &store.codex,
        "opencode" => &store.opencode,
        "gemini" => &store.gemini,
        _ => &store.claude,
    }
}

fn normalize_claude_auth_keys_in_json_env(env_obj: &mut Map<String, Value>) {
    let auth_token = env_obj
        .get("ANTHROPIC_AUTH_TOKEN")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);
    let api_key = env_obj
        .get("ANTHROPIC_API_KEY")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);
    let has_custom_base_url = env_obj
        .get("ANTHROPIC_BASE_URL")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .map_or(false, |v| !v.is_empty());

    if has_custom_base_url {
        match (auth_token, api_key) {
            (Some(token), _) => {
                env_obj.insert("ANTHROPIC_AUTH_TOKEN".to_string(), Value::String(token));
                env_obj.remove("ANTHROPIC_API_KEY");
            }
            (None, Some(key)) => {
                env_obj.insert("ANTHROPIC_AUTH_TOKEN".to_string(), Value::String(key));
                env_obj.remove("ANTHROPIC_API_KEY");
            }
            (None, None) => {
                env_obj.remove("ANTHROPIC_AUTH_TOKEN");
                env_obj.remove("ANTHROPIC_API_KEY");
            }
        }
        return;
    }

    match (auth_token, api_key) {
        (Some(_token), Some(key)) => {
            env_obj.insert("ANTHROPIC_API_KEY".to_string(), Value::String(key));
            env_obj.remove("ANTHROPIC_AUTH_TOKEN");
        }
        (Some(token), None) => {
            env_obj.insert("ANTHROPIC_AUTH_TOKEN".to_string(), Value::String(token));
            env_obj.remove("ANTHROPIC_API_KEY");
        }
        (None, Some(key)) => {
            env_obj.insert("ANTHROPIC_API_KEY".to_string(), Value::String(key));
            env_obj.remove("ANTHROPIC_AUTH_TOKEN");
        }
        (None, None) => {
            env_obj.remove("ANTHROPIC_AUTH_TOKEN");
            env_obj.remove("ANTHROPIC_API_KEY");
        }
    }
}

fn preset_id(name: &str) -> String {
    let mut id = String::with_capacity(name.len());
    let mut last_was_underscore = false;

    for ch in name.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            id.push(ch);
            last_was_underscore = false;
        } else if !last_was_underscore {
            id.push('_');
            last_was_underscore = true;
        }
    }

    id.trim_matches('_').to_string()
}

fn preset_entry(
    name: &str,
    category: &str,
    settings_config: Value,
    website_url: Option<&str>,
    api_key_url: Option<&str>,
    icon_color: Option<&str>,
    meta: Option<Value>,
) -> ProviderEntry {
    ProviderEntry {
        id: preset_id(name),
        name: name.to_string(),
        category: category.to_string(),
        settings_config,
        website_url: website_url.map(str::to_string),
        api_key_url: api_key_url.map(str::to_string),
        icon_color: icon_color.map(str::to_string),
        notes: None,
        created_at: None,
        sort_index: None,
        meta,
    }
}

fn claude_preset_entries() -> Vec<ProviderEntry> {
    vec![
        preset_entry(
            "Claude Official",
            "official",
            json!({ "env": {} }),
            Some("https://www.anthropic.com/claude-code"),
            Some("https://console.anthropic.com/settings/keys"),
            Some("#D97757"),
            None,
        ),
        preset_entry(
            "DeepSeek",
            "cn_official",
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.deepseek.com/anthropic",
                    "ANTHROPIC_MODEL": "DeepSeek-V3.2",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "DeepSeek-V3.2",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "DeepSeek-V3.2",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "DeepSeek-V3.2"
                }
            }),
            Some("https://platform.deepseek.com"),
            Some("https://platform.deepseek.com/api_keys"),
            Some("#1E88E5"),
            None,
        ),
        preset_entry(
            "Zhipu GLM",
            "cn_official",
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://open.bigmodel.cn/api/anthropic",
                    "ANTHROPIC_MODEL": "glm-5.1",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "glm-5.1",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "glm-5.1",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "glm-5.1"
                }
            }),
            Some("https://open.bigmodel.cn"),
            Some("https://www.bigmodel.cn/claude-code"),
            Some("#0F62FE"),
            None,
        ),
        preset_entry(
            "Bailian",
            "cn_official",
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://dashscope.aliyuncs.com/apps/anthropic"
                }
            }),
            Some("https://bailian.console.aliyun.com"),
            Some("https://bailian.console.aliyun.com/#/api-key"),
            Some("#624AFF"),
            None,
        ),
        preset_entry(
            "Kimi",
            "cn_official",
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.moonshot.cn/anthropic",
                    "ANTHROPIC_MODEL": "kimi-k2.5",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "kimi-k2.5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "kimi-k2.5",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "kimi-k2.5"
                }
            }),
            Some("https://platform.moonshot.cn/console"),
            Some("https://platform.moonshot.cn/console/api-keys"),
            Some("#6366F1"),
            None,
        ),
        preset_entry(
            "MiniMax",
            "cn_official",
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.minimaxi.com/anthropic",
                    "ANTHROPIC_MODEL": "MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "MiniMax-M2.7"
                }
            }),
            Some("https://platform.minimaxi.com"),
            Some("https://platform.minimaxi.com/user-center/basic-information/interface-key"),
            Some("#FF6B6B"),
            None,
        ),
        preset_entry(
            "DouBaoSeed",
            "cn_official",
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://ark.cn-beijing.volces.com/api/coding",
                    "ANTHROPIC_MODEL": "doubao-seed-2-0-code-preview-latest",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "doubao-seed-2-0-code-preview-latest",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "doubao-seed-2-0-code-preview-latest",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "doubao-seed-2-0-code-preview-latest"
                }
            }),
            Some("https://www.volcengine.com/product/doubao"),
            Some("https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey"),
            Some("#3370FF"),
            None,
        ),
        preset_entry(
            "Xiaomi MiMo",
            "cn_official",
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.xiaomimimo.com/anthropic",
                    "ANTHROPIC_MODEL": "mimo-v2-pro",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "mimo-v2-pro",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "mimo-v2-pro",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "mimo-v2-pro"
                }
            }),
            Some("https://platform.xiaomimimo.com"),
            None,
            Some("#000000"),
            None,
        ),
        preset_entry(
            "OpenRouter",
            "aggregator",
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                    "ANTHROPIC_MODEL": "anthropic/claude-sonnet-4.6",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "anthropic/claude-haiku-4.5",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "anthropic/claude-sonnet-4.6",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "anthropic/claude-opus-4.6"
                }
            }),
            Some("https://openrouter.ai"),
            Some("https://openrouter.ai/keys"),
            Some("#6566F1"),
            None,
        ),
        preset_entry(
            "SiliconFlow",
            "aggregator",
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.siliconflow.cn",
                    "ANTHROPIC_MODEL": "Pro/MiniMaxAI/MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "Pro/MiniMaxAI/MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_SONNET_MODEL": "Pro/MiniMaxAI/MiniMax-M2.7",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "Pro/MiniMaxAI/MiniMax-M2.7"
                }
            }),
            Some("https://siliconflow.cn"),
            Some("https://cloud.siliconflow.cn/account/ak"),
            Some("#6E29F6"),
            None,
        ),
    ]
}

fn codex_preset_entries() -> Vec<ProviderEntry> {
    vec![
        preset_entry(
            "OpenAI Official",
            "official",
            json!({
                "config": "model = \"gpt-5.4\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true"
            }),
            Some("https://chatgpt.com/codex"),
            Some("https://platform.openai.com/api-keys"),
            Some("#00A67E"),
            None,
        ),
        preset_entry(
            "OpenRouter",
            "aggregator",
            json!({
                "config": "model_provider = \"openrouter\"\nmodel = \"gpt-5.4\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\n\n[model_providers.openrouter]\nname = \"openrouter\"\nbase_url = \"https://openrouter.ai/api/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = true"
            }),
            Some("https://openrouter.ai"),
            Some("https://openrouter.ai/keys"),
            Some("#6566F1"),
            None,
        ),
    ]
}

fn opencode_preset_entries() -> Vec<ProviderEntry> {
    vec![
        preset_entry(
            "DeepSeek",
            "cn_official",
            json!({
                "provider": {
                    "deepseek": {
                        "npm": "@ai-sdk/openai-compatible",
                        "options": { "baseURL": "https://api.deepseek.com/v1", "apiKey": "", "setCacheKey": true },
                        "models": {
                            "deepseek-chat": { "name": "DeepSeek V3.2" },
                            "deepseek-reasoner": { "name": "DeepSeek R1" }
                        }
                    }
                }
            }),
            Some("https://platform.deepseek.com"),
            Some("https://platform.deepseek.com/api_keys"),
            Some("#1E88E5"),
            Some(json!({ "baseURL": "https://api.deepseek.com/v1" })),
        ),
        preset_entry(
            "Zhipu GLM",
            "cn_official",
            json!({
                "provider": {
                    "zhipu_glm": {
                        "npm": "@ai-sdk/openai-compatible",
                        "name": "Zhipu GLM",
                        "options": { "baseURL": "https://open.bigmodel.cn/api/paas/v4", "apiKey": "", "setCacheKey": true },
                        "models": { "glm-5.1": { "name": "GLM-5.1" } }
                    }
                }
            }),
            Some("https://open.bigmodel.cn"),
            None,
            Some("#0F62FE"),
            Some(json!({ "baseURL": "https://open.bigmodel.cn/api/paas/v4" })),
        ),
        preset_entry(
            "Kimi K2.5",
            "cn_official",
            json!({
                "provider": {
                    "kimi_k2_5": {
                        "npm": "@ai-sdk/openai-compatible",
                        "name": "Kimi k2.5",
                        "options": { "baseURL": "https://api.moonshot.cn/v1", "apiKey": "", "setCacheKey": true },
                        "models": { "kimi-k2.5": { "name": "Kimi K2.5" } }
                    }
                }
            }),
            Some("https://platform.moonshot.cn/console"),
            None,
            Some("#6366F1"),
            Some(json!({ "baseURL": "https://api.moonshot.cn/v1" })),
        ),
        preset_entry(
            "Bailian",
            "cn_official",
            json!({
                "provider": {
                    "bailian": {
                        "npm": "@ai-sdk/openai-compatible",
                        "name": "Bailian",
                        "options": { "baseURL": "https://dashscope.aliyuncs.com/compatible-mode/v1", "apiKey": "", "setCacheKey": true },
                        "models": {}
                    }
                }
            }),
            Some("https://bailian.console.aliyun.com"),
            None,
            Some("#624AFF"),
            Some(json!({ "baseURL": "https://dashscope.aliyuncs.com/compatible-mode/v1" })),
        ),
        preset_entry(
            "MiniMax",
            "cn_official",
            json!({
                "provider": {
                    "minimax": {
                        "npm": "@ai-sdk/openai-compatible",
                        "name": "MiniMax",
                        "options": { "baseURL": "https://api.minimaxi.com/v1", "apiKey": "", "setCacheKey": true },
                        "models": {}
                    }
                }
            }),
            Some("https://platform.minimaxi.com"),
            None,
            Some("#FF6B6B"),
            Some(json!({ "baseURL": "https://api.minimaxi.com/v1" })),
        ),
        preset_entry(
            "DouBaoSeed",
            "cn_official",
            json!({
                "provider": {
                    "doubaoseed": {
                        "npm": "@ai-sdk/openai-compatible",
                        "name": "DouBaoSeed",
                        "options": { "baseURL": "https://ark.cn-beijing.volces.com/api/v3", "apiKey": "", "setCacheKey": true },
                        "models": {
                            "doubao-seed-2-0-code-preview-latest": { "name": "Doubao Seed Code Preview" }
                        }
                    }
                }
            }),
            Some("https://www.volcengine.com/product/doubao"),
            None,
            Some("#3370FF"),
            Some(json!({ "baseURL": "https://ark.cn-beijing.volces.com/api/v3" })),
        ),
        preset_entry(
            "Xiaomi MiMo",
            "cn_official",
            json!({
                "provider": {
                    "xiaomi_mimo": {
                        "npm": "@ai-sdk/openai-compatible",
                        "name": "Xiaomi MiMo",
                        "options": { "baseURL": "https://api.xiaomimimo.com/v1", "apiKey": "", "setCacheKey": true },
                        "models": { "mimo-v2-pro": { "name": "MiMo V2 Pro" } }
                    }
                }
            }),
            Some("https://platform.xiaomimimo.com"),
            None,
            Some("#000000"),
            Some(json!({ "baseURL": "https://api.xiaomimimo.com/v1" })),
        ),
    ]
}

fn gemini_preset_entries() -> Vec<ProviderEntry> {
    vec![preset_entry(
        "Gemini Official",
        "official",
        json!({
            "env": {
                "GEMINI_API_KEY": ""
            }
        }),
        Some("https://aistudio.google.com/"),
        Some("https://aistudio.google.com/app/apikey"),
        Some("#3B82F6"),
        None,
    )]
}

// ── Public API ──────────────────────────────────────────────────

pub fn get_providers(app_id: &str) -> Result<(HashMap<String, ProviderEntry>, Option<String>)> {
    let store = read_store()?;
    let app = get_app(&store, app_id);
    Ok((app.providers.clone(), app.current.clone()))
}

pub fn add_provider(app_id: &str, entry: ProviderEntry) -> Result<()> {
    let mut store = read_store()?;
    let app = get_app_mut(&mut store, app_id);
    app.providers.insert(entry.id.clone(), entry);
    write_store(&store)
}

pub fn update_provider(app_id: &str, entry: ProviderEntry) -> Result<()> {
    let mut store = read_store()?;
    let app = get_app_mut(&mut store, app_id);
    let is_current = app.current.as_deref() == Some(&entry.id);
    let config = entry.settings_config.clone();
    app.providers.insert(entry.id.clone(), entry);
    write_store(&store)?;

    // Apply config to live config file only if this provider is already active.
    if is_current {
        apply_config_to_app(app_id, &config)?;
    }
    Ok(())
}

pub fn delete_provider(app_id: &str, provider_id: &str) -> Result<()> {
    let mut store = read_store()?;
    let app = get_app_mut(&mut store, app_id);
    app.providers.remove(provider_id);
    if app.current.as_deref() == Some(provider_id) {
        app.current = None;
    }
    write_store(&store)
}

/// Write a provider's connection-layer config to the app's live config file.
/// **Only merges connection fields** (env/auth/provider blocks), preserving
/// all behavior settings (effortLevel, approval_policy, permission, etc.).
fn apply_config_to_app(app_id: &str, config: &Value) -> Result<()> {
    match app_id {
        "claude" => {
            // Only replace provider-owned env keys, preserving unrelated user env vars.
            let mut existing = claude::read_settings().unwrap_or(Value::Object(Default::default()));
            if !existing.is_object() {
                existing = Value::Object(Default::default());
            }
            if let Some(new_env) = config.get("env").and_then(|v| v.as_object()) {
                let settings_obj = existing.as_object_mut().unwrap();
                let env_value = settings_obj
                    .entry("env".to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
                if !env_value.is_object() {
                    *env_value = Value::Object(Map::new());
                }
                let env_obj = env_value.as_object_mut().unwrap();

                for key in CLAUDE_PROVIDER_ENV_KEYS {
                    env_obj.remove(*key);
                }

                for (key, value) in new_env {
                    env_obj.insert(key.clone(), value.clone());
                }

                normalize_claude_auth_keys_in_json_env(env_obj);
            }
            claude::write_settings(&existing)?;
        }
        "codex" => {
            // Merge provider connection fields into existing config.toml,
            // preserving behavior settings (model_reasoning_effort, approval_policy, etc.)
            use toml_edit::DocumentMut;

            let existing_text = codex::read_config_text().unwrap_or_default();
            let mut doc: DocumentMut = match existing_text.parse() {
                Ok(d) => d,
                Err(e) => return Err(anyhow::anyhow!("config.toml 格式错误，请检查语法: {}", e)),
            };

            // Parse the provider's stored config text to extract connection fields
            let provider_toml = config.get("config").and_then(|v| v.as_str()).unwrap_or("");

            if !provider_toml.trim().is_empty() {
                // Sanitize: stored config may contain double-escaped quotes (\\") from
                // JSON serialization roundtrip — normalize to plain quotes for valid TOML.
                let sanitized_toml = provider_toml.replace("\\\"", "\"");

                if let Ok(provider_doc) = sanitized_toml.parse::<DocumentMut>() {
                    // Merge top-level connection fields: model, model_provider, base_url, openai_base_url
                    for key in [
                        "model",
                        "model_provider",
                        "base_url",
                        "openai_base_url",
                        "disable_response_storage",
                    ] {
                        if let Some(item) = provider_doc.get(key) {
                            doc[key] = item.clone();
                        } else if key == "model_provider"
                            || key == "openai_base_url"
                            || key == "base_url"
                        {
                            doc.remove(key);
                        }
                    }

                    // Replace model_providers table entirely to avoid stale provider buildup
                    if let Some(providers_item) = provider_doc.get("model_providers") {
                        doc["model_providers"] = providers_item.clone();
                    } else {
                        doc.remove("model_providers");
                    }
                } else {
                    // TOML parsing failed even after sanitization — still clean up stale fields
                    // so we don't leave behind old provider configs.
                    tracing::warn!(
                        "Failed to parse provider TOML, clearing stale connection fields"
                    );
                    for key in [
                        "model_provider",
                        "base_url",
                        "openai_base_url",
                        "model_providers",
                        "disable_response_storage",
                    ] {
                        doc.remove(key);
                    }
                }
            } else {
                // Empty config = OpenAI official direct (no provider, no proxy)
                // Only clear provider pointer fields, keep behavior settings
                doc.remove("model_provider");
                doc.remove("base_url");
                doc.remove("openai_base_url");
                doc.remove("model_providers");
                if doc.get("model").is_none() {
                    doc["model"] = toml_edit::Item::Value(toml_edit::Value::from("gpt-5.4"));
                }
                if doc.get("model_reasoning_effort").is_none() {
                    doc["model_reasoning_effort"] =
                        toml_edit::Item::Value(toml_edit::Value::from("high"));
                }
            }

            codex::write_config(&doc.to_string())?;

            // Merge API key fields into auth.json (preserves OAuth tokens)
            if let Some(auth) = config.get("auth").and_then(|v| v.as_object()) {
                let fields: std::collections::HashMap<String, String> = auth
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();
                if !fields.is_empty() {
                    codex::merge_auth_fields(&fields)?;
                }
            }
        }
        "opencode" => {
            // Merge connection fields: provider block + model selection, preserve behavior fields
            let mut existing = opencode::read_config().unwrap_or(Value::Object(Default::default()));
            if !existing.is_object() {
                existing = Value::Object(Default::default());
            }
            let obj = existing.as_object_mut().unwrap();
            if let Some(new_provider) = config.get("provider") {
                obj.insert("provider".to_string(), new_provider.clone());
            }
            // Also set model / small_model if the provider config specifies them
            for key in ["model", "small_model"] {
                if let Some(val) = config.get(key) {
                    if val.is_string() && !val.as_str().unwrap_or("").is_empty() {
                        obj.insert(key.to_string(), val.clone());
                    }
                }
            }
            opencode::write_config(&existing)?;
        }
        "gemini" => {
            // Gemini currently has no separate configuration file.
            // Environment variables are injected dynamically.
        }
        _ => {}
    }
    Ok(())
}

/// Switch to a provider: update `current`, then write the provider's
/// `settingsConfig` to the app's live config file.
pub fn switch_provider(app_id: &str, provider_id: &str) -> Result<()> {
    let mut store = read_store()?;
    let app = get_app_mut(&mut store, app_id);

    let entry = app
        .providers
        .get(provider_id)
        .cloned()
        .context("Provider not found")?;

    app.current = Some(provider_id.to_string());
    write_store(&store)?;

    // Write to the app's live config file
    apply_config_to_app(app_id, &entry.settings_config)?;

    Ok(())
}

/// Clear the current provider selection for an app.
/// Used when OAuth account takes over as the active auth method.
pub fn clear_current(app_id: &str) -> Result<()> {
    let mut store = read_store()?;
    let app = get_app_mut(&mut store, app_id);
    app.current = None;
    write_store(&store)
}

pub fn reorder_providers(app_id: &str, provider_ids: Vec<String>) -> Result<()> {
    let mut store = read_store()?;
    let app = get_app_mut(&mut store, app_id);
    for (i, id) in provider_ids.into_iter().enumerate() {
        if let Some(entry) = app.providers.get_mut(&id) {
            entry.sort_index = Some(i as u32);
        }
    }
    write_store(&store)
}

pub fn get_provider_presets(app_id: &str) -> Vec<ProviderEntry> {
    match app_id {
        "claude" => claude_preset_entries(),
        "codex" => codex_preset_entries(),
        "opencode" => opencode_preset_entries(),
        "gemini" => gemini_preset_entries(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use serde_json::json;
    use std::ffi::OsStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static std::sync::Mutex<()> {
        crate::test_env_lock()
    }

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    fn with_temp_home<F>(suffix: &str, f: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-model-providers-{}-{}", suffix, stamp));
        let previous_home = std::env::var_os("HOME");
        let previous_test_home = std::env::var_os("SKILLSTAR_TEST_HOME");
        let previous_test_config = std::env::var_os("SKILLSTAR_TEST_CONFIG_DIR");
        set_env("HOME", temp_root.join("home"));
        set_env("SKILLSTAR_TEST_HOME", temp_root.join("home"));
        set_env("SKILLSTAR_TEST_CONFIG_DIR", temp_root.join("config"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = f();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        match previous_test_home {
            Some(value) => set_env("SKILLSTAR_TEST_HOME", value),
            None => remove_env("SKILLSTAR_TEST_HOME"),
        }
        match previous_test_config {
            Some(value) => set_env("SKILLSTAR_TEST_CONFIG_DIR", value),
            None => remove_env("SKILLSTAR_TEST_CONFIG_DIR"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }

    fn sample_codex_provider() -> ProviderEntry {
        ProviderEntry {
            id: "openrouter".to_string(),
            name: "OpenRouter".to_string(),
            category: "aggregator".to_string(),
            settings_config: json!({
                "config": "model_provider = \"openrouter\"\nmodel = \"gpt-5.4\"\n\n[model_providers.openrouter]\nname = \"openrouter\"\nbase_url = \"https://openrouter.ai/api/v1\"\nrequires_openai_auth = true"
            }),
            website_url: None,
            api_key_url: None,
            icon_color: None,
            notes: None,
            created_at: Some(1),
            sort_index: Some(0),
            meta: None,
        }
    }

    #[test]
    fn read_store_tolerates_utf8_bom() -> Result<()> {
        with_temp_home("bom-tolerant", || {
            let path = store_path();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(
                br#"{"claude":{"providers":{"p1":{"id":"p1","name":"P1","category":"custom","settingsConfig":{"env":{"ANTHROPIC_API_KEY":"k"}}}},"current":"p1"}}"#,
            );
            std::fs::write(&path, bytes)?;

            let store = read_store()?;
            assert!(store.claude.providers.contains_key("p1"));
            assert_eq!(store.claude.current.as_deref(), Some("p1"));
            Ok(())
        })
    }

    #[test]
    fn add_and_update_provider_do_not_activate_until_explicit_switch() -> Result<()> {
        with_temp_home("no-auto-activate", || {
            let entry = sample_codex_provider();
            add_provider("codex", entry.clone())?;

            let (providers, current) = get_providers("codex")?;
            assert!(providers.contains_key(&entry.id));
            assert!(current.is_none());
            assert!(
                !codex::config_toml_path().exists(),
                "adding a provider should not rewrite the live Codex config"
            );

            let mut updated = entry.clone();
            updated.settings_config = json!({
                "config": "model_provider = \"openrouter\"\nmodel = \"gpt-5.4\"\n\n[model_providers.openrouter]\nname = \"openrouter\"\nbase_url = \"https://openrouter.ai/api/v1\"\nrequires_openai_auth = true",
                "auth": { "OPENAI_API_KEY": "sk-test" }
            });
            update_provider("codex", updated)?;

            let (_, current_after_update) = get_providers("codex")?;
            assert!(current_after_update.is_none());
            assert!(
                !codex::config_toml_path().exists(),
                "editing an unselected provider should not rewrite the live Codex config"
            );
            Ok(())
        })
    }

    #[test]
    fn apply_claude_provider_env_preserves_unrelated_env_keys() -> Result<()> {
        with_temp_home("claude-merge-env", || {
            claude::write_settings(&json!({
                "theme": "dark",
                "env": {
                    "HTTP_PROXY": "http://127.0.0.1:8080",
                    "CUSTOM_FLAG": "keep-me",
                    "ANTHROPIC_AUTH_TOKEN": "old-token",
                    "ANTHROPIC_MODEL": "old-model",
                    "ANTHROPIC_DEFAULT_OPUS_MODEL": "old-opus"
                }
            }))?;

            apply_config_to_app(
                "claude",
                &json!({
                    "env": {
                        "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                        "ANTHROPIC_MODEL": "anthropic/claude-sonnet-4.6"
                    }
                }),
            )?;

            let settings = claude::read_settings()?;
            assert_eq!(settings.get("theme"), Some(&json!("dark")));

            let env = settings
                .get("env")
                .and_then(|value| value.as_object())
                .expect("env object should exist");

            assert_eq!(env.get("HTTP_PROXY"), Some(&json!("http://127.0.0.1:8080")));
            assert_eq!(env.get("CUSTOM_FLAG"), Some(&json!("keep-me")));
            assert_eq!(
                env.get("ANTHROPIC_BASE_URL"),
                Some(&json!("https://openrouter.ai/api"))
            );
            assert_eq!(
                env.get("ANTHROPIC_MODEL"),
                Some(&json!("anthropic/claude-sonnet-4.6"))
            );
            assert!(
                !env.contains_key("ANTHROPIC_AUTH_TOKEN"),
                "stale provider auth fields should be cleared"
            );
            assert!(
                !env.contains_key("ANTHROPIC_DEFAULT_OPUS_MODEL"),
                "stale provider model mappings should be cleared"
            );
            Ok(())
        })
    }

    #[test]
    fn apply_claude_provider_env_keeps_auth_token_only() -> Result<()> {
        with_temp_home("claude-auth-token-only", || {
            apply_config_to_app(
                "claude",
                &json!({
                    "env": {
                        "ANTHROPIC_AUTH_TOKEN": "token-only"
                    }
                }),
            )?;

            let settings = claude::read_settings()?;
            let env = settings
                .get("env")
                .and_then(|value| value.as_object())
                .expect("env object should exist");

            assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN"), Some(&json!("token-only")));
            assert!(!env.contains_key("ANTHROPIC_API_KEY"));
            Ok(())
        })
    }

    #[test]
    fn apply_claude_provider_env_token_with_custom_base_url_keeps_auth_token() -> Result<()> {
        with_temp_home("claude-auth-token-custom-base", || {
            apply_config_to_app(
                "claude",
                &json!({
                    "env": {
                        "ANTHROPIC_AUTH_TOKEN": "token-only",
                        "ANTHROPIC_BASE_URL": "https://api.minimaxi.com/anthropic"
                    }
                }),
            )?;

            let settings = claude::read_settings()?;
            let env = settings
                .get("env")
                .and_then(|value| value.as_object())
                .expect("env object should exist");

            assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN"), Some(&json!("token-only")));
            assert!(!env.contains_key("ANTHROPIC_API_KEY"));
            Ok(())
        })
    }

    #[test]
    fn apply_claude_provider_env_api_key_with_custom_base_url_maps_to_auth_token() -> Result<()> {
        with_temp_home("claude-api-key-custom-base", || {
            apply_config_to_app(
                "claude",
                &json!({
                    "env": {
                        "ANTHROPIC_API_KEY": "key-only",
                        "ANTHROPIC_BASE_URL": "https://api.minimaxi.com/anthropic"
                    }
                }),
            )?;

            let settings = claude::read_settings()?;
            let env = settings
                .get("env")
                .and_then(|value| value.as_object())
                .expect("env object should exist");

            assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN"), Some(&json!("key-only")));
            assert!(!env.contains_key("ANTHROPIC_API_KEY"));
            Ok(())
        })
    }

    #[test]
    fn apply_claude_provider_env_keeps_api_key_only() -> Result<()> {
        with_temp_home("claude-api-key-only", || {
            apply_config_to_app(
                "claude",
                &json!({
                    "env": {
                        "ANTHROPIC_API_KEY": "key-only"
                    }
                }),
            )?;

            let settings = claude::read_settings()?;
            let env = settings
                .get("env")
                .and_then(|value| value.as_object())
                .expect("env object should exist");

            assert_eq!(env.get("ANTHROPIC_API_KEY"), Some(&json!("key-only")));
            assert!(!env.contains_key("ANTHROPIC_AUTH_TOKEN"));
            Ok(())
        })
    }

    #[test]
    fn apply_claude_provider_env_prefers_api_key_when_both_present() -> Result<()> {
        with_temp_home("claude-auth-key-conflict", || {
            apply_config_to_app(
                "claude",
                &json!({
                    "env": {
                        "ANTHROPIC_AUTH_TOKEN": "token-value",
                        "ANTHROPIC_API_KEY": "key-value"
                    }
                }),
            )?;

            let settings = claude::read_settings()?;
            let env = settings
                .get("env")
                .and_then(|value| value.as_object())
                .expect("env object should exist");

            assert_eq!(env.get("ANTHROPIC_API_KEY"), Some(&json!("key-value")));
            assert!(!env.contains_key("ANTHROPIC_AUTH_TOKEN"));
            Ok(())
        })
    }

    #[test]
    fn switch_provider_updates_current_and_writes_live_config() -> Result<()> {
        with_temp_home("switch-provider", || {
            let entry = sample_codex_provider();
            add_provider("codex", entry.clone())?;
            switch_provider("codex", &entry.id)?;

            let (providers, current) = get_providers("codex")?;
            assert_eq!(current.as_deref(), Some(entry.id.as_str()));
            assert!(providers.contains_key(&entry.id));
            assert!(
                codex::config_toml_path().exists(),
                "switching provider should write live Codex config"
            );

            Ok(())
        })
    }

    #[test]
    fn delete_provider_removes_entry_and_clears_current() -> Result<()> {
        with_temp_home("delete-provider", || {
            let entry = sample_codex_provider();
            add_provider("codex", entry.clone())?;
            switch_provider("codex", &entry.id)?;

            delete_provider("codex", &entry.id)?;

            let (providers, current) = get_providers("codex")?;
            assert!(!providers.contains_key(&entry.id));
            assert!(current.is_none());
            Ok(())
        })
    }

    #[test]
    fn clear_current_resets_active_provider() -> Result<()> {
        with_temp_home("clear-current", || {
            let entry = sample_codex_provider();
            add_provider("codex", entry.clone())?;
            switch_provider("codex", &entry.id)?;

            clear_current("codex")?;

            let (_, current) = get_providers("codex")?;
            assert!(current.is_none());
            Ok(())
        })
    }

    #[test]
    fn reorder_providers_assigns_sort_indices() -> Result<()> {
        with_temp_home("reorder", || {
            let a = ProviderEntry {
                id: "a".to_string(),
                name: "A".to_string(),
                category: "custom".to_string(),
                settings_config: json!({}),
                website_url: None,
                api_key_url: None,
                icon_color: None,
                notes: None,
                created_at: None,
                sort_index: None,
                meta: None,
            };
            let b = ProviderEntry {
                id: "b".to_string(),
                name: "B".to_string(),
                category: "custom".to_string(),
                settings_config: json!({}),
                website_url: None,
                api_key_url: None,
                icon_color: None,
                notes: None,
                created_at: None,
                sort_index: None,
                meta: None,
            };
            add_provider("codex", a.clone())?;
            add_provider("codex", b.clone())?;

            reorder_providers("codex", vec!["b".to_string(), "a".to_string()])?;

            let (providers, _) = get_providers("codex")?;
            assert_eq!(providers.get("b").unwrap().sort_index, Some(0));
            assert_eq!(providers.get("a").unwrap().sort_index, Some(1));
            Ok(())
        })
    }

    #[test]
    fn update_provider_activates_when_current() -> Result<()> {
        with_temp_home("update-active", || {
            let entry = sample_codex_provider();
            add_provider("codex", entry.clone())?;
            switch_provider("codex", &entry.id)?;

            let mut updated = entry.clone();
            updated.settings_config = json!({
                "config": "model_provider = \"openrouter\"\nmodel = \"gpt-5.4\"\n",
                "auth": { "OPENAI_API_KEY": "new-key" }
            });
            update_provider("codex", updated)?;

            let (providers, current) = get_providers("codex")?;
            assert_eq!(current.as_deref(), Some(entry.id.as_str()));
            assert!(providers.contains_key(&entry.id));
            assert!(codex::auth_json_path().exists());
            Ok(())
        })
    }

    #[test]
    fn get_provider_presets_returns_codex_builtin_entries() {
        let presets = get_provider_presets("codex");

        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0].id, "openai_official");
        assert_eq!(presets[0].name, "OpenAI Official");
        assert_eq!(presets[0].category, "official");
        assert_eq!(
            presets[0]
                .settings_config
                .get("config")
                .and_then(|value| value.as_str()),
            Some(
                "model = \"gpt-5.4\"\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true"
            )
        );
        assert_eq!(presets[1].id, "openrouter");
    }

    #[test]
    fn get_provider_presets_wraps_opencode_provider_block_and_meta() {
        let presets = get_provider_presets("opencode");
        let minimax = presets
            .into_iter()
            .find(|preset| preset.id == "minimax")
            .expect("minimax preset should exist");

        assert_eq!(
            minimax
                .settings_config
                .get("provider")
                .and_then(|provider| provider.get("minimax"))
                .and_then(|provider| provider.get("options"))
                .and_then(|options| options.get("baseURL"))
                .and_then(|value| value.as_str()),
            Some("https://api.minimaxi.com/v1")
        );
        assert_eq!(
            minimax
                .meta
                .as_ref()
                .and_then(|meta| meta.get("baseURL"))
                .and_then(|value| value.as_str()),
            Some("https://api.minimaxi.com/v1")
        );
    }

    #[test]
    fn get_provider_presets_returns_empty_for_unknown_app() {
        assert!(get_provider_presets("unknown").is_empty());
    }

    #[test]
    fn get_provider_presets_returns_gemini_builtin_entries() {
        let presets = get_provider_presets("gemini");

        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, "gemini_official");
        assert_eq!(presets[0].category, "official");
        assert_eq!(
            presets[0]
                .settings_config
                .get("env")
                .and_then(|env| env.get("GEMINI_API_KEY"))
                .and_then(|value| value.as_str()),
            Some("")
        );
        assert_eq!(
            presets[0].api_key_url.as_deref(),
            Some("https://aistudio.google.com/app/apikey")
        );
    }
}

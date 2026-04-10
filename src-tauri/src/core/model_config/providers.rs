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
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;

use super::{atomic_write, claude, codex, opencode};

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
    let base = super::home_dir().join(".skillstar").join("config");
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

    // For Codex: clear OAuth current account so only one auth shows "当前"
    if app_id == "codex" {
        if let Err(e) = super::codex_accounts::clear_current_account() {
            tracing::warn!(
                "Failed to clear codex current account on provider switch: {}",
                e
            );
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use serde_json::json;
    use std::ffi::OsStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static std::sync::Mutex<()> {
        crate::core::test_env_lock()
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
}

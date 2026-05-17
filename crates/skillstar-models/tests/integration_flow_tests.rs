//! Integration tests for the full provider switch flow.
//!
//! **Validates: Requirements 4.1, 4.2, 4.3, 4.4, 4.5**
//!
//! Test 1: Create → Activate → Verify Store
//! - Create a provider with `create_provider_at`
//! - Verify it's auto-activated (first provider)
//! - Create a second provider
//! - Switch to the second provider with `switch_active_provider_at`
//! - Read the store and verify `current` is the second provider's ID
//!
//! Test 2: Preset Creation → Verify Fields → Generate Config
//! - Create a provider from preset with `create_from_preset_at`
//! - Verify the provider has preset fields filled (base_url, models, icon_color, preset_id)
//! - Parse the settings_config and call `generate_claude_code_config` / `generate_codex_config`
//! - Verify the generated config contains the correct apiUrl/apiKey or base_url/api_key

use serde_json::Value;
use skillstar_models::providers::{
    create_from_preset_at, create_provider_at, read_store_from, switch_active_provider_at,
    ModelMapping, ProviderEntry, ProviderSettings,
};
use skillstar_models::tool_sync::{generate_claude_code_config, generate_codex_config};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a temp directory with a store file path inside it.
fn setup_temp_store() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("model_providers.json");
    (tmp, path)
}

/// Build valid ProviderSettings with the given base_url and api_key.
fn make_settings(base_url: &str, api_key: &str) -> ProviderSettings {
    ProviderSettings {
        base_url: base_url.to_string(),
        api_key: api_key.to_string(),
        models: vec![ModelMapping {
            source_model: "model-a".to_string(),
            target_model: "model-b".to_string(),
            enabled: true,
        }],
        timeout_ms: None,
        max_retries: None,
    }
}

/// Build a valid ProviderEntry with the given id, name, base_url, and api_key.
fn make_provider(id: &str, name: &str, base_url: &str, api_key: &str) -> ProviderEntry {
    let settings = make_settings(base_url, api_key);
    ProviderEntry {
        id: id.to_string(),
        name: name.to_string(),
        category: "cloud".to_string(),
        settings_config: serde_json::to_value(&settings).unwrap(),
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

// ===========================================================================
// Test 1: Create → Activate → Verify Store
//
// Full provider switch flow:
// 1. Create a provider → verify auto-activated (first provider)
// 2. Create a second provider → verify first is still active
// 3. Switch to second provider → verify store updated
// ===========================================================================

#[test]
fn test_create_activate_verify_store_flow() {
    let (_tmp, path) = setup_temp_store();

    // Step 1: Create the first provider for "claude"
    let provider1 = make_provider(
        "provider-1",
        "First Provider",
        "https://api.first.com/v1",
        "sk-first-key-123",
    );
    let created1 = create_provider_at("claude", provider1, &path).unwrap();

    // Verify it was created with metadata
    assert_eq!(created1.id, "provider-1");
    assert_eq!(created1.name, "First Provider");
    assert!(created1.created_at.is_some());
    assert_eq!(created1.sort_index, Some(0));

    // Verify auto-activation: first provider becomes current
    let store = read_store_from(&path).unwrap();
    assert_eq!(
        store.claude.current,
        Some("provider-1".to_string()),
        "First provider should be auto-activated"
    );

    // Step 2: Create a second provider for "claude"
    let provider2 = make_provider(
        "provider-2",
        "Second Provider",
        "https://api.second.com/v1",
        "sk-second-key-456",
    );
    let created2 = create_provider_at("claude", provider2, &path).unwrap();

    assert_eq!(created2.id, "provider-2");
    assert_eq!(created2.sort_index, Some(1));

    // Verify first provider is still active (second doesn't auto-activate)
    let store = read_store_from(&path).unwrap();
    assert_eq!(
        store.claude.current,
        Some("provider-1".to_string()),
        "First provider should still be active after creating second"
    );

    // Step 3: Switch to the second provider
    switch_active_provider_at("claude", "provider-2", &path).unwrap();

    // Verify store is updated with second provider as current
    let store = read_store_from(&path).unwrap();
    assert_eq!(
        store.claude.current,
        Some("provider-2".to_string()),
        "Current should be updated to provider-2 after switch"
    );

    // Verify both providers still exist in the store
    assert_eq!(store.claude.providers.len(), 2);
    assert!(store.claude.providers.contains_key("provider-1"));
    assert!(store.claude.providers.contains_key("provider-2"));
}

#[test]
fn test_create_activate_verify_store_codex_app() {
    let (_tmp, path) = setup_temp_store();

    // Same flow but for "codex" app to verify AppId isolation
    let provider1 = make_provider(
        "codex-p1",
        "Codex Provider 1",
        "https://api.openai.com/v1",
        "sk-openai-key-1",
    );
    let provider2 = make_provider(
        "codex-p2",
        "Codex Provider 2",
        "https://api.deepseek.com/v1",
        "sk-deepseek-key-2",
    );

    // Create first → auto-activated
    create_provider_at("codex", provider1, &path).unwrap();
    let store = read_store_from(&path).unwrap();
    assert_eq!(store.codex.current, Some("codex-p1".to_string()));

    // Create second → first still active
    create_provider_at("codex", provider2, &path).unwrap();
    let store = read_store_from(&path).unwrap();
    assert_eq!(store.codex.current, Some("codex-p1".to_string()));

    // Switch to second
    switch_active_provider_at("codex", "codex-p2", &path).unwrap();
    let store = read_store_from(&path).unwrap();
    assert_eq!(store.codex.current, Some("codex-p2".to_string()));

    // Verify claude app is unaffected (AppId isolation)
    assert!(store.claude.providers.is_empty());
    assert_eq!(store.claude.current, None);
}

#[test]
fn test_switch_to_nonexistent_provider_fails() {
    let (_tmp, path) = setup_temp_store();

    // Create one provider
    let provider = make_provider(
        "existing",
        "Existing Provider",
        "https://api.example.com/v1",
        "sk-key",
    );
    create_provider_at("claude", provider, &path).unwrap();

    // Attempt to switch to a non-existent provider
    let result = switch_active_provider_at("claude", "nonexistent-id", &path);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("not found"));

    // Verify original provider is still active
    let store = read_store_from(&path).unwrap();
    assert_eq!(store.claude.current, Some("existing".to_string()));
}

// ===========================================================================
// Test 2: Preset Creation → Verify Fields → Generate Config
//
// Full preset-to-config flow:
// 1. Create a provider from preset → verify preset fields filled
// 2. Parse settings_config → generate Claude Code config → verify apiUrl/apiKey
// 3. Parse settings_config → generate Codex config → verify base_url/api_key
// ===========================================================================

#[test]
fn test_preset_creation_activation_claude_code_config() {
    let (_tmp, path) = setup_temp_store();

    // Step 1: Create a provider from the "deepseek" preset for "claude" app
    let api_key = "sk-deepseek-test-key-789";
    let entry = create_from_preset_at("claude", "deepseek", api_key, &path).unwrap();

    // Verify preset fields are filled
    assert_eq!(entry.preset_id.as_deref(), Some("deepseek"));
    assert_eq!(entry.icon_color.as_deref(), Some("#4D6BFE"));
    assert_eq!(
        entry.api_key_url.as_deref(),
        Some("https://platform.deepseek.com/api_keys")
    );
    assert!(!entry.id.is_empty(), "ID should be auto-generated (UUID)");
    assert!(entry.created_at.is_some(), "created_at should be set");

    // Step 2: Parse settings_config and verify base_url and models
    let settings: ProviderSettings =
        serde_json::from_value(entry.settings_config.clone()).unwrap();
    assert_eq!(settings.base_url, "https://api.deepseek.com/v1");
    assert_eq!(settings.api_key, api_key);
    assert_eq!(settings.models.len(), 2);
    assert_eq!(settings.models[0].source_model, "deepseek-chat");
    assert_eq!(settings.models[1].source_model, "deepseek-reasoner");

    // Step 3: Generate Claude Code JSON config and verify
    let json_str = generate_claude_code_config(&settings).unwrap();
    let parsed: HashMap<String, Value> = serde_json::from_str(&json_str).unwrap();

    assert_eq!(
        parsed.get("apiUrl").unwrap().as_str().unwrap(),
        "https://api.deepseek.com/v1",
        "Claude Code config apiUrl should match provider base_url"
    );
    assert_eq!(
        parsed.get("apiKey").unwrap().as_str().unwrap(),
        api_key,
        "Claude Code config apiKey should match provider api_key"
    );
}

#[test]
fn test_preset_creation_activation_codex_config() {
    let (_tmp, path) = setup_temp_store();

    // Step 1: Create a provider from the "official" preset for "codex" app (OpenAI)
    let api_key = "sk-openai-test-key-abc";
    let entry = create_from_preset_at("codex", "official", api_key, &path).unwrap();

    // Verify preset fields are filled (Official for codex = OpenAI)
    assert_eq!(entry.preset_id.as_deref(), Some("official"));
    assert_eq!(entry.icon_color.as_deref(), Some("#10A37F"));
    assert_eq!(
        entry.api_key_url.as_deref(),
        Some("https://platform.openai.com/api-keys")
    );
    assert_eq!(entry.name, "Official (OpenAI)");

    // Step 2: Parse settings_config and verify
    let settings: ProviderSettings =
        serde_json::from_value(entry.settings_config.clone()).unwrap();
    assert_eq!(settings.base_url, "https://api.openai.com/v1");
    assert_eq!(settings.api_key, api_key);
    assert!(settings.models.len() >= 2, "Should have at least 2 models");

    // Step 3: Generate Codex TOML config and verify
    let toml_str = generate_codex_config(&settings).unwrap();
    let parsed: toml::Table = toml::from_str(&toml_str).unwrap();
    let provider_section = parsed.get("provider").unwrap().as_table().unwrap();

    assert_eq!(
        provider_section.get("base_url").unwrap().as_str().unwrap(),
        "https://api.openai.com/v1",
        "Codex config base_url should match provider base_url"
    );
    assert_eq!(
        provider_section.get("api_key").unwrap().as_str().unwrap(),
        api_key,
        "Codex config api_key should match provider api_key"
    );
}

#[test]
fn test_preset_creation_anthropic_for_claude() {
    let (_tmp, path) = setup_temp_store();

    // Create from "official" preset for "claude" app (Anthropic)
    let api_key = "sk-ant-test-key-xyz";
    let entry = create_from_preset_at("claude", "official", api_key, &path).unwrap();

    // Verify it's the Anthropic preset
    assert_eq!(entry.name, "Official (Anthropic)");
    assert_eq!(entry.preset_id.as_deref(), Some("official"));
    assert_eq!(entry.icon_color.as_deref(), Some("#D97757"));

    let settings: ProviderSettings =
        serde_json::from_value(entry.settings_config.clone()).unwrap();
    assert_eq!(settings.base_url, "https://api.anthropic.com");
    assert_eq!(settings.api_key, api_key);

    // Generate Claude Code config
    let json_str = generate_claude_code_config(&settings).unwrap();
    let parsed: HashMap<String, Value> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(
        parsed.get("apiUrl").unwrap().as_str().unwrap(),
        "https://api.anthropic.com"
    );
    assert_eq!(
        parsed.get("apiKey").unwrap().as_str().unwrap(),
        api_key
    );
}

#[test]
fn test_preset_creation_kimi_with_both_configs() {
    let (_tmp, path) = setup_temp_store();

    // Create from "kimi" preset
    let api_key = "kimi-test-key-12345";
    let entry = create_from_preset_at("claude", "kimi", api_key, &path).unwrap();

    assert_eq!(entry.preset_id.as_deref(), Some("kimi"));
    assert_eq!(entry.icon_color.as_deref(), Some("#5B45E0"));

    let settings: ProviderSettings =
        serde_json::from_value(entry.settings_config.clone()).unwrap();
    assert_eq!(settings.base_url, "https://api.moonshot.cn/v1");
    assert_eq!(settings.api_key, api_key);
    assert_eq!(settings.models.len(), 2);
    assert_eq!(settings.models[0].source_model, "moonshot-v1-128k");
    assert_eq!(settings.models[1].source_model, "moonshot-v1-32k");

    // Verify Claude Code config generation
    let json_str = generate_claude_code_config(&settings).unwrap();
    let parsed: HashMap<String, Value> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(
        parsed.get("apiUrl").unwrap().as_str().unwrap(),
        "https://api.moonshot.cn/v1"
    );
    assert_eq!(
        parsed.get("apiKey").unwrap().as_str().unwrap(),
        api_key
    );

    // Verify Codex config generation (kimi can be used with any AppId)
    let toml_str = generate_codex_config(&settings).unwrap();
    let parsed: toml::Table = toml::from_str(&toml_str).unwrap();
    let provider_section = parsed.get("provider").unwrap().as_table().unwrap();
    assert_eq!(
        provider_section.get("base_url").unwrap().as_str().unwrap(),
        "https://api.moonshot.cn/v1"
    );
    assert_eq!(
        provider_section.get("api_key").unwrap().as_str().unwrap(),
        api_key
    );
}

#[test]
fn test_full_flow_create_switch_verify_config_generation() {
    let (_tmp, path) = setup_temp_store();

    // Create two providers from different presets
    let key1 = "sk-deepseek-key-111";
    let key2 = "sk-glm-key-222";

    let entry1 = create_from_preset_at("claude", "deepseek", key1, &path).unwrap();
    let entry2 = create_from_preset_at("claude", "glm", key2, &path).unwrap();

    // First provider is auto-activated
    let store = read_store_from(&path).unwrap();
    assert_eq!(store.claude.current.as_deref(), Some(entry1.id.as_str()));

    // Switch to second provider
    switch_active_provider_at("claude", &entry2.id, &path).unwrap();
    let store = read_store_from(&path).unwrap();
    assert_eq!(store.claude.current.as_deref(), Some(entry2.id.as_str()));

    // Verify the active provider's config can be generated correctly
    let active_entry = store.claude.providers.get(&entry2.id).unwrap();
    let settings: ProviderSettings =
        serde_json::from_value(active_entry.settings_config.clone()).unwrap();

    // GLM preset values
    assert_eq!(settings.base_url, "https://open.bigmodel.cn/api/paas/v4");
    assert_eq!(settings.api_key, key2);

    // Generate Claude Code config for the active provider
    let json_str = generate_claude_code_config(&settings).unwrap();
    let parsed: HashMap<String, Value> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(
        parsed.get("apiUrl").unwrap().as_str().unwrap(),
        "https://open.bigmodel.cn/api/paas/v4"
    );
    assert_eq!(
        parsed.get("apiKey").unwrap().as_str().unwrap(),
        key2
    );

    // Generate Codex config for the active provider
    let toml_str = generate_codex_config(&settings).unwrap();
    let parsed: toml::Table = toml::from_str(&toml_str).unwrap();
    let provider_section = parsed.get("provider").unwrap().as_table().unwrap();
    assert_eq!(
        provider_section.get("base_url").unwrap().as_str().unwrap(),
        "https://open.bigmodel.cn/api/paas/v4"
    );
    assert_eq!(
        provider_section.get("api_key").unwrap().as_str().unwrap(),
        key2
    );
}

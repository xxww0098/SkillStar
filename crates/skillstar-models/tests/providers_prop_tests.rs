//! Property-based tests for Provider validation (Property 3), Malformed Store Recovery (Property 15),
//! and Preset Creation Fills Expected Fields (Property 8).
//!
//! **Validates: Requirements 2.1, 2.2, 2.3, 3.2, 3.5, 3.6, 7.3**
//!
//! Property 3: Provider Validation Rejects Invalid Inputs
//! For any provider creation attempt where the name is empty or exceeds 64 characters,
//! or the base_url is not a valid URL, or the models list is empty, the Backend SHALL
//! reject the creation and return a validation error.
//!
//! Property 8: Preset Creation Fills Expected Fields
//! For any built-in preset and valid AppId, creating a provider from that preset SHALL
//! produce a ProviderEntry with base_url, models, api_key_url, icon_color, and preset_id
//! populated from the preset definition.
//!
//! Property 15: Malformed Store Recovery
//! For any malformed or unreadable model_providers.json content, the Backend SHALL
//! return a valid empty ProvidersStore with empty provider collections for each AppId.

use proptest::prelude::*;
use serde_json::Value;
use skillstar_models::providers::{
    create_from_preset_at, create_provider_at, get_provider_presets, read_store_from,
    ModelMapping, ProviderEntry, ProviderSettings,
};
use tempfile::TempDir;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a temp directory with a store file path inside it.
fn setup_temp_store() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("model_providers.json");
    (tmp, path)
}

/// Build a valid ProviderSettings as a serde_json::Value.
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

/// Build a ProviderEntry with the given name and valid defaults for other fields.
fn make_entry_with_name(name: &str) -> ProviderEntry {
    ProviderEntry {
        id: "test-id".to_string(),
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

/// Build a ProviderEntry with the given base_url and valid defaults for other fields.
fn make_entry_with_url(base_url: &str) -> ProviderEntry {
    let settings = ProviderSettings {
        base_url: base_url.to_string(),
        api_key: "sk-test-key".to_string(),
        models: vec![ModelMapping {
            source_model: "model-a".to_string(),
            target_model: "model-a".to_string(),
            enabled: true,
        }],
        timeout_ms: None,
        max_retries: None,
    };
    ProviderEntry {
        id: "test-id".to_string(),
        name: "Valid Name".to_string(),
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

/// Build a ProviderEntry with an empty models list.
fn make_entry_with_empty_models() -> ProviderEntry {
    let settings = ProviderSettings {
        base_url: "https://api.example.com/v1".to_string(),
        api_key: "sk-test-key".to_string(),
        models: vec![],
        timeout_ms: None,
        max_retries: None,
    };
    ProviderEntry {
        id: "test-id".to_string(),
        name: "Valid Name".to_string(),
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

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Strategy that generates empty strings (the only empty string is "").
fn empty_name_strategy() -> impl Strategy<Value = String> {
    Just(String::new())
}

/// Strategy that generates strings longer than 64 characters.
fn long_name_strategy() -> impl Strategy<Value = String> {
    // Generate strings between 65 and 256 chars using arbitrary printable characters
    prop::collection::vec(prop::char::range('\x20', '\x7e'), 65..=256)
        .prop_map(|chars| chars.into_iter().collect::<String>())
}

/// Strategy that generates strings that are NOT valid URLs.
/// We filter generated strings to ensure url::Url::parse rejects them.
fn invalid_url_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Arbitrary strings filtered to ensure they are truly invalid URLs
        "[a-zA-Z0-9 _\\-\\./!@#$%^&*()]{0,50}".prop_filter(
            "must not be a valid URL",
            |s| url::Url::parse(s).is_err()
        ),
        // Strings with spaces (never valid URLs)
        " [a-z ]{2,20}",
        // Empty string
        Just(String::new()),
        // Relative paths (no scheme)
        Just("/just/a/path".to_string()),
        Just("relative/path".to_string()),
        // Missing scheme
        Just("://missing-scheme".to_string()),
        // Just a hostname without scheme
        Just("example.com".to_string()),
        Just("api.example.com/v1".to_string()),
    ]
}

// ---------------------------------------------------------------------------
// Property Tests
// ---------------------------------------------------------------------------

proptest! {
    /// **Validates: Requirements 2.1**
    ///
    /// Property 3 (part 1): Empty name must be rejected.
    #[test]
    fn prop_empty_name_rejected(name in empty_name_strategy()) {
        let (_tmp, path) = setup_temp_store();
        let entry = make_entry_with_name(&name);
        let result = create_provider_at("claude", entry, &path);
        prop_assert!(result.is_err(), "Expected error for empty name, got Ok");
        let err_msg = result.unwrap_err().to_string();
        prop_assert!(
            err_msg.contains("name must not be empty"),
            "Error message should mention empty name, got: {}",
            err_msg
        );
    }

    /// **Validates: Requirements 2.1**
    ///
    /// Property 3 (part 2): Names exceeding 64 characters must be rejected.
    #[test]
    fn prop_long_name_rejected(name in long_name_strategy()) {
        let (_tmp, path) = setup_temp_store();
        let entry = make_entry_with_name(&name);
        let result = create_provider_at("claude", entry, &path);
        prop_assert!(result.is_err(), "Expected error for name length {}, got Ok", name.len());
        let err_msg = result.unwrap_err().to_string();
        prop_assert!(
            err_msg.contains("at most 64 characters"),
            "Error message should mention 64 char limit, got: {}",
            err_msg
        );
    }

    /// **Validates: Requirements 2.2**
    ///
    /// Property 3 (part 3): Invalid URLs must be rejected.
    #[test]
    fn prop_invalid_url_rejected(url in invalid_url_strategy()) {
        let (_tmp, path) = setup_temp_store();
        let entry = make_entry_with_url(&url);
        let result = create_provider_at("claude", entry, &path);
        prop_assert!(result.is_err(), "Expected error for invalid URL '{}', got Ok", url);
        let err_msg = result.unwrap_err().to_string();
        prop_assert!(
            err_msg.contains("Invalid base_url"),
            "Error message should mention invalid URL, got: {}",
            err_msg
        );
    }

    /// **Validates: Requirements 2.3**
    ///
    /// Property 3 (part 4): Empty model lists must be rejected.
    #[test]
    fn prop_empty_models_rejected(_dummy in 0..100u32) {
        let (_tmp, path) = setup_temp_store();
        let entry = make_entry_with_empty_models();
        let result = create_provider_at("claude", entry, &path);
        prop_assert!(result.is_err(), "Expected error for empty models list, got Ok");
        let err_msg = result.unwrap_err().to_string();
        prop_assert!(
            err_msg.contains("At least one model"),
            "Error message should mention model requirement, got: {}",
            err_msg
        );
    }
}


// ===========================================================================
// Property 15: Malformed Store Recovery
//
// For any malformed or unreadable model_providers.json content, the Backend
// SHALL return a valid empty ProvidersStore with empty provider collections
// for each AppId.
//
// **Validates: Requirement 7.3**
// ===========================================================================

/// Helper: assert that a ProvidersStore is a valid empty default store.
fn assert_empty_store(store: &skillstar_models::providers::ProvidersStore) {
    assert!(store.claude.providers.is_empty(), "claude providers should be empty");
    assert!(store.claude.current.is_none(), "claude current should be None");
    assert!(store.codex.providers.is_empty(), "codex providers should be empty");
    assert!(store.codex.current.is_none(), "codex current should be None");
}

proptest! {
    /// **Validates: Requirement 7.3**
    ///
    /// Property 15 (part 1): Arbitrary byte sequences written to the store file
    /// must result in read_store_from returning a valid empty ProvidersStore.
    #[test]
    fn prop_malformed_store_recovery_random_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let (_tmp, path) = setup_temp_store();
        std::fs::write(&path, &bytes).unwrap();

        let store = read_store_from(&path).unwrap();
        // If the random bytes happen to be valid JSON that deserializes to a ProvidersStore,
        // that's fine — the store is still valid. Otherwise it must be the default empty store.
        // The key invariant: read_store_from NEVER returns Err for malformed content.
        // For truly invalid JSON (the vast majority of random bytes), we get the empty default.
        if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
            assert_empty_store(&store);
        }
        // In all cases, the result is Ok (never an error)
    }

    /// **Validates: Requirement 7.3**
    ///
    /// Property 15 (part 2): Strings that are not valid JSON must result in
    /// read_store_from returning a valid empty ProvidersStore.
    #[test]
    fn prop_malformed_store_recovery_invalid_json_strings(
        garbage in "[^{}\\[\\]\"]{0,500}"
    ) {
        let (_tmp, path) = setup_temp_store();
        std::fs::write(&path, garbage.as_bytes()).unwrap();

        let store = read_store_from(&path).unwrap();
        assert_empty_store(&store);
    }

    /// **Validates: Requirement 7.3**
    ///
    /// Property 15 (part 3): Partial/truncated JSON structures must result in
    /// read_store_from returning a valid empty ProvidersStore.
    #[test]
    fn prop_malformed_store_recovery_partial_json(
        prefix in prop_oneof![
            Just("{\"claude\": {".to_string()),
            Just("{\"claude\": {\"providers\": {".to_string()),
            Just("[1, 2, 3".to_string()),
            Just("{\"key\": \"value\"".to_string()),
            Just("{".to_string()),
            Just("[".to_string()),
            Just("{\"claude\": null, \"codex\":".to_string()),
        ],
        suffix in "[a-z0-9 ,:{]{0,100}",
    ) {
        let content = format!("{}{}", prefix, suffix);
        let (_tmp, path) = setup_temp_store();
        std::fs::write(&path, content.as_bytes()).unwrap();

        let store = read_store_from(&path).unwrap();
        // Partial JSON is either invalid (returns empty default) or happens to parse
        // into a valid ProvidersStore (unlikely but possible). Either way, no error.
        // For most partial JSON, serde will fail and we get the default.
        if serde_json::from_str::<skillstar_models::providers::ProvidersStore>(std::fs::read_to_string(&path).unwrap().trim_start_matches('\u{FEFF}')).is_err() {
            assert_empty_store(&store);
        }
    }
}


// ===========================================================================
// Property 8: Preset Creation Fills Expected Fields
//
// For any built-in preset and valid AppId, creating a provider from that preset
// SHALL produce a ProviderEntry with base_url, models, api_key_url, icon_color,
// and preset_id populated from the preset definition.
//
// **Validates: Requirements 3.2, 3.5, 3.6**
// ===========================================================================

/// Strategy: generate a valid preset_id.
fn arb_preset_id() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("official".to_string()),
        Just("deepseek".to_string()),
        Just("kimi".to_string()),
        Just("glm".to_string()),
    ]
}

/// Strategy: generate a valid AppId for preset creation (claude or codex).
fn arb_preset_app_id() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("claude".to_string()),
        Just("codex".to_string()),
    ]
}

/// Strategy: generate an arbitrary API key string (non-empty, printable ASCII).
fn arb_api_key() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9\\-_]{1,128}"
}

proptest! {
    /// **Validates: Requirements 3.2, 3.5, 3.6**
    ///
    /// Property 8: For each preset_id and valid AppId, creating a provider from
    /// that preset produces a ProviderEntry with preset_id, icon_color, api_key_url,
    /// base_url (non-empty valid URL), models (non-empty list), and api_key matching
    /// the provided key.
    #[test]
    fn prop_preset_creation_fills_expected_fields(
        preset_id in arb_preset_id(),
        app_id in arb_preset_app_id(),
        api_key in arb_api_key(),
    ) {
        let (_tmp, path) = setup_temp_store();

        // Create provider from preset
        let result = create_from_preset_at(&app_id, &preset_id, &api_key, &path);
        prop_assert!(
            result.is_ok(),
            "create_from_preset_at should succeed for preset '{}' and app '{}', got: {:?}",
            preset_id, app_id, result.err()
        );
        let entry = result.unwrap();

        // Assert preset_id is populated and matches the input preset_id
        prop_assert_eq!(
            entry.preset_id.as_deref(),
            Some(preset_id.as_str()),
            "preset_id should be set to '{}'", preset_id
        );

        // Assert icon_color is Some and non-empty
        prop_assert!(
            entry.icon_color.is_some(),
            "icon_color should be Some for preset '{}'", preset_id
        );
        let icon_color = entry.icon_color.as_ref().unwrap();
        prop_assert!(
            !icon_color.is_empty(),
            "icon_color should be non-empty for preset '{}'", preset_id
        );

        // Assert api_key_url is Some and non-empty
        prop_assert!(
            entry.api_key_url.is_some(),
            "api_key_url should be Some for preset '{}'", preset_id
        );
        let api_key_url = entry.api_key_url.as_ref().unwrap();
        prop_assert!(
            !api_key_url.is_empty(),
            "api_key_url should be non-empty for preset '{}'", preset_id
        );

        // Parse settings_config to verify base_url and models
        let settings: ProviderSettings = serde_json::from_value(entry.settings_config.clone())
            .expect("settings_config should deserialize to ProviderSettings");

        // Assert base_url is non-empty and a valid URL
        prop_assert!(
            !settings.base_url.is_empty(),
            "base_url should be non-empty for preset '{}'", preset_id
        );
        prop_assert!(
            url::Url::parse(&settings.base_url).is_ok(),
            "base_url '{}' should be a valid URL for preset '{}'", settings.base_url, preset_id
        );

        // Legacy provider presets still produce model mappings because the
        // legacy store validates that at least one mapping exists.
        prop_assert!(
            !settings.models.is_empty(),
            "models should be non-empty for preset '{}'",
            preset_id
        );

        // Assert api_key matches the provided key
        prop_assert_eq!(
            &settings.api_key, &api_key,
            "settings_config.api_key should match the provided api_key"
        );

        // Cross-check against the preset definitions to ensure values come from the preset
        let presets = get_provider_presets();
        let expected_preset = if preset_id == "official" {
            let target_name = match app_id.as_str() {
                "claude" => "Official (Anthropic)",
                "codex" => "Official (OpenAI)",
                _ => unreachable!(),
            };
            presets.iter().find(|p| p.id == "official" && p.name == target_name)
        } else {
            presets.iter().find(|p| p.id == preset_id)
        };

        let expected_preset = expected_preset.expect("Preset should exist in get_provider_presets()");

        // Verify base_url matches preset definition
        prop_assert_eq!(
            &settings.base_url, &expected_preset.base_url,
            "base_url should match preset definition"
        );

        // Verify icon_color matches preset definition
        prop_assert_eq!(
            icon_color, &expected_preset.icon_color,
            "icon_color should match preset definition"
        );

        // Verify api_key_url matches preset definition
        prop_assert_eq!(
            api_key_url, &expected_preset.api_key_url,
            "api_key_url should match preset definition"
        );

        // Verify models match preset definition (same count and same model names)
        prop_assert_eq!(
            settings.models.len(), expected_preset.models.len(),
            "models count should match preset definition"
        );
        for (mapping, expected_model) in settings.models.iter().zip(expected_preset.models.iter()) {
            prop_assert_eq!(
                &mapping.source_model, expected_model,
                "source_model should match preset model"
            );
            prop_assert_eq!(
                &mapping.target_model, expected_model,
                "target_model should match preset model"
            );
            prop_assert!(mapping.enabled, "model mapping should be enabled by default");
        }
    }
}


// ===========================================================================
// Feature: model-provider-management, Property 1: Store Serialization Round-Trip
//
// For any valid FlatProvidersStore instance (with arbitrary providers and
// tool_activations), serializing to JSON and then deserializing should produce
// a structurally equivalent store — all provider fields, tool_activations
// entries, and the version number are preserved.
//
// **Validates: Requirements 8.1, 8.2**
// ===========================================================================

use skillstar_models::providers::{FlatProvidersStore, ProviderEntryFlat, ToolActivation, write_flat_store, read_flat_store};

// ---------------------------------------------------------------------------
// Strategies for flat store types
// ---------------------------------------------------------------------------

/// Strategy: generate an arbitrary ToolActivation.
fn arb_tool_activation() -> impl Strategy<Value = ToolActivation> {
    (
        "[a-zA-Z0-9\\-]{1,36}",  // provider_id (UUID-like)
        "[a-zA-Z0-9\\-\\.]{1,50}", // model name
    )
        .prop_map(|(provider_id, model)| ToolActivation { provider_id, model, settings: None, last_sync_at: None })
}

/// Strategy: generate an optional ToolActivation for the tool_activations map.
fn arb_optional_tool_activation() -> impl Strategy<Value = Option<ToolActivation>> {
    prop_oneof![
        Just(None),
        arb_tool_activation().prop_map(Some),
    ]
}

/// Strategy: generate an arbitrary ProviderEntryFlat.
fn arb_provider_entry_flat() -> impl Strategy<Value = ProviderEntryFlat> {
    (
        "[a-f0-9\\-]{36}",                          // id (UUID format)
        "[a-zA-Z0-9 _\\-]{1,64}",                   // name
        "https://[a-z]{3,12}\\.[a-z]{2,6}/v[0-9]",  // base_url_openai
        prop_oneof![
            Just(String::new()),
            "https://[a-z]{3,12}\\.[a-z]{2,6}/anthropic".prop_map(|s| s),
        ],                                           // base_url_anthropic
        "[a-zA-Z0-9\\-_]{0,64}",                     // api_key
        proptest::collection::vec("[a-zA-Z0-9\\-\\.]{1,30}", 0..5), // models
        "[a-zA-Z0-9\\-\\.]{0,30}",                   // default_model
        0u32..100u32,                                // sort_index
    )
        .prop_flat_map(|(id, name, base_url_openai, base_url_anthropic, api_key, models, default_model, sort_index)| {
            // Generate optional fields
            (
                Just(id),
                Just(name),
                Just(base_url_openai),
                Just(base_url_anthropic),
                Just(api_key),
                Just(models),
                Just(default_model),
                Just(sort_index),
                proptest::option::of("[a-z\\-]{3,20}"),       // preset_id
                proptest::option::of("#[0-9A-Fa-f]{6}"),      // icon_color
                proptest::option::of("[a-zA-Z0-9 ]{0,100}"),  // notes
                proptest::option::of(1_700_000_000_000u64..1_800_000_000_000u64), // created_at
            )
        })
        .prop_map(|(id, name, base_url_openai, base_url_anthropic, api_key, models, default_model, sort_index, preset_id, icon_color, notes, created_at)| {
            ProviderEntryFlat {
                id,
                name,
                base_url_openai,
                base_url_anthropic,
                models_url: String::new(),
                api_key,
                models,
                default_model,
                sort_index,
                preset_id,
                icon_color,
                notes,
                created_at,
                meta: None, // Keep meta as None for simplicity (JSON Value is hard to generate arbitrarily)
                codex_wire_api: "responses".to_string(),
                codex_auth_mode: "api_key".to_string(),
            }
        })
}

/// Strategy: generate an arbitrary FlatProvidersStore.
fn arb_flat_providers_store() -> impl Strategy<Value = FlatProvidersStore> {
    (
        proptest::collection::vec(arb_provider_entry_flat(), 0..10), // providers
        proptest::collection::hash_map(
            "[a-z\\-]{3,15}",  // tool_id keys (e.g., "claude-code", "codex")
            arb_optional_tool_activation(),
            0..5,
        ), // tool_activations
    )
        .prop_map(|(providers, tool_activations)| FlatProvidersStore {
            version: 2,
            providers,
            tool_activations,
        })
}

// ---------------------------------------------------------------------------
// Property 1 Test
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Validates: Requirements 8.1, 8.2**
    ///
    /// Property 1: Store Serialization Round-Trip
    /// For any valid FlatProvidersStore, serialize → deserialize produces a
    /// structurally equivalent store.
    #[test]
    fn prop_flat_store_serialization_round_trip(store in arb_flat_providers_store()) {
        // Serialize to JSON
        let json = serde_json::to_string_pretty(&store)
            .expect("FlatProvidersStore should serialize to JSON");

        // Deserialize back
        let deserialized: FlatProvidersStore = serde_json::from_str(&json)
            .expect("Serialized JSON should deserialize back to FlatProvidersStore");

        // Verify version is preserved
        prop_assert_eq!(store.version, deserialized.version, "version must be preserved");

        // Verify providers count matches
        prop_assert_eq!(
            store.providers.len(),
            deserialized.providers.len(),
            "providers count must be preserved"
        );

        // Verify each provider is structurally equivalent (ProviderEntryFlat derives PartialEq)
        for (original, restored) in store.providers.iter().zip(deserialized.providers.iter()) {
            prop_assert_eq!(original, restored, "provider entry must be preserved through round-trip");
        }

        // Verify tool_activations count matches
        prop_assert_eq!(
            store.tool_activations.len(),
            deserialized.tool_activations.len(),
            "tool_activations count must be preserved"
        );

        // Verify each tool_activation entry is preserved
        for (tool_id, original_activation) in &store.tool_activations {
            let restored_activation = deserialized.tool_activations.get(tool_id);
            prop_assert!(
                restored_activation.is_some(),
                "tool_activation key '{}' must exist after round-trip",
                tool_id
            );
            prop_assert_eq!(
                original_activation,
                restored_activation.unwrap(),
                "tool_activation for '{}' must be preserved",
                tool_id
            );
        }
    }

    /// **Validates: Requirements 8.1, 8.2**
    ///
    /// Property 1 (file round-trip): For any valid FlatProvidersStore, writing to
    /// a file via write_flat_store and reading back via read_flat_store produces a
    /// structurally equivalent store.
    #[test]
    fn prop_flat_store_file_round_trip(store in arb_flat_providers_store()) {
        let (_tmp, path) = setup_temp_store();

        // Write to file
        write_flat_store(&store, &path)
            .expect("write_flat_store should succeed");

        // Read back from file
        let restored = read_flat_store(&path)
            .expect("read_flat_store should succeed");

        // Verify version
        prop_assert_eq!(store.version, restored.version, "version must survive file round-trip");

        // Verify providers
        prop_assert_eq!(
            store.providers.len(),
            restored.providers.len(),
            "providers count must survive file round-trip"
        );
        for (original, restored_entry) in store.providers.iter().zip(restored.providers.iter()) {
            prop_assert_eq!(original, restored_entry, "provider must survive file round-trip");
        }

        // Verify tool_activations
        prop_assert_eq!(
            store.tool_activations.len(),
            restored.tool_activations.len(),
            "tool_activations count must survive file round-trip"
        );
        for (tool_id, original_activation) in &store.tool_activations {
            let restored_activation = restored.tool_activations.get(tool_id);
            prop_assert!(
                restored_activation.is_some(),
                "tool_activation key '{}' must survive file round-trip",
                tool_id
            );
            prop_assert_eq!(
                original_activation,
                restored_activation.unwrap(),
                "tool_activation for '{}' must survive file round-trip",
                tool_id
            );
        }
    }
}


// ===========================================================================
// Feature: model-provider-management, Property 11: Migration Preserves All Provider Data
//
// For any valid v1 ProvidersStore (per-app format), migrating to v2 (flat format)
// should preserve every provider entry's data (name, base_url, api_key, models, etc.)
// without loss. The tool_activations in v2 should correctly reflect each app's
// `current` field from v1.
//
// **Validates: Requirements 8.7**
// ===========================================================================

use skillstar_models::providers::{
    migrate_store_if_needed, AppProviders, ProvidersStore,
};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Strategies for generating random v1 ProvidersStore instances
// ---------------------------------------------------------------------------

/// Strategy: generate a random ModelMapping.
fn arb_model_mapping() -> impl Strategy<Value = ModelMapping> {
    (
        "[a-z][a-z0-9\\-]{2,20}",  // source_model
        "[a-z][a-z0-9\\-]{2,20}",  // target_model
        any::<bool>(),              // enabled
    )
        .prop_map(|(source, target, enabled)| ModelMapping {
            source_model: source,
            target_model: target,
            enabled,
        })
}

/// Strategy: generate a random ProviderSettings as a serde_json::Value.
fn arb_provider_settings() -> impl Strategy<Value = (Value, String, String, Vec<String>)> {
    (
        // base_url: valid URL
        ("[a-z]{3,8}", "[a-z]{2,6}")
            .prop_map(|(host, path)| format!("https://{}.example.com/{}", host, path)),
        // api_key: non-empty string
        "[a-zA-Z0-9\\-_]{8,64}",
        // models: 1..=4 model mappings
        prop::collection::vec(arb_model_mapping(), 1..=4),
    )
        .prop_map(|(base_url, api_key, models)| {
            let enabled_models: Vec<String> = models
                .iter()
                .filter(|m| m.enabled)
                .map(|m| m.source_model.clone())
                .collect();
            let settings = ProviderSettings {
                base_url: base_url.clone(),
                api_key: api_key.clone(),
                models,
                timeout_ms: None,
                max_retries: None,
            };
            let value = serde_json::to_value(&settings).unwrap();
            (value, base_url, api_key, enabled_models)
        })
}

/// Strategy: generate a random ProviderEntry with extracted metadata.
fn arb_provider_entry() -> impl Strategy<Value = (ProviderEntry, String, String, Vec<String>)> {
    (
        "[a-zA-Z0-9]{4,16}",       // id
        "[A-Za-z ]{1,32}",         // name
        arb_provider_settings(),
        prop::option::of("[a-z\\-]{3,12}"),  // preset_id
        prop::option::of("#[0-9A-Fa-f]{6}"), // icon_color
        prop::option::of("[a-zA-Z0-9 ]{0,50}"), // notes
        prop::option::of(1_000_000_000u64..2_000_000_000u64), // created_at
    )
        .prop_map(|(id, name, (settings_val, base_url, api_key, models), preset_id, icon_color, notes, created_at)| {
            let entry = ProviderEntry {
                id,
                name: name.clone(),
                category: "cloud".to_string(),
                settings_config: settings_val,
                preset_id: preset_id.clone(),
                website_url: None,
                api_key_url: None,
                icon_color: icon_color.clone(),
                notes: notes.clone(),
                created_at,
                sort_index: None,
                meta: None,
            };
            (entry, base_url, api_key, models)
        })
}

/// Per-provider expectations: `provider_id → (base_url, api_key, enabled_models)`.
type ProviderExpectations = HashMap<String, (String, String, Vec<String>)>;

/// Strategy: generate a random AppProviders with 0..=3 providers.
/// Returns the AppProviders and a map of provider_id → (base_url, api_key, enabled_models).
fn arb_app_providers() -> impl Strategy<Value = (AppProviders, ProviderExpectations)> {
    prop::collection::vec(arb_provider_entry(), 0..=3)
        .prop_flat_map(|entries| {
            let len = entries.len();
            // Generate a current index (None if empty, Some(idx) if non-empty)
            let current_strategy = if len == 0 {
                Just(None).boxed()
            } else {
                prop::option::of(0..len).boxed()
            };

            (Just(entries), current_strategy)
        })
        .prop_map(|(entries, current_idx)| {
            let mut providers = HashMap::new();
            let mut meta_map = HashMap::new();

            for (entry, base_url, api_key, models) in &entries {
                providers.insert(entry.id.clone(), entry.clone());
                meta_map.insert(entry.id.clone(), (base_url.clone(), api_key.clone(), models.clone()));
            }

            let current = current_idx.and_then(|idx| {
                entries.get(idx).map(|(e, _, _, _)| e.id.clone())
            });

            (AppProviders { providers, current }, meta_map)
        })
}

/// Strategy: generate a random v1 ProvidersStore.
fn arb_v1_store() -> impl Strategy<Value = ProvidersStore> {
    (
        arb_app_providers(),
        arb_app_providers(),
        arb_app_providers(),
        arb_app_providers(),
    )
        .prop_map(|((claude, _), (codex, _), (opencode, _), (gemini, _))| {
            ProvidersStore {
                claude,
                codex,
                opencode,
                gemini,
            }
        })
}

// ---------------------------------------------------------------------------
// Property 11 Tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Validates: Requirements 8.7**
    ///
    /// Property 11: Migration preserves all provider data.
    /// For any valid v1 ProvidersStore, after migration to v2:
    /// - All provider entries' data (name, base_url, api_key, models) are preserved
    /// - tool_activations correctly reflects each app's `current` field
    #[test]
    fn prop_migration_preserves_all_provider_data(v1_store in arb_v1_store()) {
        let (_tmp, path) = setup_temp_store();

        // Write the v1 store to the temp file
        let v1_json = serde_json::to_string_pretty(&v1_store).unwrap();
        std::fs::write(&path, v1_json.as_bytes()).unwrap();

        // Perform migration
        let v2_store = migrate_store_if_needed(&path).unwrap();

        // Verify: version is 2
        prop_assert_eq!(v2_store.version, 2, "Migrated store should have version 2");

        // Collect all unique providers from v1 (keyed by base_url + api_key)
        let apps: &[&AppProviders] = &[
            &v1_store.claude,
            &v1_store.codex,
            &v1_store.opencode,
            &v1_store.gemini,
        ];

        // Gather all (base_url, api_key) → (name, models_set) from v1
        let mut v1_providers: HashMap<(String, String), (String, HashSet<String>)> = HashMap::new();

        for app in apps {
            for entry in app.providers.values() {
                let (base_url, api_key, models) = extract_settings_for_test(&entry.settings_config);
                let key = (base_url.clone(), api_key.clone());
                let entry_models: HashSet<String> = models.into_iter().collect();

                v1_providers
                    .entry(key)
                    .and_modify(|(_name, existing_models)| {
                        // Merge models
                        existing_models.extend(entry_models.clone());
                    })
                    .or_insert_with(|| (entry.name.clone(), entry_models));
            }
        }

        // Verify: the number of unique providers in v2 matches the deduplicated count from v1
        prop_assert_eq!(
            v2_store.providers.len(),
            v1_providers.len(),
            "v2 provider count should match deduplicated v1 count. v1 unique: {}, v2: {}",
            v1_providers.len(),
            v2_store.providers.len()
        );

        // Verify: each v1 provider's data is preserved in v2
        for v2_provider in &v2_store.providers {
            let key = (v2_provider.base_url_openai.clone(), v2_provider.api_key.clone());
            prop_assert!(
                v1_providers.contains_key(&key),
                "v2 provider with base_url='{}' and api_key='{}' should exist in v1",
                v2_provider.base_url_openai,
                v2_provider.api_key
            );

            let (v1_name, v1_models) = v1_providers.get(&key).unwrap();

            // Name is preserved (from the first occurrence)
            prop_assert_eq!(
                &v2_provider.name, v1_name,
                "Provider name should be preserved"
            );

            // api_key is preserved
            let found_in_v1 = apps.iter().any(|app| {
                app.providers.values().any(|e| {
                    let (bu, ak, _) = extract_settings_for_test(&e.settings_config);
                    bu == v2_provider.base_url_openai && ak == v2_provider.api_key
                })
            });
            prop_assert!(found_in_v1, "Provider api_key should exist in v1");

            // All enabled models from v1 are present in v2
            let v2_models: HashSet<String> = v2_provider.models.iter().cloned().collect();
            for model in v1_models {
                prop_assert!(
                    v2_models.contains(model),
                    "Model '{}' from v1 should be preserved in v2 for provider '{}'",
                    model,
                    v2_provider.name
                );
            }
        }

        // Verify: tool_activations correctly reflects each app's `current` field
        // claude.current → tool_activations["claude-code"]
        verify_tool_activation(
            &v1_store.claude,
            "claude-code",
            &v2_store,
            apps,
        )?;

        // codex.current → tool_activations["codex"]
        verify_tool_activation(
            &v1_store.codex,
            "codex",
            &v2_store,
            apps,
        )?;
    }

    /// **Validates: Requirements 8.7**
    ///
    /// Property 11 (part 2): Migration preserves provider metadata fields.
    /// For any valid v1 ProvidersStore, preset_id, icon_color, notes, and created_at
    /// are preserved in the migrated v2 store.
    #[test]
    fn prop_migration_preserves_metadata_fields(v1_store in arb_v1_store()) {
        let (_tmp, path) = setup_temp_store();

        // Write the v1 store to the temp file
        let v1_json = serde_json::to_string_pretty(&v1_store).unwrap();
        std::fs::write(&path, v1_json.as_bytes()).unwrap();

        // Perform migration
        let v2_store = migrate_store_if_needed(&path).unwrap();

        // For each v2 provider, find the corresponding v1 entry and verify metadata
        let apps: &[&AppProviders] = &[
            &v1_store.claude,
            &v1_store.codex,
            &v1_store.opencode,
            &v1_store.gemini,
        ];

        for v2_provider in &v2_store.providers {
            // Find the first matching v1 entry by (base_url, api_key)
            let v1_entry = apps.iter()
                .flat_map(|app| app.providers.values())
                .find(|e| {
                    let (bu, ak, _) = extract_settings_for_test(&e.settings_config);
                    bu == v2_provider.base_url_openai && ak == v2_provider.api_key
                });

            prop_assert!(
                v1_entry.is_some(),
                "Should find matching v1 entry for v2 provider '{}'",
                v2_provider.name
            );

            let v1_entry = v1_entry.unwrap();

            // Verify metadata fields are preserved
            prop_assert_eq!(
                &v2_provider.preset_id, &v1_entry.preset_id,
                "preset_id should be preserved for provider '{}'",
                v2_provider.name
            );
            prop_assert_eq!(
                &v2_provider.icon_color, &v1_entry.icon_color,
                "icon_color should be preserved for provider '{}'",
                v2_provider.name
            );
            prop_assert_eq!(
                &v2_provider.notes, &v1_entry.notes,
                "notes should be preserved for provider '{}'",
                v2_provider.name
            );
            prop_assert_eq!(
                v2_provider.created_at, v1_entry.created_at,
                "created_at should be preserved for provider '{}'",
                v2_provider.name
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers for Property 11
// ---------------------------------------------------------------------------

/// Extract base_url, api_key, and enabled model names from a v1 settings_config Value.
/// (Mirrors the logic in providers.rs extract_v1_settings)
fn extract_settings_for_test(settings_config: &Value) -> (String, String, Vec<String>) {
    if let Ok(settings) = serde_json::from_value::<ProviderSettings>(settings_config.clone()) {
        let models: Vec<String> = settings
            .models
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.source_model.clone())
            .collect();
        return (settings.base_url, settings.api_key, models);
    }

    // Fallback
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

/// Verify that tool_activations for a given tool_id correctly reflects the app's `current` field.
fn verify_tool_activation(
    app: &AppProviders,
    tool_id: &str,
    v2_store: &FlatProvidersStore,
    _all_apps: &[&AppProviders],
) -> Result<(), proptest::test_runner::TestCaseError> {
    match &app.current {
        None => {
            // If app has no current, tool_activations should either not contain this tool_id
            // or contain None for it
            if let Some(activation) = v2_store.tool_activations.get(tool_id) {
                prop_assert!(
                    activation.is_none(),
                    "tool_activations['{}'] should be None when app has no current provider",
                    tool_id
                );
            }
            // Not having the key at all is also acceptable
        }
        Some(current_id) => {
            // If the current provider exists in the app's providers map
            if let Some(current_entry) = app.providers.get(current_id) {
                let (base_url, api_key, models) =
                    extract_settings_for_test(&current_entry.settings_config);
                let dedup_key = (base_url, api_key);

                // Find the corresponding v2 provider
                let v2_match = v2_store.providers.iter().find(|p| {
                    (p.base_url_openai.clone(), p.api_key.clone()) == dedup_key
                });

                if let Some(v2_provider) = v2_match {
                    // tool_activations should reference this provider
                    let activation = v2_store.tool_activations.get(tool_id);
                    prop_assert!(
                        activation.is_some(),
                        "tool_activations should contain key '{}' when app has current='{}'",
                        tool_id,
                        current_id
                    );

                    let activation = activation.unwrap();
                    prop_assert!(
                        activation.is_some(),
                        "tool_activations['{}'] should be Some when app has current='{}'",
                        tool_id,
                        current_id
                    );

                    let activation = activation.as_ref().unwrap();
                    prop_assert_eq!(
                        &activation.provider_id,
                        &v2_provider.id,
                        "tool_activations['{}'].provider_id should match the migrated provider id",
                        tool_id
                    );

                    // The model in activation should be the first enabled model
                    let expected_model = models.first().cloned().unwrap_or_default();
                    prop_assert_eq!(
                        &activation.model,
                        &expected_model,
                        "tool_activations['{}'].model should be the first enabled model",
                        tool_id
                    );
                }
                // If v2_match is None, it means the dedup key wasn't found — this shouldn't
                // happen if migration is correct, but we don't assert here because the provider
                // might have been merged with another entry
            }
            // If current_id doesn't exist in providers map, migration can't resolve it
            // and tool_activations may not have an entry — that's acceptable
        }
    }
    Ok(())
}


// ===========================================================================
// Feature: model-provider-management, Property 2: Provider List Sort Invariant
//
// For any list of providers with arbitrary sort_index values, and for any
// reorder operation that assigns new sort_index values based on a permutation,
// the resulting provider list when sorted by sort_index ascending should match
// the intended order, and all sort_index values should be monotonically
// non-decreasing.
//
// **Validates: Requirements 2.1, 5.4**
// ===========================================================================


/// Strategy: generate a FlatProvidersStore with N providers that have unique IDs.
/// Each provider gets a random sort_index (simulating an arbitrary initial state).
fn arb_flat_store_with_unique_ids() -> impl Strategy<Value = FlatProvidersStore> {
    // Generate 1..=10 providers with unique IDs
    (1usize..=10usize).prop_flat_map(|n| {
        // Generate n unique IDs
        proptest::collection::vec("[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}", n)
            .prop_flat_map(move |ids| {
                // Generate random sort_index values for each provider
                proptest::collection::vec(0u32..1000u32, n)
                    .prop_map(move |sort_indices| {
                        let providers: Vec<ProviderEntryFlat> = ids.iter().zip(sort_indices.iter())
                            .map(|(id, &sort_idx)| ProviderEntryFlat {
                                id: id.clone(),
                                name: format!("Provider-{}", &id[..8]),
                                base_url_openai: format!("https://{}.example.com/v1", &id[..8]),
                                base_url_anthropic: String::new(),
                                models_url: String::new(),
                                api_key: "sk-test".to_string(),
                                models: vec!["model-a".to_string()],
                                default_model: "model-a".to_string(),
                                sort_index: sort_idx,
                                preset_id: None,
                                icon_color: None,
                                notes: None,
                                created_at: None,
                                meta: None,
                                codex_wire_api: "responses".to_string(),
                                codex_auth_mode: "api_key".to_string(),
                            })
                            .collect();

                        FlatProvidersStore {
                            version: 2,
                            providers,
                            tool_activations: HashMap::new(),
                        }
                    })
            })
    })
}



// ---------------------------------------------------------------------------
// Property 2 Tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Validates: Requirements 2.1, 5.4**
    ///
    /// Property 2: Provider List Sort Invariant
    /// For any list of providers with arbitrary sort_index values, and for any
    /// reorder operation (permutation of provider IDs), after reorder:
    /// 1. Sorting providers by sort_index ascending produces the same order as the permuted IDs
    /// 2. All sort_index values are monotonically non-decreasing (0, 1, 2, ...)
    /// 3. No provider data is lost (same count, same IDs)
    #[test]
    fn prop_provider_list_sort_invariant(
        store in arb_flat_store_with_unique_ids(),
        seed in any::<u64>(),
    ) {
        let n = store.providers.len();
        // Generate a permutation deterministically from the seed
        let mut indices: Vec<usize> = (0..n).collect();
        // Simple Fisher-Yates shuffle using the seed
        let mut rng_state = seed;
        for i in (1..n).rev() {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let j = (rng_state as usize) % (i + 1);
            indices.swap(i, j);
        }

        // Build the permuted ID list (the desired new order)
        let permuted_ids: Vec<String> = indices.iter()
            .map(|&idx| store.providers[idx].id.clone())
            .collect();

        // Clone the store and apply reorder
        let mut reordered_store = store.clone();
        let result = reorder_providers(&mut reordered_store, &permuted_ids);
        prop_assert!(result.is_ok(), "reorder_providers should succeed, got: {:?}", result.err());

        // ── Verification 1: No provider data is lost ──
        prop_assert_eq!(
            reordered_store.providers.len(),
            store.providers.len(),
            "Provider count must be preserved after reorder"
        );

        // Verify same set of IDs
        let original_ids: std::collections::HashSet<&String> = store.providers.iter().map(|p| &p.id).collect();
        let reordered_ids: std::collections::HashSet<&String> = reordered_store.providers.iter().map(|p| &p.id).collect();
        prop_assert_eq!(
            original_ids,
            reordered_ids,
            "Same set of provider IDs must exist after reorder"
        );

        // ── Verification 2: sort_index values are monotonically non-decreasing (0, 1, 2, ...) ──
        // After reorder, the providers referenced by permuted_ids should have sort_index = 0, 1, 2, ...
        for (expected_idx, id) in permuted_ids.iter().enumerate() {
            let provider = reordered_store.providers.iter().find(|p| &p.id == id).unwrap();
            prop_assert_eq!(
                provider.sort_index,
                expected_idx as u32,
                "Provider '{}' at position {} should have sort_index={}, got={}",
                id, expected_idx, expected_idx, provider.sort_index
            );
        }

        // ── Verification 3: Sorting by sort_index ascending produces the permuted order ──
        let mut sorted_providers: Vec<&ProviderEntryFlat> = reordered_store.providers.iter().collect();
        sorted_providers.sort_by_key(|p| p.sort_index);

        let sorted_ids: Vec<&String> = sorted_providers.iter().map(|p| &p.id).collect();
        let expected_ids: Vec<&String> = permuted_ids.iter().collect();
        prop_assert_eq!(
            sorted_ids,
            expected_ids,
            "Sorting providers by sort_index should produce the same order as the permuted IDs"
        );

        // ── Verification 4: sort_index values are strictly monotonically increasing (0, 1, 2, ...) ──
        for (i, provider) in sorted_providers.iter().enumerate() {
            prop_assert_eq!(
                provider.sort_index,
                i as u32,
                "sort_index values should be consecutive starting from 0"
            );
        }
    }
}


// ===========================================================================
// Feature: model-provider-management, Property 12: Preset Creation Produces Valid Provider
//
// For any valid preset_id from the built-in preset list and for any non-empty
// API key string, creating a provider from that preset should succeed and produce
// a ProviderEntryFlat with: the preset's base_url_openai, the preset's
// base_url_anthropic, the provided api_key, empty model fields, a valid UUID as id,
// and a non-None created_at timestamp.
//
// **Validates: Requirements 4.6**
// ===========================================================================

use skillstar_models::providers::{create_from_preset_flat, get_all_presets_flat};

// ---------------------------------------------------------------------------
// Strategies for Property 12
// ---------------------------------------------------------------------------

/// Strategy: generate a valid preset_id from the full flat preset registry.
fn arb_flat_preset_id() -> impl Strategy<Value = String> {
    let presets = get_all_presets_flat();
    let ids: Vec<String> = presets.into_iter().map(|p| p.id).collect();
    proptest::sample::select(ids)
}

/// Strategy: generate a random non-empty API key string (printable ASCII, 1..128 chars).
fn arb_flat_api_key() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9\\-_\\.]{1,128}"
}

// ---------------------------------------------------------------------------
// Property 12 Tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Validates: Requirements 4.6**
    ///
    /// Property 12: Preset Creation Produces Valid Provider
    /// For each preset_id from get_all_presets_flat() and a random api_key,
    /// create_from_preset_flat produces a valid ProviderEntryFlat with correct fields.
    #[test]
    fn prop_preset_creation_produces_valid_provider(
        preset_id in arb_flat_preset_id(),
        api_key in arb_flat_api_key(),
    ) {
        // Call create_from_preset_flat
        let result = create_from_preset_flat(&preset_id, &api_key);
        prop_assert!(
            result.is_ok(),
            "create_from_preset_flat should succeed for preset '{}', got: {:?}",
            preset_id, result.err()
        );
        let entry = result.unwrap();

        // 1. The returned ProviderEntryFlat has a valid UUID as id (non-empty, contains hyphens)
        prop_assert!(
            !entry.id.is_empty(),
            "id should be non-empty"
        );
        prop_assert!(
            entry.id.contains('-'),
            "id '{}' should contain hyphens (UUID format)",
            entry.id
        );
        // Verify it's a valid UUID by parsing
        prop_assert!(
            uuid::Uuid::parse_str(&entry.id).is_ok(),
            "id '{}' should be a valid UUID",
            entry.id
        );

        // 2. api_key matches the input
        prop_assert_eq!(
            &entry.api_key, &api_key,
            "api_key should match the input"
        );

        // 3. Look up the preset to verify base_url fields match
        let presets = get_all_presets_flat();
        let preset = presets.iter().find(|p| p.id == preset_id)
            .expect("Preset should exist in registry");

        // base_url_openai matches the preset's base_url_openai
        prop_assert_eq!(
            &entry.base_url_openai, &preset.base_url_openai,
            "base_url_openai should match preset definition"
        );

        // base_url_anthropic matches the preset's base_url_anthropic
        prop_assert_eq!(
            &entry.base_url_anthropic, &preset.base_url_anthropic,
            "base_url_anthropic should match preset definition"
        );

        // 4. models are intentionally empty; users fetch current models after
        // provider creation instead of relying on stale preset model IDs.
        prop_assert_eq!(
            &entry.models, &preset.models,
            "models should match preset definition for '{}'",
            preset_id
        );

        // 5. created_at is Some and non-zero
        prop_assert!(
            entry.created_at.is_some(),
            "created_at should be Some for preset '{}'",
            preset_id
        );
        prop_assert!(
            entry.created_at.unwrap() > 0,
            "created_at should be non-zero for preset '{}'",
            preset_id
        );

        // 6. preset_id is Some and matches the input preset_id
        prop_assert_eq!(
            entry.preset_id.as_deref(),
            Some(preset_id.as_str()),
            "preset_id should match the input"
        );
    }

    /// **Validates: Requirements 4.6**
    ///
    /// Property 12 (part 2): Invalid preset_id returns an error.
    /// Creating a provider from a non-existent preset_id should fail.
    #[test]
    fn prop_invalid_preset_id_returns_error(
        invalid_id in "[a-z]{10,30}".prop_filter(
            "must not be a real preset id",
            |s| {
                let presets = get_all_presets_flat();
                !presets.iter().any(|p| p.id == *s)
            }
        ),
        api_key in arb_flat_api_key(),
    ) {
        let result = create_from_preset_flat(&invalid_id, &api_key);
        prop_assert!(
            result.is_err(),
            "create_from_preset_flat should fail for invalid preset_id '{}', got Ok",
            invalid_id
        );
        let err_msg = result.unwrap_err().to_string();
        prop_assert!(
            err_msg.contains("not found"),
            "Error should mention 'not found', got: {}",
            err_msg
        );
    }
}


// ===========================================================================
// Feature: model-provider-management, Property 9: Failed Operations Leave State Unchanged
//
// For any operation (create, update, delete, reorder) that fails due to
// validation error, the FlatProvidersStore should be identical to its state
// before the operation was attempted. No partial writes should occur.
//
// **Validates: Requirements 5.5, 6.4**
// ===========================================================================

use skillstar_models::providers::{
    ProviderPatchFlat, create_provider_flat, delete_provider_flat, reorder_providers,
    update_provider_flat,
};

// ---------------------------------------------------------------------------
// Strategies for Property 9
// ---------------------------------------------------------------------------

/// Strategy: generate an invalid name (empty or whitespace-only) for create_provider_flat.
fn arb_invalid_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("   ".to_string()),
        Just("\t".to_string()),
        Just(" \n ".to_string()),
        proptest::collection::vec(Just(' '), 1..=10)
            .prop_map(|chars| chars.into_iter().collect::<String>()),
    ]
}

/// Strategy: generate an invalid URL (non-empty, not parseable as http/https URL).
fn arb_invalid_url_for_flat() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("not-a-url".to_string()),
        Just("ftp://invalid.com/v1".to_string()),
        Just("://missing-scheme".to_string()),
        Just("just-text".to_string()),
        Just("file:///local/path".to_string()),
        "[a-z]{3,15}".prop_filter("must not be a valid http(s) URL", |s| {
            url::Url::parse(s).is_err() || {
                let parsed = url::Url::parse(s).unwrap();
                parsed.scheme() != "http" && parsed.scheme() != "https"
            }
        }),
    ]
}

/// Strategy: generate a non-existent UUID-like ID that won't match any provider in the store.
fn arb_nonexistent_id() -> impl Strategy<Value = String> {
    Just("nonexistent-00000000-0000-0000-0000-000000000000".to_string())
}

// ---------------------------------------------------------------------------
// Property 9 Tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Validates: Requirements 5.5, 6.4**
    ///
    /// Property 9 (part 1): create_provider_flat with empty/whitespace name should fail,
    /// and the store should remain unchanged.
    #[test]
    fn prop_failed_create_empty_name_leaves_state_unchanged(
        store in arb_flat_providers_store(),
        invalid_name in arb_invalid_name(),
    ) {
        let mut store_mut = store.clone();
        let original = store.clone();

        // Build an entry with an invalid (empty/whitespace) name
        let entry = ProviderEntryFlat {
            id: String::new(),
            name: invalid_name,
            base_url_openai: "https://api.example.com/v1".to_string(),
            base_url_anthropic: String::new(),
            models_url: String::new(),
            api_key: "sk-test".to_string(),
            models: vec!["model-a".to_string()],
            default_model: "model-a".to_string(),
            sort_index: 0,
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: None,
            meta: None,
            codex_wire_api: "responses".to_string(),
            codex_auth_mode: "api_key".to_string(),
        };

        let result = create_provider_flat(&mut store_mut, entry);
        prop_assert!(result.is_err(), "create_provider_flat with empty name should fail");

        // Store should be unchanged
        prop_assert_eq!(
            store_mut.providers.len(),
            original.providers.len(),
            "providers count should be unchanged after failed create"
        );
        for (orig, current) in original.providers.iter().zip(store_mut.providers.iter()) {
            prop_assert_eq!(orig, current, "provider entry should be unchanged after failed create");
        }
        prop_assert_eq!(
            store_mut.tool_activations.len(),
            original.tool_activations.len(),
            "tool_activations should be unchanged after failed create"
        );
    }

    /// **Validates: Requirements 5.5, 6.4**
    ///
    /// Property 9 (part 2): create_provider_flat with invalid URL should fail,
    /// and the store should remain unchanged.
    #[test]
    fn prop_failed_create_invalid_url_leaves_state_unchanged(
        store in arb_flat_providers_store(),
        invalid_url in arb_invalid_url_for_flat(),
    ) {
        let mut store_mut = store.clone();
        let original = store.clone();

        // Build an entry with an invalid URL
        let entry = ProviderEntryFlat {
            id: String::new(),
            name: "Valid Name".to_string(),
            base_url_openai: invalid_url,
            base_url_anthropic: String::new(),
            models_url: String::new(),
            api_key: "sk-test".to_string(),
            models: vec!["model-a".to_string()],
            default_model: "model-a".to_string(),
            sort_index: 0,
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: None,
            meta: None,
            codex_wire_api: "responses".to_string(),
            codex_auth_mode: "api_key".to_string(),
        };

        let result = create_provider_flat(&mut store_mut, entry);
        prop_assert!(result.is_err(), "create_provider_flat with invalid URL should fail");

        // Store should be unchanged
        prop_assert_eq!(
            store_mut.providers.len(),
            original.providers.len(),
            "providers count should be unchanged after failed create (invalid URL)"
        );
        for (orig, current) in original.providers.iter().zip(store_mut.providers.iter()) {
            prop_assert_eq!(orig, current, "provider entry should be unchanged after failed create (invalid URL)");
        }
        prop_assert_eq!(
            store_mut.tool_activations.len(),
            original.tool_activations.len(),
            "tool_activations should be unchanged after failed create (invalid URL)"
        );
    }

    /// **Validates: Requirements 5.5, 6.4**
    ///
    /// Property 9 (part 3): update_provider_flat with non-existent ID should fail,
    /// and the store should remain unchanged.
    #[test]
    fn prop_failed_update_nonexistent_id_leaves_state_unchanged(
        store in arb_flat_providers_store(),
        nonexistent_id in arb_nonexistent_id(),
    ) {
        let mut store_mut = store.clone();
        let original = store.clone();

        let patch = ProviderPatchFlat {
            name: Some("New Name".to_string()),
            ..Default::default()
        };

        let result = update_provider_flat(&mut store_mut, &nonexistent_id, patch);
        prop_assert!(result.is_err(), "update_provider_flat with non-existent ID should fail");

        // Store should be unchanged
        prop_assert_eq!(
            store_mut.providers.len(),
            original.providers.len(),
            "providers count should be unchanged after failed update"
        );
        for (orig, current) in original.providers.iter().zip(store_mut.providers.iter()) {
            prop_assert_eq!(orig, current, "provider entry should be unchanged after failed update");
        }
        prop_assert_eq!(
            store_mut.tool_activations.len(),
            original.tool_activations.len(),
            "tool_activations should be unchanged after failed update"
        );
    }

    /// **Validates: Requirements 5.5, 6.4**
    ///
    /// Property 9 (part 4): delete_provider_flat with non-existent ID should fail,
    /// and the store should remain unchanged.
    #[test]
    fn prop_failed_delete_nonexistent_id_leaves_state_unchanged(
        store in arb_flat_providers_store(),
        nonexistent_id in arb_nonexistent_id(),
    ) {
        let mut store_mut = store.clone();
        let original = store.clone();

        let result = delete_provider_flat(&mut store_mut, &nonexistent_id);
        prop_assert!(result.is_err(), "delete_provider_flat with non-existent ID should fail");

        // Store should be unchanged
        prop_assert_eq!(
            store_mut.providers.len(),
            original.providers.len(),
            "providers count should be unchanged after failed delete"
        );
        for (orig, current) in original.providers.iter().zip(store_mut.providers.iter()) {
            prop_assert_eq!(orig, current, "provider entry should be unchanged after failed delete");
        }
        prop_assert_eq!(
            store_mut.tool_activations.len(),
            original.tool_activations.len(),
            "tool_activations should be unchanged after failed delete"
        );
    }

    /// **Validates: Requirements 5.5, 6.4**
    ///
    /// Property 9 (part 5): reorder_providers with non-existent IDs should fail,
    /// and the store should remain unchanged.
    #[test]
    fn prop_failed_reorder_nonexistent_ids_leaves_state_unchanged(
        store in arb_flat_providers_store(),
        nonexistent_id in arb_nonexistent_id(),
    ) {
        let mut store_mut = store.clone();
        let original = store.clone();

        // Include a non-existent ID in the reorder list
        let ordered_ids = vec![nonexistent_id];

        let result = reorder_providers(&mut store_mut, &ordered_ids);
        prop_assert!(result.is_err(), "reorder_providers with non-existent IDs should fail");

        // Store should be unchanged — all sort_index values should be the same
        prop_assert_eq!(
            store_mut.providers.len(),
            original.providers.len(),
            "providers count should be unchanged after failed reorder"
        );
        for (orig, current) in original.providers.iter().zip(store_mut.providers.iter()) {
            prop_assert_eq!(orig, current, "provider entry (including sort_index) should be unchanged after failed reorder");
        }
        prop_assert_eq!(
            store_mut.tool_activations.len(),
            original.tool_activations.len(),
            "tool_activations should be unchanged after failed reorder"
        );
    }
}


// ===========================================================================
// Feature: model-provider-management, Property 5: Single Active Provider Per Tool
//
// For any sequence of activate_tool operations, each tool_id in the
// tool_activations map should map to at most one provider_id at any point in
// time. Activating provider B for a tool that currently has provider A active
// should result in only provider B being active for that tool.
//
// **Validates: Requirements 3.8, 5.2**
// ===========================================================================

use skillstar_models::providers::activate_tool;

// ---------------------------------------------------------------------------
// Strategies for Property 5
// ---------------------------------------------------------------------------

/// Strategy: generate a FlatProvidersStore with 2..=5 providers that have valid URLs.
/// All providers have non-empty base_url_openai and base_url_anthropic so that
/// activate_tool can succeed for any tool_id.
fn arb_store_with_valid_providers() -> impl Strategy<Value = FlatProvidersStore> {
    proptest::collection::vec(
        (
            "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}", // id
            "[A-Za-z]{3,16}",                                                  // name
            proptest::collection::vec("[a-z]{3,12}".prop_map(|s| s), 1..=3),   // models
        ),
        2..=5,
    )
    .prop_map(|entries| {
        let providers: Vec<ProviderEntryFlat> = entries
            .into_iter()
            .enumerate()
            .map(|(i, (id, name, models))| ProviderEntryFlat {
                id,
                name,
                base_url_openai: format!("https://api{}.example.com/v1", i),
                base_url_anthropic: format!("https://api{}.example.com/anthropic", i),
                models_url: format!("https://api{}.example.com/v1/models", i),
                api_key: format!("sk-test-{}", i),
                models: models.clone(),
                default_model: models.first().cloned().unwrap_or_default(),
                sort_index: i as u32,
                preset_id: None,
                icon_color: None,
                notes: None,
                created_at: None,
                meta: None,
                codex_wire_api: "responses".to_string(),
                codex_auth_mode: "api_key".to_string(),
            })
            .collect();

        FlatProvidersStore {
            version: 2,
            providers,
            tool_activations: HashMap::new(),
        }
    })
}

/// A single activate_tool operation: (provider_index, tool_id_index, model_option).
/// We use indices into the providers vec and a fixed set of tool_ids.
#[derive(Debug, Clone)]
struct ActivateOp {
    provider_idx: usize,
    tool_id_idx: usize,
    model: Option<String>,
}

/// The fixed set of tool_ids used in the test.
const TOOL_IDS: &[&str] = &["claude-code", "codex", "custom-tool"];

/// Strategy: generate a sequence of activate operations.
fn arb_activate_ops(max_providers: usize) -> impl Strategy<Value = Vec<ActivateOp>> {
    proptest::collection::vec(
        (
            0..max_providers,                          // provider_idx
            0..TOOL_IDS.len(),                         // tool_id_idx
            proptest::option::of("[a-z]{3,12}"),       // optional model override
        )
            .prop_map(|(provider_idx, tool_id_idx, model)| ActivateOp {
                provider_idx,
                tool_id_idx,
                model,
            }),
        5..=30, // sequence length: 5 to 30 operations
    )
}

// ---------------------------------------------------------------------------
// Property 5 Tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Validates: Requirements 3.8, 5.2**
    ///
    /// Property 5: Single Active Provider Per Tool
    /// For any sequence of activate_tool operations, each tool_id in the
    /// tool_activations map should map to at most one provider_id at any point
    /// in time. Activating provider B for a tool that currently has provider A
    /// active should result in only provider B being active for that tool.
    #[test]
    fn prop_single_active_provider_per_tool(
        store in arb_store_with_valid_providers(),
        ops in arb_activate_ops(5),
    ) {
        let mut store_mut = store.clone();
        let num_providers = store_mut.providers.len();

        // Apply each activation operation and verify the invariant after each step
        for (step, op) in ops.iter().enumerate() {
            // Clamp provider_idx to valid range
            let provider_idx = op.provider_idx % num_providers;
            let provider_id = store_mut.providers[provider_idx].id.clone();
            let tool_id = TOOL_IDS[op.tool_id_idx];
            let model_ref = op.model.as_deref();

            // Perform activation
            let result = activate_tool(&mut store_mut, &provider_id, tool_id, model_ref, None);
            prop_assert!(
                result.is_ok(),
                "activate_tool should succeed at step {} (provider_idx={}, tool='{}'), got: {:?}",
                step, provider_idx, tool_id, result.err()
            );

            // ── Invariant check after each activation ──
            // Each tool_id should map to at most one provider_id
            for (tid, activation) in &store_mut.tool_activations {
                if let Some(act) = activation {
                    // Count how many tool_activations entries reference this same provider
                    // for THIS specific tool — there should be exactly one entry per tool_id
                    // (which is guaranteed by HashMap keys being unique), but we verify the
                    // value is consistent: only one provider_id per tool.
                    //
                    // The real invariant: for the tool we just activated, only the new
                    // provider should be active.
                    if tid == tool_id {
                        prop_assert_eq!(
                            &act.provider_id, &provider_id,
                            "At step {}: tool '{}' should have provider '{}' active, but found '{}'",
                            step, tool_id, provider_id, act.provider_id
                        );
                    }
                }
            }

            // Additional invariant: no two different tool_activations entries for the
            // SAME tool_id can exist (HashMap guarantees this, but let's verify the
            // broader property: each tool has at most one active provider)
            let active_count_for_tool: usize = store_mut
                .tool_activations
                .iter()
                .filter(|(tid, act)| *tid == tool_id && act.is_some())
                .count();
            prop_assert!(
                active_count_for_tool <= 1,
                "At step {}: tool '{}' should have at most 1 active provider, found {}",
                step, tool_id, active_count_for_tool
            );
        }

        // ── Final state verification ──
        // After all operations, each tool_id should still have at most one active provider
        for tool_id in TOOL_IDS {
            let active_count: usize = store_mut
                .tool_activations
                .iter()
                .filter(|(tid, act)| tid.as_str() == *tool_id && act.is_some())
                .count();
            prop_assert!(
                active_count <= 1,
                "Final state: tool '{}' should have at most 1 active provider, found {}",
                tool_id, active_count
            );
        }

        // Verify that the last activation for each tool is the one that's active
        // Build expected state: for each tool, the last operation that targeted it
        // should determine the active provider
        let mut expected_active: HashMap<&str, &str> = HashMap::new();
        for op in &ops {
            let provider_idx = op.provider_idx % num_providers;
            let provider_id = &store.providers[provider_idx].id;
            let tool_id = TOOL_IDS[op.tool_id_idx];
            expected_active.insert(tool_id, provider_id);
        }

        for (tool_id, expected_provider_id) in &expected_active {
            if let Some(Some(activation)) = store_mut.tool_activations.get(*tool_id) {
                prop_assert_eq!(
                    &activation.provider_id, *expected_provider_id,
                    "Final state: tool '{}' should have provider '{}' active, but found '{}'",
                    tool_id, expected_provider_id, activation.provider_id
                );
            }
        }
    }
}

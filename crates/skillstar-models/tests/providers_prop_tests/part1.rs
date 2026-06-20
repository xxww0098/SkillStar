//! Part 1: Provider validation (Property 3), Malformed Store Recovery (Property 15),
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

use super::setup_temp_store;
use proptest::prelude::*;
use serde_json::Value;
use skillstar_models::providers::{
    ModelMapping, ProviderEntry, ProviderSettings, create_from_preset_at, create_provider_at,
    get_provider_presets, read_store_from,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
        "[a-zA-Z0-9 _\\-\\./!@#$%^&*()]{0,50}"
            .prop_filter("must not be a valid URL", |s| url::Url::parse(s).is_err()),
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
    assert!(
        store.claude.providers.is_empty(),
        "claude providers should be empty"
    );
    assert!(
        store.claude.current.is_none(),
        "claude current should be None"
    );
    assert!(
        store.codex.providers.is_empty(),
        "codex providers should be empty"
    );
    assert!(
        store.codex.current.is_none(),
        "codex current should be None"
    );
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
    prop_oneof![Just("claude".to_string()), Just("codex".to_string()),]
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

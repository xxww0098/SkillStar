//! Property-based tests for the `providers` module (split from inline `mod proptests`).

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

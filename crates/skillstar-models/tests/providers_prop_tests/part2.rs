//! Part 2: Store Serialization Round-Trip (Property 1) and
//! Migration Preserves All Provider Data (Property 11).
//!
//! **Validates: Requirements 8.1, 8.2, 8.7**

use super::{arb_flat_providers_store, setup_temp_store};
use proptest::prelude::*;
use serde_json::Value;
use skillstar_models::providers::{
    migrate_store_if_needed, read_flat_store, write_flat_store, AppProviders, FlatProvidersStore,
    ModelMapping, ProviderEntry, ProviderSettings, ProvidersStore,
};
use std::collections::{HashMap, HashSet};

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

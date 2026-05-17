//! Property-based tests for tool_sync module.
//!
//! Contains:
//! - Property 9: Tool Config Serialization Round-Trip
//! - Property 6: Config Merge Preserves Existing Fields
//! - Property 8: Save Re-Syncs Active Tools

use proptest::prelude::*;
use serde_json::Value;
use skillstar_models::providers::{
    FlatProvidersStore, ModelMapping, ProviderEntryFlat, ProviderSettings, ToolActivation,
};
use skillstar_models::tool_sync::{
    generate_claude_code_config, generate_codex_config, merge_json_env_write,
    resync_active_tools, write_codex_config_flat,
};
use std::collections::HashMap;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Strategy that generates valid URL strings (with scheme, host, and optional path).
fn valid_url_strategy() -> impl Strategy<Value = String> {
    (
        prop_oneof![Just("https"), Just("http")],
        "[a-z][a-z0-9]{2,15}",           // subdomain/host part
        prop_oneof![Just("com"), Just("io"), Just("org"), Just("cn"), Just("net")],
        prop::option::of("[a-z0-9/]{1,20}"), // optional path
    )
        .prop_map(|(scheme, host, tld, path)| {
            match path {
                Some(p) => format!("{}://{}.{}/{}", scheme, host, tld, p),
                None => format!("{}://{}.{}", scheme, host, tld),
            }
        })
}

/// Strategy that generates non-empty API key strings.
fn api_key_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Typical sk-prefixed keys
        "sk-[a-zA-Z0-9]{10,50}",
        // Other key formats
        "[a-zA-Z0-9_\\-]{8,64}",
        // Keys with special but safe characters
        "[a-zA-Z][a-zA-Z0-9\\-_\\.]{7,40}",
    ]
}

/// Strategy that generates a non-empty list of ModelMapping entries.
fn model_mappings_strategy() -> impl Strategy<Value = Vec<ModelMapping>> {
    prop::collection::vec(
        (
            "[a-z][a-z0-9\\-]{2,30}",  // source_model
            "[a-z][a-z0-9\\-]{2,30}",  // target_model
            any::<bool>(),              // enabled
        )
            .prop_map(|(source, target, enabled)| ModelMapping {
                source_model: source,
                target_model: target,
                enabled,
            }),
        1..=5, // at least 1, at most 5 models
    )
}

/// Strategy that generates valid ProviderSettings.
fn valid_provider_settings_strategy() -> impl Strategy<Value = ProviderSettings> {
    (
        valid_url_strategy(),
        api_key_strategy(),
        model_mappings_strategy(),
        prop::option::of(1000u64..60000u64),  // timeout_ms
        prop::option::of(1u32..5u32),         // max_retries
    )
        .prop_map(|(base_url, api_key, models, timeout_ms, max_retries)| ProviderSettings {
            base_url,
            api_key,
            models,
            timeout_ms,
            max_retries,
        })
}

// ---------------------------------------------------------------------------
// Property 6 Strategies
// ---------------------------------------------------------------------------

/// Strategy that generates a safe JSON key name (not one of the managed env keys).
/// Avoids ANTHROPIC_BASE_URL, ANTHROPIC_AUTH_TOKEN, ANTHROPIC_MODEL.
fn non_managed_env_key_strategy() -> impl Strategy<Value = String> {
    "[A-Z][A-Z0-9_]{2,20}".prop_filter(
        "Must not be a managed env key",
        |k| {
            k != "ANTHROPIC_BASE_URL"
                && k != "ANTHROPIC_AUTH_TOKEN"
                && k != "ANTHROPIC_MODEL"
        },
    )
}

/// Strategy that generates a safe top-level JSON key (not "env").
fn non_env_top_level_key_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-zA-Z0-9_]{2,15}".prop_filter(
        "Must not be 'env'",
        |k| k != "env",
    )
}

/// Strategy that generates a simple JSON value (string, number, or bool).
fn simple_json_value_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        "[a-zA-Z0-9_ ]{1,30}".prop_map(|s| Value::String(s)),
        (0i64..10000).prop_map(|n| Value::Number(serde_json::Number::from(n))),
        any::<bool>().prop_map(Value::Bool),
    ]
}

/// Strategy that generates random extra fields for the env block (non-managed keys).
fn extra_env_fields_strategy() -> impl Strategy<Value = Vec<(String, Value)>> {
    prop::collection::vec(
        (non_managed_env_key_strategy(), simple_json_value_strategy()),
        0..=5,
    )
}

/// Strategy that generates random extra top-level fields (not "env").
fn extra_top_level_fields_strategy() -> impl Strategy<Value = Vec<(String, Value)>> {
    prop::collection::vec(
        (non_env_top_level_key_strategy(), simple_json_value_strategy()),
        0..=5,
    )
}

/// Strategy that generates a valid ProviderEntryFlat for sync testing.
fn provider_entry_flat_strategy() -> impl Strategy<Value = ProviderEntryFlat> {
    (
        valid_url_strategy(),  // base_url_openai
        valid_url_strategy(),  // base_url_anthropic
        api_key_strategy(),
        "[a-z][a-z0-9\\-]{2,20}", // model name
    )
        .prop_map(|(url_openai, url_anthropic, api_key, model)| ProviderEntryFlat {
            id: "test-uuid".to_string(),
            name: "Test Provider".to_string(),
            base_url_openai: url_openai,
            base_url_anthropic: url_anthropic,
            models_url: String::new(),
            api_key,
            models: vec![model.clone()],
            default_model: model,
            sort_index: 0,
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: Some(1719000000000),
            meta: None,
        })
}

/// Strategy that generates a safe TOML section name (not managed by Codex sync).
/// Avoids: "model_providers", and top-level keys "model_provider", "model".
fn non_managed_toml_section_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z_]{2,12}".prop_filter(
        "Must not be a managed TOML key/section",
        |k| {
            k != "model_providers"
                && k != "model_provider"
                && k != "model"
        },
    )
}

/// Strategy that generates a simple TOML value (string, integer, or bool).
fn simple_toml_value_strategy() -> impl Strategy<Value = toml::Value> {
    prop_oneof![
        "[a-zA-Z0-9_ ]{1,20}".prop_map(|s| toml::Value::String(s)),
        (0i64..10000).prop_map(|n| toml::Value::Integer(n)),
        any::<bool>().prop_map(toml::Value::Boolean),
    ]
}

/// Strategy that generates random extra TOML sections with key-value pairs.
fn extra_toml_sections_strategy(
) -> impl Strategy<Value = Vec<(String, Vec<(String, toml::Value)>)>> {
    prop::collection::vec(
        (
            non_managed_toml_section_strategy(),
            prop::collection::vec(
                ("[a-z][a-z_]{1,10}", simple_toml_value_strategy()),
                1..=3,
            ),
        ),
        1..=4,
    )
}

/// Strategy that generates a model name for Codex sync.
fn model_name_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9\\-]{2,20}"
}

// ---------------------------------------------------------------------------
// Property 9 Tests: Tool Config Serialization Round-Trip
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Validates: Requirements 4.4**
    ///
    /// Property 9 (part 1): Claude Code JSON serialization round-trip.
    #[test]
    fn prop_claude_code_config_round_trip(settings in valid_provider_settings_strategy()) {
        let json_str = generate_claude_code_config(&settings)
            .expect("generate_claude_code_config should not fail for valid settings");

        let parsed: HashMap<String, Value> = serde_json::from_str(&json_str)
            .expect("Generated JSON should be valid");

        let api_url = parsed.get("apiUrl")
            .expect("Parsed JSON should contain 'apiUrl' key")
            .as_str()
            .expect("'apiUrl' should be a string");
        prop_assert_eq!(api_url, settings.base_url.as_str());

        let api_key = parsed.get("apiKey")
            .expect("Parsed JSON should contain 'apiKey' key")
            .as_str()
            .expect("'apiKey' should be a string");
        prop_assert_eq!(api_key, settings.api_key.as_str());
    }

    /// **Validates: Requirements 4.5**
    ///
    /// Property 9 (part 2): Codex TOML serialization round-trip.
    #[test]
    fn prop_codex_config_round_trip(settings in valid_provider_settings_strategy()) {
        let toml_str = generate_codex_config(&settings)
            .expect("generate_codex_config should not fail for valid settings");

        let parsed: toml::Table = toml::from_str(&toml_str)
            .expect("Generated TOML should be valid");

        let provider_section = parsed.get("provider")
            .expect("Parsed TOML should contain 'provider' section")
            .as_table()
            .expect("'provider' should be a table");

        let base_url = provider_section.get("base_url")
            .expect("[provider] should contain 'base_url' key")
            .as_str()
            .expect("'base_url' should be a string");
        prop_assert_eq!(base_url, settings.base_url.as_str());

        let api_key = provider_section.get("api_key")
            .expect("[provider] should contain 'api_key' key")
            .as_str()
            .expect("'api_key' should be a string");
        prop_assert_eq!(api_key, settings.api_key.as_str());
    }
}

// ---------------------------------------------------------------------------
// Feature: model-provider-management, Property 6: Config Merge Preserves Existing Fields
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Validates: Requirements 8.3**
    ///
    /// Property 6 (part 1): JSON merge (Claude Code) preserves existing fields.
    ///
    /// Generate random JSON content with extra fields in the env block and at top level,
    /// then call merge_json_env_write with managed fields. Verify that all non-managed
    /// fields in the env block and all top-level fields are preserved unchanged.
    ///
    /// Managed fields for Claude Code env block:
    /// - ANTHROPIC_BASE_URL
    /// - ANTHROPIC_AUTH_TOKEN
    /// - ANTHROPIC_MODEL
    #[test]
    fn prop_json_merge_preserves_existing_fields(
        extra_env_fields in extra_env_fields_strategy(),
        extra_top_level_fields in extra_top_level_fields_strategy(),
        provider in provider_entry_flat_strategy(),
        model in model_name_strategy(),
    ) {
        let tmp = TempDir::new().expect("Failed to create temp dir");
        let config_path = tmp.path().join("settings.json");

        // Build initial JSON with extra fields
        let mut initial_json = serde_json::Map::new();

        // Add extra top-level fields
        for (key, value) in &extra_top_level_fields {
            initial_json.insert(key.clone(), value.clone());
        }

        // Add env block with extra (non-managed) fields
        let mut env_map = serde_json::Map::new();
        for (key, value) in &extra_env_fields {
            env_map.insert(key.clone(), value.clone());
        }
        initial_json.insert("env".to_string(), Value::Object(env_map));

        // Write initial JSON to file
        let initial_content = serde_json::to_string_pretty(&Value::Object(initial_json.clone()))
            .expect("Failed to serialize initial JSON");
        std::fs::write(&config_path, &initial_content)
            .expect("Failed to write initial config");

        // Call merge_json_env_write with managed fields
        let managed_fields: Vec<(&str, Value)> = vec![
            ("ANTHROPIC_BASE_URL", Value::String(provider.base_url_anthropic.clone())),
            ("ANTHROPIC_AUTH_TOKEN", Value::String(provider.api_key.clone())),
            ("ANTHROPIC_MODEL", Value::String(model.clone())),
        ];
        merge_json_env_write(&config_path, &managed_fields)
            .expect("merge_json_env_write should succeed");

        // Read back the result
        let result_content = std::fs::read_to_string(&config_path)
            .expect("Failed to read result config");
        let result_json: Value = serde_json::from_str(&result_content)
            .expect("Result should be valid JSON");
        let result_obj = result_json.as_object()
            .expect("Result should be a JSON object");

        // Verify: all extra top-level fields are preserved
        for (key, expected_value) in &extra_top_level_fields {
            let actual = result_obj.get(key);
            prop_assert_eq!(
                actual, Some(expected_value),
                "Top-level field '{}' was not preserved. Expected {:?}, got {:?}",
                key, expected_value, actual
            );
        }

        // Verify: env block exists and contains managed fields with correct values
        let result_env = result_obj.get("env")
            .expect("Result should have 'env' key")
            .as_object()
            .expect("'env' should be an object");

        prop_assert_eq!(
            result_env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
            Some(provider.base_url_anthropic.as_str()),
            "ANTHROPIC_BASE_URL mismatch"
        );
        prop_assert_eq!(
            result_env.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()),
            Some(provider.api_key.as_str()),
            "ANTHROPIC_AUTH_TOKEN mismatch"
        );
        prop_assert_eq!(
            result_env.get("ANTHROPIC_MODEL").and_then(|v| v.as_str()),
            Some(model.as_str()),
            "ANTHROPIC_MODEL mismatch"
        );

        // Verify: all extra env fields (non-managed) are preserved
        for (key, expected_value) in &extra_env_fields {
            let actual = result_env.get(key);
            prop_assert_eq!(
                actual, Some(expected_value),
                "Env field '{}' was not preserved. Expected {:?}, got {:?}",
                key, expected_value, actual
            );
        }
    }

    /// **Validates: Requirements 8.4**
    ///
    /// Property 6 (part 2): TOML merge (Codex) preserves existing fields.
    ///
    /// Generate random TOML content with extra sections, then call write_codex_config_flat
    /// with provider settings. Verify that all non-managed sections/keys are preserved
    /// unchanged.
    ///
    /// Managed fields for Codex config.toml:
    /// - model_provider (top-level)
    /// - model (top-level)
    /// - [model_providers.skillstar] section
    #[test]
    fn prop_toml_merge_preserves_existing_fields(
        extra_sections in extra_toml_sections_strategy(),
        provider in provider_entry_flat_strategy(),
        model in model_name_strategy(),
    ) {
        let tmp = TempDir::new().expect("Failed to create temp dir");
        let config_path = tmp.path().join("config.toml");

        // Build initial TOML with extra sections
        let mut initial_table = toml::Table::new();
        for (section_name, fields) in &extra_sections {
            let mut section = toml::Table::new();
            for (key, value) in fields {
                section.insert(key.clone(), value.clone());
            }
            initial_table.insert(section_name.clone(), toml::Value::Table(section));
        }

        // Write initial TOML to file
        let initial_content = toml::to_string_pretty(&initial_table)
            .expect("Failed to serialize initial TOML");
        std::fs::write(&config_path, &initial_content)
            .expect("Failed to write initial config");

        // Call write_codex_config_flat
        write_codex_config_flat(&config_path, &provider, &model)
            .expect("write_codex_config_flat should succeed");

        // Read back the result
        let result_content = std::fs::read_to_string(&config_path)
            .expect("Failed to read result config");
        let result_table: toml::Table = toml::from_str(&result_content)
            .expect("Result should be valid TOML");

        // Verify: managed fields are correctly set
        prop_assert_eq!(
            result_table.get("model_provider").and_then(|v| v.as_str()),
            Some("skillstar"),
            "model_provider should be 'skillstar'"
        );
        prop_assert_eq!(
            result_table.get("model").and_then(|v| v.as_str()),
            Some(model.as_str()),
            "model should match the provided model"
        );

        // Verify: [model_providers.skillstar] section is correctly set
        let mp = result_table.get("model_providers")
            .expect("Result should have 'model_providers'")
            .as_table()
            .expect("'model_providers' should be a table");
        let skillstar = mp.get("skillstar")
            .expect("model_providers should have 'skillstar'")
            .as_table()
            .expect("'skillstar' should be a table");
        prop_assert_eq!(
            skillstar.get("base_url").and_then(|v| v.as_str()),
            Some(provider.base_url_openai.as_str()),
            "skillstar.base_url mismatch"
        );
        prop_assert_eq!(
            skillstar.get("name").and_then(|v| v.as_str()),
            Some("SkillStar"),
            "skillstar.name should be 'SkillStar'"
        );

        // Verify: all extra sections are preserved unchanged
        for (section_name, fields) in &extra_sections {
            let section = result_table.get(section_name)
                .unwrap_or_else(|| panic!("Section '{}' should be preserved", section_name));
            let section_table = section.as_table()
                .unwrap_or_else(|| panic!("Section '{}' should be a table", section_name));

            for (key, expected_value) in fields {
                let actual = section_table.get(key);
                prop_assert_eq!(
                    actual, Some(expected_value),
                    "Field '{}.{}' was not preserved. Expected {:?}, got {:?}",
                    section_name, key, expected_value, actual
                );
            }
        }
    }
}


// ---------------------------------------------------------------------------
// Feature: model-provider-management, Property 8: Save Re-Syncs Active Tools
// ---------------------------------------------------------------------------

/// Strategy that generates a random provider_id (UUID-like string).
fn provider_id_strategy() -> impl Strategy<Value = String> {
    "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}"
}

/// Strategy that generates a FlatProvidersStore with a target provider that is
/// active for a random number of tools (0 to N).
///
/// The store contains:
/// - One "target" provider (the one we'll resync)
/// - Optionally other providers (to ensure we only resync the target)
/// - tool_activations with varying numbers of tools pointing to the target provider
fn store_with_active_tools_strategy(
) -> impl Strategy<Value = (FlatProvidersStore, String, Vec<String>)> {
    // Generate the target provider
    (
        provider_id_strategy(),
        valid_url_strategy(),
        valid_url_strategy(),
        api_key_strategy(),
        "[a-z][a-z0-9\\-]{2,20}",
    )
        .prop_flat_map(|(target_id, url_openai, url_anthropic, api_key, model)| {
            let target_provider = ProviderEntryFlat {
                id: target_id.clone(),
                name: "Target Provider".to_string(),
                base_url_openai: url_openai,
                base_url_anthropic: url_anthropic,
                models_url: String::new(),
                api_key,
                models: vec![model.clone()],
                default_model: model,
                sort_index: 0,
                preset_id: None,
                icon_color: None,
                notes: None,
                created_at: Some(1719000000000),
                meta: None,
            };

            // Generate a subset of known tool_ids that will be activated for this provider
            let known_tools = vec!["claude-code".to_string(), "codex".to_string()];

            // Use proptest to select which tools are active (0..=2 tools)
            let target_id_clone = target_id.clone();
            let target_provider_clone = target_provider.clone();

            prop::collection::vec(
                prop::sample::select(known_tools),
                0..=2usize,
            )
            .prop_map(move |selected_tools| {
                // Deduplicate selected tools
                let mut active_tools: Vec<String> = Vec::new();
                for t in &selected_tools {
                    if !active_tools.contains(t) {
                        active_tools.push(t.clone());
                    }
                }

                // Build tool_activations map
                let mut tool_activations: HashMap<String, Option<ToolActivation>> = HashMap::new();
                for tool_id in &active_tools {
                    tool_activations.insert(
                        tool_id.clone(),
                        Some(ToolActivation {
                            provider_id: target_id_clone.clone(),
                            model: target_provider_clone.default_model.clone(),
                        }),
                    );
                }

                let store = FlatProvidersStore {
                    version: 2,
                    providers: vec![target_provider_clone.clone()],
                    tool_activations,
                };

                (store, target_id_clone.clone(), active_tools)
            })
        })
}

/// Strategy that generates a FlatProvidersStore with multiple providers and
/// varying tool activations, some pointing to the target and some to others.
fn store_with_mixed_activations_strategy(
) -> impl Strategy<Value = (FlatProvidersStore, String, Vec<String>)> {
    (
        provider_id_strategy(),  // target provider id
        provider_id_strategy(),  // other provider id
        valid_url_strategy(),
        valid_url_strategy(),
        api_key_strategy(),
        "[a-z][a-z0-9\\-]{2,20}",  // model for target
        "[a-z][a-z0-9\\-]{2,20}",  // model for other
        any::<bool>(),  // whether claude-code points to target
        any::<bool>(),  // whether codex points to target
    )
        .prop_filter(
            "Provider IDs must be different",
            |(target_id, other_id, _, _, _, _, _, _, _)| target_id != other_id,
        )
        .prop_map(
            |(target_id, other_id, url_openai, url_anthropic, api_key, model_target, model_other, claude_to_target, codex_to_target)| {
                let target_provider = ProviderEntryFlat {
                    id: target_id.clone(),
                    name: "Target Provider".to_string(),
                    base_url_openai: url_openai.clone(),
                    base_url_anthropic: url_anthropic.clone(),
                    models_url: String::new(),
                    api_key: api_key.clone(),
                    models: vec![model_target.clone()],
                    default_model: model_target.clone(),
                    sort_index: 0,
                    preset_id: None,
                    icon_color: None,
                    notes: None,
                    created_at: Some(1719000000000),
                    meta: None,
                };

                let other_provider = ProviderEntryFlat {
                    id: other_id.clone(),
                    name: "Other Provider".to_string(),
                    base_url_openai: url_openai,
                    base_url_anthropic: url_anthropic,
                    models_url: String::new(),
                    api_key,
                    models: vec![model_other.clone()],
                    default_model: model_other.clone(),
                    sort_index: 1,
                    preset_id: None,
                    icon_color: None,
                    notes: None,
                    created_at: Some(1719000000001),
                    meta: None,
                };

                // Build tool_activations: each tool points to either target or other
                let mut tool_activations: HashMap<String, Option<ToolActivation>> = HashMap::new();
                let mut expected_active_tools: Vec<String> = Vec::new();

                // Claude Code activation
                if claude_to_target {
                    tool_activations.insert(
                        "claude-code".to_string(),
                        Some(ToolActivation {
                            provider_id: target_id.clone(),
                            model: model_target.clone(),
                        }),
                    );
                    expected_active_tools.push("claude-code".to_string());
                } else {
                    tool_activations.insert(
                        "claude-code".to_string(),
                        Some(ToolActivation {
                            provider_id: other_id.clone(),
                            model: model_other.clone(),
                        }),
                    );
                }

                // Codex activation
                if codex_to_target {
                    tool_activations.insert(
                        "codex".to_string(),
                        Some(ToolActivation {
                            provider_id: target_id.clone(),
                            model: model_target.clone(),
                        }),
                    );
                    expected_active_tools.push("codex".to_string());
                } else {
                    tool_activations.insert(
                        "codex".to_string(),
                        Some(ToolActivation {
                            provider_id: other_id.clone(),
                            model: model_other,
                        }),
                    );
                }

                let store = FlatProvidersStore {
                    version: 2,
                    providers: vec![target_provider, other_provider],
                    tool_activations,
                };

                (store, target_id, expected_active_tools)
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Validates: Requirements 3.10**
    ///
    /// Property 8 (part 1): resync_active_tools returns exactly K results for K active tools.
    ///
    /// For any FlatProvidersStore where a provider is active for K tools (K ≥ 0):
    /// - Call resync_active_tools(store, provider_id)
    /// - Verify the result contains exactly K entries (one per active tool)
    /// - Verify each result's tool_id matches one of the tools where this provider is active
    ///
    /// Note: The actual sync will fail in tests (no real config files at ~/.claude or ~/.codex),
    /// but we verify the COUNT of results matches K and that each result has the correct tool_id.
    #[test]
    fn prop_resync_active_tools_returns_correct_count(
        (store, target_id, expected_active_tools) in store_with_active_tools_strategy()
    ) {
        let results = resync_active_tools(&store, &target_id);

        // Verify: result count matches the number of active tools for this provider
        prop_assert_eq!(
            results.len(),
            expected_active_tools.len(),
            "Expected {} results for {} active tools, got {}",
            expected_active_tools.len(),
            expected_active_tools.len(),
            results.len()
        );

        // Verify: each result's tool_id matches one of the expected active tools
        let result_tool_ids: Vec<&str> = results.iter().map(|r| r.tool_id.as_str()).collect();
        for expected_tool in &expected_active_tools {
            prop_assert!(
                result_tool_ids.contains(&expected_tool.as_str()),
                "Expected tool_id '{}' not found in results. Results: {:?}",
                expected_tool,
                result_tool_ids
            );
        }

        // Verify: no unexpected tool_ids in results
        for result in &results {
            prop_assert!(
                expected_active_tools.contains(&result.tool_id),
                "Unexpected tool_id '{}' in results. Expected only: {:?}",
                result.tool_id,
                expected_active_tools
            );
        }
    }

    /// **Validates: Requirements 3.10**
    ///
    /// Property 8 (part 2): resync_active_tools only syncs tools belonging to the target provider.
    ///
    /// With multiple providers and mixed activations, verify that resync only produces
    /// results for tools where the target provider is active, not for tools using other providers.
    #[test]
    fn prop_resync_active_tools_only_syncs_target_provider(
        (store, target_id, expected_active_tools) in store_with_mixed_activations_strategy()
    ) {
        let results = resync_active_tools(&store, &target_id);

        // Verify: result count matches expected active tools for target provider
        prop_assert_eq!(
            results.len(),
            expected_active_tools.len(),
            "Expected {} results, got {}. Expected tools: {:?}",
            expected_active_tools.len(),
            results.len(),
            expected_active_tools
        );

        // Verify: each result's tool_id is one that the target provider is active for
        for result in &results {
            prop_assert!(
                expected_active_tools.contains(&result.tool_id),
                "Result tool_id '{}' should not be synced — target provider is not active for it. Expected: {:?}",
                result.tool_id,
                expected_active_tools
            );
        }

        // Verify: all expected tools are represented in results
        for expected_tool in &expected_active_tools {
            let found = results.iter().any(|r| r.tool_id == *expected_tool);
            prop_assert!(
                found,
                "Expected tool '{}' not found in resync results",
                expected_tool
            );
        }
    }
}

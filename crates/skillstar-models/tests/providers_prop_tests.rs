//! Property-based tests for Provider management.
//!
//! This integration-test binary is split across several modules to keep each
//! source file small. The entry file holds the shared imports, helpers, and
//! strategies that more than one part depends on; each `part*` module contains a
//! cohesive group of property tests.
//!
//! - `part1`: Provider validation (Property 3), Malformed Store Recovery
//!   (Property 15), Preset Creation Fills Expected Fields (Property 8).
//! - `part2`: Store Serialization Round-Trip (Property 1), Migration Preserves
//!   All Provider Data (Property 11).
//! - `part3`: Provider List Sort Invariant (Property 2), Preset Creation Produces
//!   Valid Provider (Property 12), Failed Operations Leave State Unchanged
//!   (Property 9), Single Active Provider Per Tool (Property 5).
//!
//! **Validates: Requirements 2.1, 2.2, 2.3, 3.2, 3.5, 3.6, 3.8, 4.6, 5.2, 5.4,
//! 5.5, 6.4, 7.3, 8.1, 8.2, 8.7**

use proptest::prelude::*;
use skillstar_models::providers::{
    FlatProvidersStore, ProviderEntryFlat, ToolActivation, ToolBinding,
};
use std::path::PathBuf;
use tempfile::TempDir;

#[path = "providers_prop_tests/part1.rs"]
mod part1;
#[path = "providers_prop_tests/part2.rs"]
mod part2;
#[path = "providers_prop_tests/part3.rs"]
mod part3;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Create a temp directory with a store file path inside it.
fn setup_temp_store() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("model_providers.json");
    (tmp, path)
}

// ---------------------------------------------------------------------------
// Shared strategies for flat store types
// ---------------------------------------------------------------------------

/// Strategy: generate an arbitrary ToolActivation.
fn arb_tool_activation() -> impl Strategy<Value = ToolActivation> {
    (
        "[a-zA-Z0-9\\-]{1,36}",    // provider_id (UUID-like)
        "[a-zA-Z0-9\\-\\.]{1,50}", // model name
    )
        .prop_map(|(provider_id, model)| ToolActivation {
            provider_id,
            model,
            settings: None,
            last_sync_at: None,
        })
}

/// Strategy: generate a ToolBinding for the tool_activations map — either empty
/// (the "not bound" state) or a single-entry binding.
fn arb_optional_tool_activation() -> impl Strategy<Value = ToolBinding> {
    prop_oneof![
        Just(ToolBinding::default()),
        arb_tool_activation().prop_map(ToolBinding::single),
    ]
}

/// Strategy: generate an arbitrary ProviderEntryFlat.
fn arb_provider_entry_flat() -> impl Strategy<Value = ProviderEntryFlat> {
    (
        "[a-f0-9\\-]{36}",                         // id (UUID format)
        "[a-zA-Z0-9 _\\-]{1,64}",                  // name
        "https://[a-z]{3,12}\\.[a-z]{2,6}/v[0-9]", // base_url_openai
        prop_oneof![
            Just(String::new()),
            "https://[a-z]{3,12}\\.[a-z]{2,6}/anthropic".prop_map(|s| s),
        ], // base_url_anthropic
        "[a-zA-Z0-9\\-_]{0,64}",                   // api_key
        proptest::collection::vec("[a-zA-Z0-9\\-\\.]{1,30}", 0..5), // models
        "[a-zA-Z0-9\\-\\.]{0,30}",                 // default_model
        0u32..100u32,                              // sort_index
    )
        .prop_flat_map(
            |(
                id,
                name,
                base_url_openai,
                base_url_anthropic,
                api_key,
                models,
                default_model,
                sort_index,
            )| {
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
                    proptest::option::of("[a-z\\-]{3,20}"), // preset_id
                    proptest::option::of("#[0-9A-Fa-f]{6}"), // icon_color
                    proptest::option::of("[a-zA-Z0-9 ]{0,100}"), // notes
                    proptest::option::of(1_700_000_000_000u64..1_800_000_000_000u64), // created_at
                )
            },
        )
        .prop_map(
            |(
                id,
                name,
                base_url_openai,
                base_url_anthropic,
                api_key,
                models,
                default_model,
                sort_index,
                preset_id,
                icon_color,
                notes,
                created_at,
            )| {
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
            },
        )
}

/// Strategy: generate an arbitrary FlatProvidersStore.
fn arb_flat_providers_store() -> impl Strategy<Value = FlatProvidersStore> {
    (
        proptest::collection::vec(arb_provider_entry_flat(), 0..10), // providers
        proptest::collection::hash_map(
            "[a-z\\-]{3,15}", // tool_id keys (e.g., "claude-code", "codex")
            arb_optional_tool_activation(),
            0..5,
        ), // tool_activations
    )
        .prop_map(|(providers, tool_activations)| FlatProvidersStore {
            version: skillstar_models::providers::FLAT_STORE_VERSION,
            providers,
            tool_activations,
        })
}

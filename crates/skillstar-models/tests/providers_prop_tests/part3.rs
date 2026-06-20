//! Part 3: Provider List Sort Invariant (Property 2), Preset Creation Produces
//! Valid Provider (Property 12), Failed Operations Leave State Unchanged
//! (Property 9), and Single Active Provider Per Tool (Property 5).
//!
//! **Validates: Requirements 2.1, 3.8, 4.6, 5.2, 5.4, 5.5, 6.4**

use super::arb_flat_providers_store;
use proptest::prelude::*;
use skillstar_models::providers::{
    FlatProvidersStore, ProviderEntryFlat, ProviderPatchFlat, activate_tool,
    create_from_preset_flat, create_provider_flat, delete_provider_flat, get_all_presets_flat,
    reorder_providers, update_provider_flat,
};
use std::collections::HashMap;

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
        proptest::collection::vec(
            "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}",
            n,
        )
        .prop_flat_map(move |ids| {
            // Generate random sort_index values for each provider
            proptest::collection::vec(0u32..1000u32, n).prop_map(move |sort_indices| {
                let providers: Vec<ProviderEntryFlat> = ids
                    .iter()
                    .zip(sort_indices.iter())
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
            "[A-Za-z]{3,16}",                                               // name
            proptest::collection::vec("[a-z]{3,12}".prop_map(|s| s), 1..=3), // models
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
            0..max_providers,                    // provider_idx
            0..TOOL_IDS.len(),                   // tool_id_idx
            proptest::option::of("[a-z]{3,12}"), // optional model override
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

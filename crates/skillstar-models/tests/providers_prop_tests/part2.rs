//! Part 2: Store Serialization Round-Trip (Property 1).
//!
//! **Validates: Requirements 8.1, 8.2**

use super::{arb_flat_providers_store, setup_temp_store};
use proptest::prelude::*;
use skillstar_models::providers::{FlatProvidersStore, read_flat_store, write_flat_store};

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

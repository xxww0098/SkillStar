//! Property-based tests for the providers module.
//!
//! Uses `proptest` to verify correctness properties across arbitrary inputs.

#[cfg(test)]
mod tests {
    use crate::providers::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    /// Helper: create a temp directory with a store file path inside it.
    fn setup_temp_store() -> (TempDir, std::path::PathBuf) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("model_providers.json");
        (tmp, path)
    }

    /// Strategy: generate a non-empty alphanumeric provider ID (1..=32 chars).
    fn arb_provider_id() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9]{1,32}".prop_map(|s| s)
    }

    /// Strategy: generate a valid provider name (1..=64 chars, alphanumeric + spaces).
    fn arb_provider_name() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 ]{1,64}".prop_map(|s| s)
    }

    /// Strategy: generate a valid AppId.
    fn arb_app_id() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("claude".to_string()),
            Just("codex".to_string()),
            Just("opencode".to_string()),
            Just("gemini".to_string()),
        ]
    }

    /// Build a valid ProviderEntry with the given id and name.
    fn make_valid_entry(id: &str, name: &str) -> ProviderEntry {
        let settings = ProviderSettings {
            base_url: "https://api.example.com/v1".to_string(),
            api_key: "sk-test-key-12345".to_string(),
            models: vec![ModelMapping {
                source_model: "model-a".to_string(),
                target_model: "model-a".to_string(),
                enabled: true,
            }],
            timeout_ms: None,
            max_retries: None,
        };
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

    // =========================================================================
    // Property 4: Provider ID Uniqueness Within AppId
    //
    // For any AppId and any existing provider with ID X, attempting to create
    // another provider with the same ID X within the same AppId SHALL fail
    // with a uniqueness error.
    //
    // **Validates: Requirement 2.4**
    // =========================================================================

    proptest! {
        #[test]
        fn prop_provider_id_uniqueness_within_app_id(
            provider_id in arb_provider_id(),
            name1 in arb_provider_name(),
            name2 in arb_provider_name(),
            app_id in arb_app_id(),
        ) {
            let (_tmp, path) = setup_temp_store();

            // Create the first provider with the generated ID
            let entry1 = make_valid_entry(&provider_id, &name1);
            let result1 = create_provider_at(&app_id, entry1, &path);
            prop_assert!(result1.is_ok(), "First creation should succeed, got: {:?}", result1.err());

            // Attempt to create a second provider with the same ID in the same AppId
            let entry2 = make_valid_entry(&provider_id, &name2);
            let result2 = create_provider_at(&app_id, entry2, &path);

            // The second creation MUST fail
            prop_assert!(result2.is_err(), "Second creation with same ID '{}' in app '{}' should fail", provider_id, app_id);

            // The error message should indicate a uniqueness violation
            let err_msg = result2.unwrap_err().to_string();
            prop_assert!(
                err_msg.contains("already exists"),
                "Error should mention 'already exists', got: {}", err_msg
            );
        }
    }

    // =========================================================================
    // Property 5: First Provider Auto-Activation
    //
    // For any AppId that has no providers, creating the first provider SHALL
    // result in that provider being set as the current active provider.
    // Creating a second provider SHALL NOT change the current active provider.
    //
    // **Validates: Requirement 2.5**
    // =========================================================================

    /// Strategy: generate a valid base URL (https scheme with a host).
    fn arb_valid_url() -> impl Strategy<Value = String> {
        ("[a-z]{3,10}", "[a-z]{2,8}")
            .prop_map(|(host, path)| format!("https://{}.example.com/{}", host, path))
    }

    /// Strategy: generate a non-empty model list (1..=3 models).
    fn arb_model_list() -> impl Strategy<Value = Vec<ModelMapping>> {
        prop::collection::vec(
            "[a-z]{3,12}".prop_map(|name| ModelMapping {
                source_model: name.clone(),
                target_model: name,
                enabled: true,
            }),
            1..=3,
        )
    }

    /// Build a valid ProviderEntry with arbitrary generated data.
    fn make_arb_entry(
        id: &str,
        name: &str,
        base_url: &str,
        models: Vec<ModelMapping>,
    ) -> ProviderEntry {
        let settings = ProviderSettings {
            base_url: base_url.to_string(),
            api_key: "sk-test-key-12345".to_string(),
            models,
            timeout_ms: None,
            max_retries: None,
        };
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

    proptest! {
        /// **Validates: Requirement 2.5**
        ///
        /// Property 5: Creating the first provider in an empty store auto-activates it.
        /// Creating a second provider does NOT change the current active provider.
        #[test]
        fn prop_first_provider_auto_activation(
            id1 in arb_provider_id(),
            id2 in arb_provider_id(),
            name1 in arb_provider_name(),
            name2 in arb_provider_name(),
            url1 in arb_valid_url(),
            url2 in arb_valid_url(),
            models1 in arb_model_list(),
            models2 in arb_model_list(),
            app_id in arb_app_id(),
        ) {
            // Ensure distinct IDs so the second create doesn't fail on uniqueness
            prop_assume!(id1 != id2);

            let (_tmp, path) = setup_temp_store();

            // Verify the store starts empty for this app_id
            let store = read_store_from(&path).unwrap();
            let app = match app_id.as_str() {
                "claude" => &store.claude,
                "codex" => &store.codex,
                "opencode" => &store.opencode,
                "gemini" => &store.gemini,
                _ => unreachable!(),
            };
            prop_assert_eq!(app.current.as_deref(), None, "Store should start with no current provider");

            // Create the first provider
            let entry1 = make_arb_entry(&id1, &name1, &url1, models1);
            let result1 = create_provider_at(&app_id, entry1, &path);
            prop_assert!(result1.is_ok(), "First provider creation should succeed, got: {:?}", result1.err());

            // Assert current is set to the first provider's ID
            let store = read_store_from(&path).unwrap();
            let app = match app_id.as_str() {
                "claude" => &store.claude,
                "codex" => &store.codex,
                "opencode" => &store.opencode,
                "gemini" => &store.gemini,
                _ => unreachable!(),
            };
            prop_assert_eq!(
                app.current.as_deref(),
                Some(id1.as_str()),
                "After first provider creation, current should be set to its ID"
            );

            // Create a second provider
            let entry2 = make_arb_entry(&id2, &name2, &url2, models2);
            let result2 = create_provider_at(&app_id, entry2, &path);
            prop_assert!(result2.is_ok(), "Second provider creation should succeed, got: {:?}", result2.err());

            // Assert current is STILL the first provider's ID (not changed)
            let store = read_store_from(&path).unwrap();
            let app = match app_id.as_str() {
                "claude" => &store.claude,
                "codex" => &store.codex,
                "opencode" => &store.opencode,
                "gemini" => &store.gemini,
                _ => unreachable!(),
            };
            prop_assert_eq!(
                app.current.as_deref(),
                Some(id1.as_str()),
                "After second provider creation, current should still be the first provider's ID"
            );
        }
    }

    // =========================================================================
    // Property 16: AppId Isolation
    //
    // For any operation (create, update, delete, switch) performed on providers
    // within one AppId, the providers and current active provider of the other
    // AppId SHALL remain unchanged.
    //
    // **Validates: Requirement 7.5**
    // =========================================================================

    /// Represents a CRUD operation to perform on a single AppId.
    #[derive(Debug, Clone)]
    enum CrudOp {
        Create { id: String, name: String },
        Update { id: String, patch: ProviderPatch },
        Delete { id: String },
    }

    /// Strategy: generate a ProviderPatch for updates (only safe fields).
    fn arb_provider_patch() -> impl Strategy<Value = ProviderPatch> {
        prop_oneof![
            arb_provider_name().prop_map(|name| ProviderPatch {
                name: Some(name),
                ..Default::default()
            }),
            Just(ProviderPatch {
                category: Some("local".to_string()),
                ..Default::default()
            }),
            Just(ProviderPatch {
                notes: Some("updated notes".to_string()),
                ..Default::default()
            }),
        ]
    }

    /// Strategy: generate a sequence of CRUD operations.
    /// Creates 1-3 providers, then optionally updates the first and deletes the last.
    fn arb_crud_ops() -> impl Strategy<Value = Vec<CrudOp>> {
        prop::collection::vec((arb_provider_id(), arb_provider_name()), 1..=3).prop_flat_map(
            |id_name_pairs| {
                let ids: Vec<String> = id_name_pairs.iter().map(|(id, _)| id.clone()).collect();
                let creates: Vec<CrudOp> = id_name_pairs
                    .into_iter()
                    .map(|(id, name)| CrudOp::Create { id, name })
                    .collect();

                let ids_for_ops = ids.clone();
                arb_provider_patch().prop_map(move |patch| {
                    let mut ops = creates.clone();
                    // Add an update on the first provider
                    if let Some(first_id) = ids_for_ops.first() {
                        ops.push(CrudOp::Update {
                            id: first_id.clone(),
                            patch: patch.clone(),
                        });
                    }
                    // Add a delete on the last provider (if more than 1)
                    if ids_for_ops.len() > 1
                        && let Some(last_id) = ids_for_ops.last()
                    {
                        ops.push(CrudOp::Delete {
                            id: last_id.clone(),
                        });
                    }
                    ops
                })
            },
        )
    }

    /// Helper: snapshot an AppProviders state for comparison.
    fn snapshot_app(app: &AppProviders) -> (Vec<(String, String)>, Option<String>) {
        let mut providers: Vec<(String, String)> = app
            .providers
            .iter()
            .map(|(k, v)| (k.clone(), v.name.clone()))
            .collect();
        providers.sort_by(|a, b| a.0.cmp(&b.0));
        (providers, app.current.clone())
    }

    /// Helper: execute a CRUD operation on a given app_id and path.
    /// Returns Ok(()) if the operation succeeded or was expected to fail (e.g. delete non-existent).
    fn execute_op(op: &CrudOp, app_id: &str, path: &std::path::Path) {
        match op {
            CrudOp::Create { id, name } => {
                let entry = make_valid_entry(id, name);
                // Ignore errors (e.g. duplicate ID) — we only care about isolation
                let _ = create_provider_at(app_id, entry, path);
            }
            CrudOp::Update { id, patch } => {
                let _ = update_provider_at(app_id, id, patch.clone(), path);
            }
            CrudOp::Delete { id } => {
                let _ = delete_provider_at(app_id, id, path);
            }
        }
    }

    proptest! {
        /// **Validates: Requirements 7.5**
        ///
        /// Property 16 (part 1): CRUD operations on "claude" do not affect "codex".
        /// 1. Create a provider in "claude"
        /// 2. Assert "codex" is unchanged (empty providers, null current)
        /// 3. Create a provider in "codex"
        /// 4. Perform operations (update, delete) on "claude" and verify "codex" is unaffected
        #[test]
        fn prop_appid_isolation_claude_does_not_affect_codex(
            claude_ops in arb_crud_ops(),
            codex_id in arb_provider_id(),
            codex_name in arb_provider_name(),
        ) {
            let (_tmp, path) = setup_temp_store();

            // Step 1: Execute the first create op for claude
            if let Some(op @ CrudOp::Create { .. }) = claude_ops.first() {
                execute_op(op, "claude", &path);
            }

            // Step 2: Assert "codex" is unchanged (empty providers, null current)
            let store = read_store_from(&path).unwrap();
            prop_assert!(store.codex.providers.is_empty(),
                "Codex providers should be empty after claude create");
            prop_assert_eq!(store.codex.current, None,
                "Codex current should be None after claude create");

            // Step 3: Create a provider in "codex"
            let codex_entry = make_valid_entry(&codex_id, &codex_name);
            let _ = create_provider_at("codex", codex_entry, &path);

            // Snapshot codex state after its own creation
            let store = read_store_from(&path).unwrap();
            let codex_snapshot = snapshot_app(&store.codex);

            // Step 4: Perform remaining operations on "claude" and verify "codex" is unaffected
            for op in claude_ops.iter().skip(1) {
                execute_op(op, "claude", &path);

                // After each claude operation, verify codex is unchanged
                let store = read_store_from(&path).unwrap();
                let codex_current = snapshot_app(&store.codex);
                prop_assert_eq!(
                    &codex_snapshot, &codex_current,
                    "Codex state changed after claude operation {:?}", op
                );
            }
        }

        /// **Validates: Requirements 7.5**
        ///
        /// Property 16 (part 2): CRUD operations on "codex" do not affect "claude".
        /// Vice versa direction.
        #[test]
        fn prop_appid_isolation_codex_does_not_affect_claude(
            codex_ops in arb_crud_ops(),
            claude_id in arb_provider_id(),
            claude_name in arb_provider_name(),
        ) {
            let (_tmp, path) = setup_temp_store();

            // Step 1: Execute the first create op for codex
            if let Some(op @ CrudOp::Create { .. }) = codex_ops.first() {
                execute_op(op, "codex", &path);
            }

            // Step 2: Assert "claude" is unchanged (empty providers, null current)
            let store = read_store_from(&path).unwrap();
            prop_assert!(store.claude.providers.is_empty(),
                "Claude providers should be empty after codex create");
            prop_assert_eq!(store.claude.current, None,
                "Claude current should be None after codex create");

            // Step 3: Create a provider in "claude"
            let claude_entry = make_valid_entry(&claude_id, &claude_name);
            let _ = create_provider_at("claude", claude_entry, &path);

            // Snapshot claude state after its own creation
            let store = read_store_from(&path).unwrap();
            let claude_snapshot = snapshot_app(&store.claude);

            // Step 4: Perform remaining operations on "codex" and verify "claude" is unaffected
            for op in codex_ops.iter().skip(1) {
                execute_op(op, "codex", &path);

                // After each codex operation, verify claude is unchanged
                let store = read_store_from(&path).unwrap();
                let claude_current = snapshot_app(&store.claude);
                prop_assert_eq!(
                    &claude_snapshot, &claude_current,
                    "Claude state changed after codex operation {:?}", op
                );
            }
        }

        /// **Validates: Requirements 7.5**
        ///
        /// Property 16 (part 3): switch_active_provider on one AppId does not affect the other.
        #[test]
        fn prop_appid_isolation_switch_does_not_cross(
            claude_id1 in arb_provider_id(),
            claude_id2 in arb_provider_id(),
            codex_id in arb_provider_id(),
        ) {
            // Ensure distinct IDs for claude providers
            prop_assume!(claude_id1 != claude_id2);

            let (_tmp, path) = setup_temp_store();

            // Setup: create two providers in claude, one in codex
            let entry1 = make_valid_entry(&claude_id1, "Claude P1");
            let entry2 = make_valid_entry(&claude_id2, "Claude P2");
            let codex_entry = make_valid_entry(&codex_id, "Codex Provider");

            create_provider_at("claude", entry1, &path).unwrap();
            create_provider_at("claude", entry2, &path).unwrap();
            create_provider_at("codex", codex_entry, &path).unwrap();

            // Snapshot codex state
            let store = read_store_from(&path).unwrap();
            let codex_snapshot = snapshot_app(&store.codex);

            // Switch active provider in claude
            switch_active_provider_at("claude", &claude_id2, &path).unwrap();

            // Verify codex is unchanged
            let store = read_store_from(&path).unwrap();
            let codex_after = snapshot_app(&store.codex);
            prop_assert_eq!(&codex_snapshot, &codex_after,
                "Codex state changed after switching claude active provider");

            // Switch back in claude
            switch_active_provider_at("claude", &claude_id1, &path).unwrap();

            // Verify codex is still unchanged
            let store = read_store_from(&path).unwrap();
            let codex_after2 = snapshot_app(&store.codex);
            prop_assert_eq!(&codex_snapshot, &codex_after2,
                "Codex state changed after switching claude active provider back");
        }
    }
}

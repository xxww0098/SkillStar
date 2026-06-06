use super::*;

    #[test]
    fn test_create_from_preset_flat_relay_empty_models() {
        let result = create_from_preset_flat("openrouter", "or-key").unwrap();
        assert_eq!(result.name, "OpenRouter");
        assert!(result.models.is_empty());
        assert_eq!(result.default_model, "");
        assert_eq!(result.base_url_openai, "https://openrouter.ai/api/v1");
        assert!(result.base_url_anthropic.is_empty());
    }
    #[test]
    fn test_create_from_preset_flat_invalid_preset_id() {
        let result = create_from_preset_flat("nonexistent-preset", "key");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
    #[test]
    fn test_create_from_preset_flat_all_presets_succeed() {
        let presets = get_all_presets_flat();
        for preset in &presets {
            let result = create_from_preset_flat(&preset.id, "test-api-key");
            assert!(
                result.is_ok(),
                "Failed to create provider from preset '{}': {:?}",
                preset.id,
                result.err()
            );
            let entry = result.unwrap();
            assert_eq!(entry.name, preset.name);
            assert_eq!(entry.base_url_openai, preset.base_url_openai);
            assert_eq!(entry.base_url_anthropic, preset.base_url_anthropic);
            assert!(entry.models.is_empty());
            assert!(entry.default_model.is_empty());
            assert_eq!(entry.icon_color, Some(preset.icon_color.clone()));
            assert_eq!(entry.preset_id, Some(preset.id.clone()));
            assert!(entry.created_at.is_some());
            assert!(uuid::Uuid::parse_str(&entry.id).is_ok());
        }
    }
    #[test]
    fn test_create_provider_flat_basic() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("My Provider");
        let result = create_provider_flat(&mut store, entry).unwrap();

        assert_eq!(result.name, "My Provider");
        assert!(uuid::Uuid::parse_str(&result.id).is_ok());
        assert!(result.created_at.is_some());
        assert_eq!(result.sort_index, 0);
        assert_eq!(store.providers.len(), 1);
        assert_eq!(store.providers[0].id, result.id);
    }
    #[test]
    fn test_create_provider_flat_empty_name() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("");
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name must not be empty"));
    }
    #[test]
    fn test_create_provider_flat_whitespace_name() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("   ");
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name must not be empty"));
    }
    #[test]
    fn test_create_provider_flat_invalid_url() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.base_url_openai = "not-a-url".to_string();
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid URL"));
    }
    #[test]
    fn test_create_provider_flat_invalid_scheme() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.base_url_openai = "ftp://api.example.com/v1".to_string();
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("http or https"));
    }
    #[test]
    fn test_create_provider_flat_empty_url_allowed() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.base_url_anthropic = String::new();
        let result = create_provider_flat(&mut store, entry);
        assert!(result.is_ok());
    }
    #[test]
    fn test_create_provider_flat_generates_uuid() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.id = "user-provided-id".to_string();
        let result = create_provider_flat(&mut store, entry).unwrap();
        // ID should be overwritten with a valid UUID
        assert_ne!(result.id, "user-provided-id");
        assert!(uuid::Uuid::parse_str(&result.id).is_ok());
    }
    #[test]
    fn test_create_provider_flat_sets_created_at() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Test");
        let result = create_provider_flat(&mut store, entry).unwrap();
        assert!(result.created_at.is_some());
        assert!(result.created_at.unwrap() > 0);
    }
    #[test]
    fn test_create_provider_flat_preserves_existing_created_at() {
        let mut store = FlatProvidersStore::default();
        let mut entry = make_flat_entry("Test");
        entry.created_at = Some(1719000000000);
        let result = create_provider_flat(&mut store, entry).unwrap();
        assert_eq!(result.created_at, Some(1719000000000));
    }
    #[test]
    fn test_create_provider_flat_sort_index_increments() {
        let mut store = FlatProvidersStore::default();

        let entry1 = make_flat_entry("First");
        let result1 = create_provider_flat(&mut store, entry1).unwrap();
        assert_eq!(result1.sort_index, 0);

        let entry2 = make_flat_entry("Second");
        let result2 = create_provider_flat(&mut store, entry2).unwrap();
        assert_eq!(result2.sort_index, 1);

        let entry3 = make_flat_entry("Third");
        let result3 = create_provider_flat(&mut store, entry3).unwrap();
        assert_eq!(result3.sort_index, 2);
    }
    #[test]
    fn test_update_provider_flat_basic() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Original");
        let created = create_provider_flat(&mut store, entry).unwrap();

        let patch = ProviderPatchFlat {
            name: Some("Updated".to_string()),
            ..Default::default()
        };
        let updated = update_provider_flat(&mut store, &created.id, patch).unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.id, created.id);
        // Other fields unchanged
        assert_eq!(updated.base_url_openai, "https://api.example.com/v1");
    }
    #[test]
    fn test_update_provider_flat_multiple_fields() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Original");
        let created = create_provider_flat(&mut store, entry).unwrap();

        let patch = ProviderPatchFlat {
            name: Some("New Name".to_string()),
            base_url_openai: Some("https://new-api.com/v1".to_string()),
            api_key: Some("new-key".to_string()),
            models: Some(vec!["new-model".to_string()]),
            default_model: Some("new-model".to_string()),
            icon_color: Some("#FF0000".to_string()),
            notes: Some("Some notes".to_string()),
            ..Default::default()
        };
        let updated = update_provider_flat(&mut store, &created.id, patch).unwrap();
        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.base_url_openai, "https://new-api.com/v1");
        assert_eq!(updated.api_key, "new-key");
        assert_eq!(updated.models, vec!["new-model"]);
        assert_eq!(updated.default_model, "new-model");
        assert_eq!(updated.icon_color, Some("#FF0000".to_string()));
        assert_eq!(updated.notes, Some("Some notes".to_string()));
    }
    #[test]
    fn test_update_provider_flat_not_found() {
        let mut store = FlatProvidersStore::default();
        let patch = ProviderPatchFlat {
            name: Some("New".to_string()),
            ..Default::default()
        };
        let result = update_provider_flat(&mut store, "nonexistent", patch);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
    #[test]
    fn test_delete_provider_flat_basic() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("To Delete");
        let created = create_provider_flat(&mut store, entry).unwrap();
        assert_eq!(store.providers.len(), 1);

        delete_provider_flat(&mut store, &created.id).unwrap();
        assert_eq!(store.providers.len(), 0);
    }
    #[test]
    fn test_delete_provider_flat_not_found() {
        let mut store = FlatProvidersStore::default();
        let result = delete_provider_flat(&mut store, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
    #[test]
    fn test_delete_provider_flat_cleans_tool_activations() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Active Provider");
        let created = create_provider_flat(&mut store, entry).unwrap();

        // Set up tool_activations referencing this provider
        store.tool_activations.insert(
            "claude-code".to_string(),
            Some(ToolActivation {
                provider_id: created.id.clone(),
                model: "model-a".to_string(),
                settings: None,
                last_sync_at: None,
            }),
        );
        store.tool_activations.insert(
            "codex".to_string(),
            Some(ToolActivation {
                provider_id: created.id.clone(),
                model: "model-a".to_string(),
                settings: None,
                last_sync_at: None,
            }),
        );

        delete_provider_flat(&mut store, &created.id).unwrap();

        // Both activations should be cleared
        assert_eq!(store.tool_activations.get("claude-code").unwrap(), &None);
        assert_eq!(store.tool_activations.get("codex").unwrap(), &None);
    }
    #[test]
    fn test_delete_provider_flat_preserves_other_activations() {
        let mut store = FlatProvidersStore::default();
        let entry1 = make_flat_entry("Provider 1");
        let entry2 = make_flat_entry("Provider 2");
        let created1 = create_provider_flat(&mut store, entry1).unwrap();
        let created2 = create_provider_flat(&mut store, entry2).unwrap();

        // Set up tool_activations: claude-code → provider1, codex → provider2
        store.tool_activations.insert(
            "claude-code".to_string(),
            Some(ToolActivation {
                provider_id: created1.id.clone(),
                model: "model-a".to_string(),
                settings: None,
                last_sync_at: None,
            }),
        );
        store.tool_activations.insert(
            "codex".to_string(),
            Some(ToolActivation {
                provider_id: created2.id.clone(),
                model: "model-a".to_string(),
                settings: None,
                last_sync_at: None,
            }),
        );

        // Delete provider1 — only claude-code should be cleared
        delete_provider_flat(&mut store, &created1.id).unwrap();

        assert_eq!(store.tool_activations.get("claude-code").unwrap(), &None);
        let codex_act = store.tool_activations.get("codex").unwrap().as_ref().unwrap();
        assert_eq!(codex_act.provider_id, created2.id);
    }
    #[test]
    fn test_reorder_providers_basic() {
        let mut store = FlatProvidersStore::default();
        let entry1 = make_flat_entry("First");
        let entry2 = make_flat_entry("Second");
        let entry3 = make_flat_entry("Third");
        let created1 = create_provider_flat(&mut store, entry1).unwrap();
        let created2 = create_provider_flat(&mut store, entry2).unwrap();
        let created3 = create_provider_flat(&mut store, entry3).unwrap();

        // Reorder: Third, First, Second
        let ordered_ids = vec![
            created3.id.clone(),
            created1.id.clone(),
            created2.id.clone(),
        ];
        reorder_providers(&mut store, &ordered_ids).unwrap();

        // Verify sort_index assignments
        let p1 = store.providers.iter().find(|p| p.id == created1.id).unwrap();
        let p2 = store.providers.iter().find(|p| p.id == created2.id).unwrap();
        let p3 = store.providers.iter().find(|p| p.id == created3.id).unwrap();
        assert_eq!(p3.sort_index, 0);
        assert_eq!(p1.sort_index, 1);
        assert_eq!(p2.sort_index, 2);
    }
    #[test]
    fn test_reorder_providers_partial() {
        let mut store = FlatProvidersStore::default();
        let entry1 = make_flat_entry("First");
        let entry2 = make_flat_entry("Second");
        let entry3 = make_flat_entry("Third");
        let created1 = create_provider_flat(&mut store, entry1).unwrap();
        let created2 = create_provider_flat(&mut store, entry2).unwrap();
        let created3 = create_provider_flat(&mut store, entry3).unwrap();

        // Only reorder two of three — Third keeps its existing sort_index
        let ordered_ids = vec![created2.id.clone(), created1.id.clone()];
        reorder_providers(&mut store, &ordered_ids).unwrap();

        let p1 = store.providers.iter().find(|p| p.id == created1.id).unwrap();
        let p2 = store.providers.iter().find(|p| p.id == created2.id).unwrap();
        let p3 = store.providers.iter().find(|p| p.id == created3.id).unwrap();
        assert_eq!(p2.sort_index, 0);
        assert_eq!(p1.sort_index, 1);
        // Third keeps its original sort_index (2)
        assert_eq!(p3.sort_index, 2);
    }
    #[test]
    fn test_reorder_providers_invalid_id() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Test");
        create_provider_flat(&mut store, entry).unwrap();

        let ordered_ids = vec!["nonexistent-id".to_string()];
        let result = reorder_providers(&mut store, &ordered_ids);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
    #[test]
    fn test_reorder_providers_empty_list() {
        let mut store = FlatProvidersStore::default();
        let entry = make_flat_entry("Test");
        let created = create_provider_flat(&mut store, entry).unwrap();

        // Empty reorder list — no changes
        reorder_providers(&mut store, &[]).unwrap();

        let p = store.providers.iter().find(|p| p.id == created.id).unwrap();
        assert_eq!(p.sort_index, 0); // Unchanged
    }
    #[test]
    fn test_app_id_isolation() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Claude Provider");
        create_provider_at("claude", entry, &path).unwrap();

        // Codex should be unaffected
        let store = read_store_from(&path).unwrap();
        assert!(store.codex.providers.is_empty());
        assert_eq!(store.codex.current, None);
    }
    #[test]
    fn test_app_id_isolation_bidirectional() {
        let (_tmp, path) = setup_temp_store();
        let entry1 = make_valid_entry("p1", "Claude Provider");
        let entry2 = make_valid_entry("p2", "Codex Provider");
        create_provider_at("claude", entry1, &path).unwrap();
        create_provider_at("codex", entry2, &path).unwrap();

        // Delete from claude should not affect codex
        delete_provider_at("claude", "p1", &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert!(store.claude.providers.is_empty());
        assert_eq!(store.codex.providers.len(), 1);
        assert_eq!(store.codex.current, Some("p2".to_string()));
    }
    #[test]
    fn test_unknown_app_id() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Test");
        let result = create_provider_at("unknown_app", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown app_id"));
    }
    #[tokio::test]
    async fn prop_concurrent_write_serialization() {
        use std::sync::Arc;

        let (_tmp, path) = setup_temp_store();
        let path = Arc::new(path);
        let num_tasks = 10;

        // Spawn multiple concurrent create_provider_at calls with unique IDs
        let mut handles = Vec::new();
        for i in 0..num_tasks {
            let p = Arc::clone(&path);
            handles.push(tokio::spawn(async move {
                let id = format!("concurrent-provider-{}", i);
                let name = format!("Provider {}", i);
                let entry = ProviderEntry {
                    id: id.clone(),
                    name,
                    category: "cloud".to_string(),
                    settings_config: serde_json::to_value(ProviderSettings {
                        base_url: "https://api.example.com/v1".to_string(),
                        api_key: format!("sk-key-{}", i),
                        models: vec![ModelMapping {
                            source_model: format!("model-{}", i),
                            target_model: format!("model-{}", i),
                            enabled: true,
                        }],
                        timeout_ms: None,
                        max_retries: None,
                    })
                    .unwrap(),
                    preset_id: None,
                    website_url: None,
                    api_key_url: None,
                    icon_color: None,
                    notes: None,
                    created_at: None,
                    sort_index: None,
                    meta: None,
                };
                let result = create_provider_at("claude", entry, &p);
                (id, result.is_ok())
            }));
        }

        // Collect results
        let mut successful_ids: Vec<String> = Vec::new();
        for handle in handles {
            let (id, ok) = handle.await.unwrap();
            if ok {
                successful_ids.push(id);
            }
        }

        // Assertion 1: The store file is valid JSON (no corruption)
        let raw_content = std::fs::read_to_string(path.as_ref())
            .expect("Store file should exist after concurrent writes");
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&raw_content);
        assert!(
            parsed.is_ok(),
            "Store file must be valid JSON after concurrent writes, but got parse error: {:?}",
            parsed.err()
        );

        // Assertion 2: The store deserializes to a valid ProvidersStore
        let store = read_store_from(path.as_ref())
            .expect("Store should be readable after concurrent writes");

        // Assertion 3: All successfully created providers are present in the store
        for id in &successful_ids {
            assert!(
                store.claude.providers.contains_key(id),
                "Successfully created provider '{}' should be present in the store",
                id
            );
        }

        // Assertion 4: The store is internally consistent
        // If current is set, it must reference a valid provider
        if let Some(ref current_id) = store.claude.current {
            assert!(
                store.claude.providers.contains_key(current_id),
                "current '{}' must reference an existing provider. Existing: {:?}",
                current_id,
                store.claude.providers.keys().collect::<Vec<_>>()
            );
        }

        // Assertion 5: At least one provider was created successfully
        // (demonstrates the race condition may cause some to fail, but not all)
        assert!(
            !successful_ids.is_empty(),
            "At least one concurrent create should succeed"
        );

        // Note: Without external locking (like the Tauri Mutex), some creates may
        // fail due to read-modify-write races. The key property is that the final
        // state is still valid JSON with no corruption, even if not all creates
        // succeeded. This demonstrates why the Mutex is needed at the command layer.
    }
    #[test]
    fn test_migrate_store_if_needed_file_not_found() {
        let (_tmp, path) = setup_temp_store();
        let store = migrate_store_if_needed(&path).unwrap();
        assert_eq!(store.version, 2);
        assert!(store.providers.is_empty());
        assert!(store.tool_activations.is_empty());
    }
    #[test]
    fn test_migrate_store_if_needed_already_v2() {
        let (_tmp, path) = setup_temp_store();
        let original = FlatProvidersStore {
            version: 2,
            providers: vec![ProviderEntryFlat {
                id: "existing-id".to_string(),
                name: "Existing Provider".to_string(),
                base_url_openai: "https://api.example.com/v1".to_string(),
                base_url_anthropic: String::new(),
                models_url: String::new(),
                api_key: "sk-key".to_string(),
                models: vec!["model-a".to_string()],
                default_model: "model-a".to_string(),
                sort_index: 0,
                preset_id: None,
                icon_color: None,
                notes: None,
                created_at: Some(1719000000000),
                meta: None,
                codex_wire_api: "responses".to_string(),
                codex_auth_mode: "api_key".to_string(),
            }],
            tool_activations: {
                let mut map = HashMap::new();
                map.insert(
                    "claude-code".to_string(),
                    Some(ToolActivation {
                        provider_id: "existing-id".to_string(),
                        model: "model-a".to_string(),
                        settings: None,
                        last_sync_at: None,
                    }),
                );
                map
            },
        };
        write_flat_store(&original, &path).unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        assert_eq!(result.version, 2);
        assert_eq!(result.providers.len(), 1);
        assert_eq!(result.providers[0].id, "existing-id");
        assert_eq!(result.providers[0].name, "Existing Provider");
        assert_eq!(
            result.tool_activations.get("claude-code").unwrap().as_ref().unwrap().provider_id,
            "existing-id"
        );
    }
    #[test]
    fn test_migrate_store_if_needed_v1_basic() {
        let (_tmp, path) = setup_temp_store();

        // Write a v1 store
        let mut store = ProvidersStore::default();
        let entry = make_valid_entry("p1", "DeepSeek");
        store.claude.providers.insert("p1".to_string(), entry);
        store.claude.current = Some("p1".to_string());
        write_store_to(&store, &path).unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        assert_eq!(result.version, 2);
        assert_eq!(result.providers.len(), 1);
        assert_eq!(result.providers[0].name, "DeepSeek");
        assert_eq!(result.providers[0].base_url_openai, "https://api.example.com/v1");
        assert_eq!(result.providers[0].api_key, "sk-test-key-12345");
        assert_eq!(result.providers[0].models, vec!["model-a"]);

        // tool_activations should map claude → claude-code
        let claude_activation = result.tool_activations.get("claude-code");
        assert!(claude_activation.is_some());
        let activation = claude_activation.unwrap().as_ref().unwrap();
        assert_eq!(activation.provider_id, result.providers[0].id);
        assert_eq!(activation.model, "model-a");
    }
    #[test]
    fn test_migrate_store_if_needed_v1_deduplication() {
        let (_tmp, path) = setup_temp_store();

        // Create a v1 store with the same provider in both claude and codex
        let mut store = ProvidersStore::default();
        let entry_claude = make_valid_entry("p1", "Shared Provider");
        let entry_codex = make_valid_entry("p2", "Shared Provider");
        // Both have the same base_url and api_key (from make_valid_entry)
        store.claude.providers.insert("p1".to_string(), entry_claude);
        store.claude.current = Some("p1".to_string());
        store.codex.providers.insert("p2".to_string(), entry_codex);
        store.codex.current = Some("p2".to_string());
        write_store_to(&store, &path).unwrap();

        let result = migrate_store_if_needed(&path).unwrap();
        // Should be deduplicated to 1 provider (same base_url + api_key)
        assert_eq!(result.providers.len(), 1);

        // Both tool_activations should point to the same provider
        let claude_act = result.tool_activations.get("claude-code")
            .unwrap().as_ref().unwrap();
        let codex_act = result.tool_activations.get("codex")
            .unwrap().as_ref().unwrap();
        assert_eq!(claude_act.provider_id, codex_act.provider_id);
    }

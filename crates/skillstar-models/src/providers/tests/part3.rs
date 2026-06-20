use super::*;

#[test]
fn test_migrate_store_if_needed_v1_different_providers() {
    let (_tmp, path) = setup_temp_store();

    // Create a v1 store with different providers in claude and codex
    let mut store = ProvidersStore::default();

    let settings_claude = ProviderSettings {
        base_url: "https://api.deepseek.com/v1".to_string(),
        api_key: "sk-deepseek".to_string(),
        models: vec![ModelMapping {
            source_model: "deepseek-chat".to_string(),
            target_model: "deepseek-chat".to_string(),
            enabled: true,
        }],
        timeout_ms: None,
        max_retries: None,
    };
    let entry_claude = ProviderEntry {
        id: "p1".to_string(),
        name: "DeepSeek".to_string(),
        category: "cloud".to_string(),
        settings_config: serde_json::to_value(&settings_claude).unwrap(),
        preset_id: Some("deepseek".to_string()),
        website_url: None,
        api_key_url: None,
        icon_color: Some("#4D6BFE".to_string()),
        notes: None,
        created_at: Some(1719000000000),
        sort_index: Some(0),
        meta: None,
    };

    let settings_codex = ProviderSettings {
        base_url: "https://api.openai.com/v1".to_string(),
        api_key: "sk-openai".to_string(),
        models: vec![ModelMapping {
            source_model: "gpt-4".to_string(),
            target_model: "gpt-4".to_string(),
            enabled: true,
        }],
        timeout_ms: None,
        max_retries: None,
    };
    let entry_codex = ProviderEntry {
        id: "p2".to_string(),
        name: "OpenAI".to_string(),
        category: "cloud".to_string(),
        settings_config: serde_json::to_value(&settings_codex).unwrap(),
        preset_id: Some("official".to_string()),
        website_url: None,
        api_key_url: None,
        icon_color: Some("#10A37F".to_string()),
        notes: None,
        created_at: Some(1719000000000),
        sort_index: Some(0),
        meta: None,
    };

    store
        .claude
        .providers
        .insert("p1".to_string(), entry_claude);
    store.claude.current = Some("p1".to_string());
    store.codex.providers.insert("p2".to_string(), entry_codex);
    store.codex.current = Some("p2".to_string());
    write_store_to(&store, &path).unwrap();

    let result = migrate_store_if_needed(&path).unwrap();
    // Should have 2 distinct providers (different base_url + api_key)
    assert_eq!(result.providers.len(), 2);

    // Verify tool_activations point to different providers
    let claude_act = result
        .tool_activations
        .get("claude-code")
        .unwrap()
        .as_ref()
        .unwrap();
    let codex_act = result
        .tool_activations
        .get("codex")
        .unwrap()
        .as_ref()
        .unwrap();
    assert_ne!(claude_act.provider_id, codex_act.provider_id);

    // Verify the correct models
    assert_eq!(claude_act.model, "deepseek-chat");
    assert_eq!(codex_act.model, "gpt-4");
}
#[test]
fn test_migrate_store_if_needed_creates_backup() {
    let (_tmp, path) = setup_temp_store();

    // Write a v1 store
    let mut store = ProvidersStore::default();
    let entry = make_valid_entry("p1", "Test");
    store.claude.providers.insert("p1".to_string(), entry);
    write_store_to(&store, &path).unwrap();

    // Read original content for comparison
    let original_content = std::fs::read_to_string(&path).unwrap();

    // Migrate
    migrate_store_if_needed(&path).unwrap();

    // Verify backup was created
    let backup_path = path.with_extension("json.bak");
    assert!(backup_path.exists(), "Backup file should be created");

    // Verify backup content matches original
    let backup_content = std::fs::read_to_string(&backup_path).unwrap();
    assert_eq!(backup_content, original_content);
}
#[test]
fn test_migrate_store_if_needed_malformed_json() {
    let (_tmp, path) = setup_temp_store();
    std::fs::write(&path, "not valid json {{{").unwrap();

    let result = migrate_store_if_needed(&path).unwrap();
    assert_eq!(result.version, 2);
    assert!(result.providers.is_empty());
}
#[test]
fn test_migrate_store_if_needed_model_merging() {
    let (_tmp, path) = setup_temp_store();

    // Create a v1 store where the same provider (same base_url + api_key)
    // appears in both apps but with different models
    let mut store = ProvidersStore::default();

    let settings1 = ProviderSettings {
        base_url: "https://api.deepseek.com/v1".to_string(),
        api_key: "sk-shared".to_string(),
        models: vec![ModelMapping {
            source_model: "deepseek-chat".to_string(),
            target_model: "deepseek-chat".to_string(),
            enabled: true,
        }],
        timeout_ms: None,
        max_retries: None,
    };
    let entry1 = ProviderEntry {
        id: "p1".to_string(),
        name: "DeepSeek (Claude)".to_string(),
        category: "cloud".to_string(),
        settings_config: serde_json::to_value(&settings1).unwrap(),
        preset_id: None,
        website_url: None,
        api_key_url: None,
        icon_color: None,
        notes: None,
        created_at: None,
        sort_index: None,
        meta: None,
    };

    let settings2 = ProviderSettings {
        base_url: "https://api.deepseek.com/v1".to_string(),
        api_key: "sk-shared".to_string(),
        models: vec![
            ModelMapping {
                source_model: "deepseek-chat".to_string(),
                target_model: "deepseek-chat".to_string(),
                enabled: true,
            },
            ModelMapping {
                source_model: "deepseek-reasoner".to_string(),
                target_model: "deepseek-reasoner".to_string(),
                enabled: true,
            },
        ],
        timeout_ms: None,
        max_retries: None,
    };
    let entry2 = ProviderEntry {
        id: "p2".to_string(),
        name: "DeepSeek (Codex)".to_string(),
        category: "cloud".to_string(),
        settings_config: serde_json::to_value(&settings2).unwrap(),
        preset_id: None,
        website_url: None,
        api_key_url: None,
        icon_color: None,
        notes: None,
        created_at: None,
        sort_index: None,
        meta: None,
    };

    store.claude.providers.insert("p1".to_string(), entry1);
    store.codex.providers.insert("p2".to_string(), entry2);
    write_store_to(&store, &path).unwrap();

    let result = migrate_store_if_needed(&path).unwrap();
    // Should be deduplicated to 1 provider
    assert_eq!(result.providers.len(), 1);
    // Models should be merged (deepseek-chat + deepseek-reasoner)
    assert!(
        result.providers[0]
            .models
            .contains(&"deepseek-chat".to_string())
    );
    assert!(
        result.providers[0]
            .models
            .contains(&"deepseek-reasoner".to_string())
    );
}
#[test]
fn test_migrate_store_if_needed_no_current() {
    let (_tmp, path) = setup_temp_store();

    // Write a v1 store with no current set
    let mut store = ProvidersStore::default();
    let entry = make_valid_entry("p1", "Test");
    store.claude.providers.insert("p1".to_string(), entry);
    // current is None
    write_store_to(&store, &path).unwrap();

    let result = migrate_store_if_needed(&path).unwrap();
    assert_eq!(result.providers.len(), 1);
    // No tool_activations should be set for claude-code
    let claude_act = result.tool_activations.get("claude-code");
    assert!(claude_act.is_none() || claude_act.unwrap().is_none());
}
// -----------------------------------------------------------------------
// Property 7: Active Provider Validity Invariant
//
// For any store state, if `current` is not null, it must reference an
// existing provider ID. Delete the active provider, assert `current`
// becomes null.
//
// **Validates: Requirements 2.8, 4.1**
// -----------------------------------------------------------------------
proptest! {
    #[test]
    fn prop_active_provider_validity_invariant(
        app_id in arb_app_id(),
        count in arb_provider_count(),
        names in prop::collection::vec(arb_provider_name(), 1..=5),
    ) {
        let (_tmp, path) = setup_temp_store();

        // Create `count` providers (capped by available names)
        let actual_count = count.min(names.len());
        let mut created_ids: Vec<String> = Vec::new();

        for i in 0..actual_count {
            let id = format!("provider-{}", i);
            let entry = make_valid_entry(&id, &names[i]);
            let result = create_provider_at(&app_id, entry, &path);
            prop_assert!(result.is_ok(), "Failed to create provider {}: {:?}", i, result.err());
            created_ids.push(id);
        }

        // Read the store and verify the invariant:
        // If current is Some, it must reference an existing provider ID
        let store = read_store_from(&path).unwrap();
        let app = get_app(&store, &app_id);

        if let Some(ref current_id) = app.current {
            prop_assert!(
                app.providers.contains_key(current_id),
                "current '{}' does not reference an existing provider. Existing IDs: {:?}",
                current_id,
                app.providers.keys().collect::<Vec<_>>()
            );
        }

        // The first provider should have been auto-activated
        prop_assert_eq!(app.current.as_deref(), Some(created_ids[0].as_str()));

        // Now delete the active provider
        let active_id = app.current.clone().unwrap();
        let delete_result = delete_provider_at(&app_id, &active_id, &path);
        prop_assert!(delete_result.is_ok(), "Failed to delete active provider: {:?}", delete_result.err());

        // After deleting the active provider, current must become None
        let store_after = read_store_from(&path).unwrap();
        let app_after = get_app(&store_after, &app_id);
        prop_assert_eq!(
            app_after.current.clone(), None,
            "current should be None after deleting the active provider, but got {:?}",
            app_after.current
        );

        // Verify the invariant still holds: if current is Some, it references an existing ID
        // (In this case current is None, so the invariant trivially holds)
        if let Some(ref current_id) = app_after.current {
            prop_assert!(
                app_after.providers.contains_key(current_id),
                "After deletion, current '{}' does not reference an existing provider",
                current_id
            );
        }
    }
}
#[test]
fn test_activate_tool_success_claude_code() {
    let mut store = FlatProvidersStore::default();
    let entry = make_flat_entry("DeepSeek");
    let created = create_provider_flat(&mut store, entry).unwrap();

    let activation = activate_tool(
        &mut store,
        &created.id,
        "claude-code",
        Some("deepseek-chat"),
        None,
    )
    .unwrap();
    assert_eq!(activation.provider_id, created.id);
    assert_eq!(activation.model, "deepseek-chat");

    // Verify it's in the store
    let stored = store
        .tool_activations
        .get("claude-code")
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(stored.provider_id, created.id);
    assert_eq!(stored.model, "deepseek-chat");
}
#[test]
fn test_activate_tool_success_codex() {
    let mut store = FlatProvidersStore::default();
    let entry = make_flat_entry("DeepSeek");
    let created = create_provider_flat(&mut store, entry).unwrap();

    let activation = activate_tool(
        &mut store,
        &created.id,
        "codex",
        Some("deepseek-reasoner"),
        None,
    )
    .unwrap();
    assert_eq!(activation.provider_id, created.id);
    assert_eq!(activation.model, "deepseek-reasoner");

    let stored = store
        .tool_activations
        .get("codex")
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(stored.provider_id, created.id);
    assert_eq!(stored.model, "deepseek-reasoner");
}
#[test]
fn test_activate_tool_falls_back_to_default_model() {
    let mut store = FlatProvidersStore::default();
    let mut entry = make_flat_entry("DeepSeek");
    entry.default_model = "deepseek-chat".to_string();
    let created = create_provider_flat(&mut store, entry).unwrap();

    // No model provided — should use default_model
    let activation = activate_tool(&mut store, &created.id, "codex", None, None).unwrap();
    assert_eq!(activation.model, "deepseek-chat");
}
#[test]
fn test_activate_tool_empty_model_falls_back_to_default() {
    let mut store = FlatProvidersStore::default();
    let mut entry = make_flat_entry("DeepSeek");
    entry.default_model = "deepseek-chat".to_string();
    let created = create_provider_flat(&mut store, entry).unwrap();

    // Empty string model — should use default_model
    let activation = activate_tool(&mut store, &created.id, "codex", Some(""), None).unwrap();
    assert_eq!(activation.model, "deepseek-chat");
}
#[test]
fn test_activate_tool_whitespace_model_falls_back_to_default() {
    let mut store = FlatProvidersStore::default();
    let mut entry = make_flat_entry("DeepSeek");
    entry.default_model = "deepseek-chat".to_string();
    let created = create_provider_flat(&mut store, entry).unwrap();

    let activation = activate_tool(&mut store, &created.id, "codex", Some("   "), None).unwrap();
    assert_eq!(activation.model, "deepseek-chat");
}
#[test]
fn test_activate_tool_provider_not_found() {
    let mut store = FlatProvidersStore::default();
    let result = activate_tool(&mut store, "nonexistent-id", "claude-code", None, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}
#[test]
fn test_activate_tool_claude_code_empty_anthropic_url() {
    let mut store = FlatProvidersStore::default();
    let mut entry = make_flat_entry("OpenRouter");
    entry.base_url_anthropic = String::new(); // No Anthropic endpoint
    let created = create_provider_flat(&mut store, entry).unwrap();

    let result = activate_tool(&mut store, &created.id, "claude-code", None, None);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Anthropic-compatible endpoint"));
    assert!(err_msg.contains("base_url_anthropic is empty"));
}
#[test]
fn test_activate_tool_claude_code_whitespace_anthropic_url() {
    let mut store = FlatProvidersStore::default();
    // Directly insert a provider with whitespace-only anthropic URL
    // (bypassing create_provider_flat validation to test activate_tool's own validation)
    store.providers.push(ProviderEntryFlat {
        id: "test-id".to_string(),
        name: "Test".to_string(),
        base_url_openai: "https://api.example.com/v1".to_string(),
        base_url_anthropic: "   ".to_string(),
        models_url: String::new(),
        api_key: "sk-key".to_string(),
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
    });

    let result = activate_tool(&mut store, "test-id", "claude-code", None, None);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Anthropic-compatible endpoint")
    );
}
#[test]
fn test_activate_tool_codex_empty_openai_url() {
    let mut store = FlatProvidersStore::default();
    let mut entry = make_flat_entry("Test");
    entry.base_url_openai = String::new(); // No OpenAI endpoint
    entry.base_url_anthropic = "https://api.example.com/anthropic".to_string();
    let created = create_provider_flat(&mut store, entry).unwrap();

    let result = activate_tool(&mut store, &created.id, "codex", None, None);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("OpenAI-compatible endpoint"));
    assert!(err_msg.contains("base_url_openai is empty"));
}
#[test]
fn test_activate_tool_other_tool_empty_openai_url() {
    let mut store = FlatProvidersStore::default();
    let mut entry = make_flat_entry("Test");
    entry.base_url_openai = String::new();
    entry.base_url_anthropic = "https://api.example.com/anthropic".to_string();
    let created = create_provider_flat(&mut store, entry).unwrap();

    // Unknown tool defaults to requiring base_url_openai
    let result = activate_tool(&mut store, &created.id, "some-other-tool", None, None);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("OpenAI-compatible endpoint")
    );
}
#[test]
fn test_activate_tool_replaces_previous_activation() {
    let mut store = FlatProvidersStore::default();
    let entry1 = make_flat_entry("Provider A");
    let entry2 = make_flat_entry("Provider B");
    let created1 = create_provider_flat(&mut store, entry1).unwrap();
    let created2 = create_provider_flat(&mut store, entry2).unwrap();

    // Activate provider A for claude-code
    activate_tool(
        &mut store,
        &created1.id,
        "claude-code",
        Some("model-a"),
        None,
    )
    .unwrap();
    let stored = store
        .tool_activations
        .get("claude-code")
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(stored.provider_id, created1.id);

    // Activate provider B for claude-code — should replace A
    activate_tool(
        &mut store,
        &created2.id,
        "claude-code",
        Some("model-b"),
        None,
    )
    .unwrap();
    let stored = store
        .tool_activations
        .get("claude-code")
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(stored.provider_id, created2.id);
    assert_eq!(stored.model, "model-b");

    // Only one activation for claude-code
    let activations_for_claude: Vec<_> = store
        .tool_activations
        .iter()
        .filter(|(k, v)| *k == "claude-code" && v.is_some())
        .collect();
    assert_eq!(activations_for_claude.len(), 1);
}
#[test]
fn test_deactivate_tool_returns_previous() {
    let mut store = FlatProvidersStore::default();
    let entry = make_flat_entry("DeepSeek");
    let created = create_provider_flat(&mut store, entry).unwrap();

    // Activate first
    activate_tool(
        &mut store,
        &created.id,
        "claude-code",
        Some("deepseek-chat"),
        None,
    )
    .unwrap();

    // Deactivate — should return the previous activation
    let previous = deactivate_tool(&mut store, "claude-code").unwrap();
    assert!(previous.is_some());
    let prev = previous.unwrap();
    assert_eq!(prev.provider_id, created.id);
    assert_eq!(prev.model, "deepseek-chat");

    // Verify it's now None in the store
    let stored = store.tool_activations.get("claude-code").unwrap();
    assert!(stored.is_none());
}
#[test]
fn test_deactivate_tool_no_previous_activation() {
    let mut store = FlatProvidersStore::default();

    // Deactivate a tool that was never activated
    let previous = deactivate_tool(&mut store, "claude-code").unwrap();
    assert!(previous.is_none());

    // The entry should now exist as None
    let stored = store.tool_activations.get("claude-code").unwrap();
    assert!(stored.is_none());
}
#[test]
fn test_deactivate_tool_already_none() {
    let mut store = FlatProvidersStore::default();
    store.tool_activations.insert("codex".to_string(), None);

    // Deactivate a tool that's already None
    let previous = deactivate_tool(&mut store, "codex").unwrap();
    assert!(previous.is_none());
}
#[test]
fn test_activate_deactivate_round_trip() {
    let mut store = FlatProvidersStore::default();
    let entry = make_flat_entry("Provider");
    let created = create_provider_flat(&mut store, entry).unwrap();

    // Activate
    let activation =
        activate_tool(&mut store, &created.id, "codex", Some("model-x"), None).unwrap();
    assert_eq!(activation.provider_id, created.id);
    assert_eq!(activation.model, "model-x");

    // Deactivate
    let previous = deactivate_tool(&mut store, "codex").unwrap();
    assert_eq!(previous.unwrap().provider_id, created.id);

    // Verify tool is deactivated
    let stored = store.tool_activations.get("codex").unwrap();
    assert!(stored.is_none());
}
#[test]
fn test_activate_multiple_tools_same_provider() {
    let mut store = FlatProvidersStore::default();
    let entry = make_flat_entry("DeepSeek");
    let created = create_provider_flat(&mut store, entry).unwrap();

    // Activate same provider for both tools
    activate_tool(
        &mut store,
        &created.id,
        "claude-code",
        Some("deepseek-chat"),
        None,
    )
    .unwrap();
    activate_tool(
        &mut store,
        &created.id,
        "codex",
        Some("deepseek-reasoner"),
        None,
    )
    .unwrap();

    // Both should be active
    let claude = store
        .tool_activations
        .get("claude-code")
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(claude.provider_id, created.id);
    assert_eq!(claude.model, "deepseek-chat");

    let codex = store
        .tool_activations
        .get("codex")
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(codex.provider_id, created.id);
    assert_eq!(codex.model, "deepseek-reasoner");
}

// NOTE: This test's verbatim source was lost when the original (uncommitted)
// `providers.rs` was deleted during the module split; it is the only one of the
// 103 provider tests not recoverable from editor history. Reconstructed to match
// its name/intent: every built-in flat preset id must resolve to a usable
// provider identity via `create_from_preset_flat`.
#[test]
fn every_preset_id_resolves_to_a_provider_identity() {
    for preset in get_all_presets_flat() {
        let entry = create_from_preset_flat(&preset.id, "sk-test-key-12345")
            .unwrap_or_else(|e| panic!("preset `{}` failed to resolve: {e}", preset.id));
        assert!(
            !entry.name.trim().is_empty(),
            "preset `{}` resolved to an empty provider name",
            preset.id
        );
        assert_eq!(
            entry.preset_id.as_deref(),
            Some(preset.id.as_str()),
            "preset `{}` did not carry its preset_id into the provider identity",
            preset.id
        );
    }
}

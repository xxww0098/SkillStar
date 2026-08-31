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
        .active()
        .unwrap();
    let codex_act = result
        .tool_activations
        .get("codex")
        .unwrap()
        .active()
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
    assert_eq!(result.version, FLAT_STORE_VERSION);
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
    assert!(claude_act.is_none() || claude_act.unwrap().is_empty());
}
// ---------------------------------------------------------------------------
// Binding an agent to a provider
// ---------------------------------------------------------------------------

#[test]
fn binding_claude_code_records_the_provider_and_model() {
    let mut store = ProvidersStoreV4::default();
    let created = create_provider(&mut store, make_provider("DeepSeek")).unwrap();

    let entry = bind_provider(
        &mut store,
        "claude-code",
        &created.id,
        Some("deepseek-chat"),
        None,
    )
    .unwrap();

    assert_eq!(entry.provider_id, created.id);
    assert_eq!(entry.model, "deepseek-chat");
    let stored = store.bindings["claude-code"].active().unwrap();
    assert_eq!(stored.provider_id, created.id);
    assert_eq!(stored.model, "deepseek-chat");
}

#[test]
fn binding_codex_requires_a_responses_endpoint() {
    let mut store = ProvidersStoreV4::default();
    let created = create_provider(&mut store, make_responses_provider("OpenAI")).unwrap();

    let entry = bind_provider(&mut store, "codex", &created.id, Some("gpt-5.4"), None).unwrap();

    assert_eq!(entry.provider_id, created.id);
    assert_eq!(entry.model, "gpt-5.4");
    assert_eq!(store.bindings["codex"].active().unwrap().model, "gpt-5.4");
}

#[test]
fn an_absent_or_blank_model_falls_back_to_the_providers_default() {
    for model in [None, Some(""), Some("   ")] {
        let mut store = ProvidersStoreV4::default();
        let mut provider = make_responses_provider("DeepSeek");
        provider.default_model = Some("deepseek-chat".to_string());
        let created = create_provider(&mut store, provider).unwrap();

        let entry = bind_provider(&mut store, "codex", &created.id, model, None).unwrap();

        assert_eq!(entry.model, "deepseek-chat", "for model = {model:?}");
    }
}

#[test]
fn binding_an_unknown_provider_is_an_error() {
    let mut store = ProvidersStoreV4::default();
    let err = bind_provider(&mut store, "claude-code", "ghost", None, None).unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
}

#[test]
fn claude_code_refuses_a_provider_with_no_anthropic_endpoint() {
    for endpoint in [None, Some("   ".to_string())] {
        let mut store = ProvidersStoreV4::default();
        let mut provider = make_provider("Relay");
        provider.endpoints.anthropic_messages = endpoint.clone();
        let created = create_provider(&mut store, provider).unwrap();

        let err =
            bind_provider(&mut store, "claude-code", &created.id, Some("m"), None).unwrap_err();

        assert!(
            err.to_string().contains("Anthropic"),
            "for endpoint {endpoint:?}: {err}"
        );
        assert!(!store.bindings.contains_key("claude-code"));
    }
}

#[test]
fn a_chat_speaking_agent_refuses_a_provider_with_no_chat_endpoint() {
    let mut store = ProvidersStoreV4::default();
    let mut provider = make_provider("Anthropic only");
    provider.endpoints.openai_chat = None;
    let created = create_provider(&mut store, provider).unwrap();

    let err = bind_provider(&mut store, "opencode", &created.id, Some("m"), None).unwrap_err();

    assert!(err.to_string().contains("chat/completions"), "{err}");
}

#[test]
fn an_unknown_agent_id_is_gated_on_chat() {
    // The safest assumption for an id the registry does not know: every
    // OpenAI-compatible host implements chat, so requiring it binds nothing
    // that could not work and refuses a row with no endpoint at all.
    let mut store = ProvidersStoreV4::default();
    let created = create_provider(&mut store, make_provider("Relay")).unwrap();

    assert!(
        bind_provider(
            &mut store,
            "some-future-agent",
            &created.id,
            Some("m"),
            None
        )
        .is_ok()
    );
}

#[test]
fn rebinding_a_single_provider_agent_replaces_its_entry() {
    let mut store = ProvidersStoreV4::default();
    let first = create_provider(&mut store, make_provider("First")).unwrap();
    let second = create_provider(&mut store, make_provider("Second")).unwrap();

    bind_provider(&mut store, "claude-code", &first.id, Some("model-a"), None).unwrap();
    bind_provider(&mut store, "claude-code", &second.id, Some("model-b"), None).unwrap();

    let binding = &store.bindings["claude-code"];
    assert_eq!(binding.entries.len(), 1);
    assert_eq!(binding.active().unwrap().provider_id, second.id);
}

#[test]
fn rebinding_a_multi_provider_agent_appends_and_points_at_the_new_row() {
    let mut store = ProvidersStoreV4::default();
    let first = create_provider(&mut store, make_provider("First")).unwrap();
    let second = create_provider(&mut store, make_provider("Second")).unwrap();

    bind_provider(&mut store, "omp", &first.id, Some("model-a"), None).unwrap();
    bind_provider(&mut store, "omp", &second.id, Some("model-b"), None).unwrap();

    let binding = &store.bindings["omp"];
    assert_eq!(binding.entries.len(), 2);
    assert_eq!(binding.active().unwrap().provider_id, second.id);
}

// ---------------------------------------------------------------------------
// Unbinding
// ---------------------------------------------------------------------------

#[test]
fn unbinding_an_agent_returns_what_was_active() {
    let mut store = ProvidersStoreV4::default();
    let created = create_provider(&mut store, make_provider("DeepSeek")).unwrap();
    bind_provider(
        &mut store,
        "claude-code",
        &created.id,
        Some("model-a"),
        None,
    )
    .unwrap();

    let previous = unbind_agent(&mut store, "claude-code").unwrap().unwrap();

    assert_eq!(previous.provider_id, created.id);
    assert!(store.bindings["claude-code"].is_empty());
}

#[test]
fn unbinding_an_agent_that_was_never_bound_returns_nothing() {
    let mut store = ProvidersStoreV4::default();
    assert!(unbind_agent(&mut store, "claude-code").unwrap().is_none());
    assert!(unbind_agent(&mut store, "claude-code").unwrap().is_none());
}

#[test]
fn bind_then_unbind_then_bind_again_round_trips() {
    let mut store = ProvidersStoreV4::default();
    let created = create_provider(&mut store, make_provider("DeepSeek")).unwrap();

    bind_provider(
        &mut store,
        "claude-code",
        &created.id,
        Some("model-a"),
        None,
    )
    .unwrap();
    unbind_agent(&mut store, "claude-code").unwrap();
    assert!(store.bindings["claude-code"].is_empty());

    bind_provider(
        &mut store,
        "claude-code",
        &created.id,
        Some("model-b"),
        None,
    )
    .unwrap();
    assert_eq!(
        store.bindings["claude-code"].active().unwrap().model,
        "model-b"
    );
}

#[test]
fn one_provider_can_serve_several_agents_with_different_models() {
    let mut store = ProvidersStoreV4::default();
    let created = create_provider(&mut store, make_responses_provider("DeepSeek")).unwrap();

    bind_provider(
        &mut store,
        "claude-code",
        &created.id,
        Some("model-a"),
        None,
    )
    .unwrap();
    bind_provider(&mut store, "codex", &created.id, Some("model-b"), None).unwrap();

    assert_eq!(
        store.bindings["claude-code"].active().unwrap().model,
        "model-a"
    );
    assert_eq!(store.bindings["codex"].active().unwrap().model, "model-b");
}

#[test]
fn binding_codex_official_forces_the_oauth_auth_mode() {
    let mut store = ProvidersStoreV4::default();
    assert!(ensure_official_providers(&mut store));

    let entry = bind_provider(&mut store, "codex", CODEX_OFFICIAL_ID, None, None).unwrap();

    assert_eq!(entry.provider_id, CODEX_OFFICIAL_ID);
    assert_eq!(
        entry
            .settings
            .expect("oauth settings")
            .get("auth_mode")
            .and_then(|v| v.as_str()),
        Some("oauth"),
        "writing OPENAI_API_KEY would clobber the user's ChatGPT session"
    );
}

#[test]
fn every_preset_id_resolves_to_a_provider_identity() {
    for preset in get_all_presets_flat() {
        let entry = create_provider_from_preset(&preset.id, "sk-test-key-12345")
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

#[test]
fn every_preset_id_maps_through_skillstar_providers() {
    for preset in get_all_presets_flat() {
        assert!(
            skillstar_providers::identity::identity_for_preset(&preset.id).is_some(),
            "preset `{}` missing from skillstar_providers::PROVIDER_IDENTITIES",
            preset.id
        );
    }
}

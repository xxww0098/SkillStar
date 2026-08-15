//! v4 provider-row CRUD, and the v1/v2/v3 → current migration chain.

use super::*;

// ---------------------------------------------------------------------------
// Building a row from a preset
// ---------------------------------------------------------------------------

#[test]
fn a_relay_preset_creates_a_row_with_no_models_and_no_anthropic_endpoint() {
    let provider = create_provider_from_preset("openrouter", "or-key").unwrap();
    assert_eq!(provider.name, "OpenRouter");
    assert!(provider.models.is_empty());
    assert_eq!(provider.default_model, None);
    assert_eq!(
        provider.endpoints.openai_chat.as_deref(),
        Some("https://openrouter.ai/api/v1")
    );
    assert_eq!(
        provider.endpoints.anthropic_messages, None,
        "an absent endpoint is None, not an empty string"
    );
}

#[test]
fn an_unknown_preset_id_is_an_error() {
    let result = create_provider_from_preset("nonexistent-preset", "key");
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn every_registered_preset_builds_a_valid_row() {
    for preset in get_all_presets_flat() {
        let provider = create_provider_from_preset(&preset.id, "test-api-key")
            .unwrap_or_else(|e| panic!("preset '{}' failed: {e}", preset.id));

        assert_eq!(provider.name, preset.name);
        assert_eq!(
            provider.endpoints.openai_chat.unwrap_or_default(),
            preset.base_url_openai
        );
        assert_eq!(
            provider.endpoints.anthropic_messages.unwrap_or_default(),
            preset.base_url_anthropic
        );
        assert!(provider.models.is_empty());
        assert_eq!(provider.default_model, None);
        assert_eq!(provider.icon_color, Some(preset.icon_color.clone()));
        assert_eq!(provider.preset_id, Some(preset.id.clone()));
        assert!(provider.created_at_ms.is_some());

        if preset.category.is_native_login() {
            assert_eq!(provider.id, preset.id, "native-login rows keep a stable id");
            assert!(
                matches!(provider.credential, Credential::ExternalCli { .. }),
                "the credential lives in the agent's own store, which is a state, not an absence"
            );
        } else {
            assert!(uuid::Uuid::parse_str(&provider.id).is_ok());
        }
    }
}

#[test]
fn only_openai_gets_a_responses_endpoint_without_being_probed() {
    // The rule the Codex fix rests on: nobody else is assumed to speak it.
    for preset in get_all_presets_flat() {
        let provider = create_provider_from_preset(&preset.id, "k").unwrap();
        let expected = preset.base_url_openai.contains("api.openai.com");
        assert_eq!(
            provider.endpoints.openai_responses.is_some(),
            expected,
            "preset '{}' got the wrong Responses assumption",
            preset.id
        );
        assert_eq!(
            provider.caps.responses_api,
            if expected { Tri::Yes } else { Tri::Unknown },
            "preset '{}' — an unprobed host is Unknown, never No",
            preset.id
        );
    }
}

#[test]
fn the_native_login_category_replaces_the_id_whitelist() {
    let native: Vec<String> = get_all_presets_flat()
        .into_iter()
        .filter(|p| p.category.is_native_login())
        .map(|p| p.id)
        .collect();
    assert_eq!(native, vec![CLAUDE_OFFICIAL_ID, CODEX_OFFICIAL_ID]);
    assert!(is_native_official_preset_id(CLAUDE_OFFICIAL_ID));
    assert!(
        !is_native_official_preset_id("grok"),
        "Grok is a vendor you reach with a key; it was only ever `official` by accident"
    );
}

// ---------------------------------------------------------------------------
// create_provider
// ---------------------------------------------------------------------------

#[test]
fn creating_a_provider_stamps_it_and_appends_it() {
    let mut store = ProvidersStoreV4::default();
    let created = create_provider(&mut store, make_provider("My Provider")).unwrap();

    assert_eq!(created.name, "My Provider");
    assert!(created.created_at_ms.is_some_and(|ms| ms > 0));
    assert_eq!(created.sort_index, 0);
    assert_eq!(store.providers.len(), 1);
    assert_eq!(store.providers[0].id, created.id);
}

#[test]
fn a_blank_name_is_rejected() {
    for name in ["", "   "] {
        let mut store = ProvidersStoreV4::default();
        let err = create_provider(&mut store, make_provider(name)).unwrap_err();
        assert!(err.to_string().contains("name must not be empty"), "{err}");
    }
}

#[test]
fn a_malformed_endpoint_is_rejected() {
    let mut store = ProvidersStoreV4::default();
    let mut provider = make_provider("Test");
    provider.endpoints.openai_chat = Some("not-a-url".to_string());
    let err = create_provider(&mut store, provider).unwrap_err();
    assert!(err.to_string().contains("Invalid URL"), "{err}");
}

#[test]
fn a_non_http_scheme_is_rejected() {
    let mut store = ProvidersStoreV4::default();
    let mut provider = make_provider("Test");
    provider.endpoints.openai_chat = Some("ftp://api.example.com/v1".to_string());
    let err = create_provider(&mut store, provider).unwrap_err();
    assert!(err.to_string().contains("http or https"), "{err}");
}

#[test]
fn an_absent_endpoint_is_allowed() {
    let mut store = ProvidersStoreV4::default();
    let mut provider = make_provider("Test");
    provider.endpoints.anthropic_messages = None;
    assert!(create_provider(&mut store, provider).is_ok());
}

#[test]
fn the_caller_keeps_the_id_it_chose() {
    // v3 overwrote it, which is why seeding a fixed-slug row needed a whitelist.
    let mut store = ProvidersStoreV4::default();
    let mut provider = make_provider("Official");
    provider.id = "claude-official".to_string();
    let created = create_provider(&mut store, provider).unwrap();
    assert_eq!(created.id, "claude-official");
}

#[test]
fn a_duplicate_id_is_refused_rather_than_reassigned() {
    let mut store = ProvidersStoreV4::default();
    let mut first = make_provider("A");
    first.id = "fixed".to_string();
    let mut second = make_provider("B");
    second.id = "fixed".to_string();

    create_provider(&mut store, first).unwrap();
    let err = create_provider(&mut store, second).unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
}

#[test]
fn an_existing_creation_timestamp_is_preserved() {
    let mut store = ProvidersStoreV4::default();
    let mut provider = make_provider("Test");
    provider.created_at_ms = Some(1719000000000);
    let created = create_provider(&mut store, provider).unwrap();
    assert_eq!(created.created_at_ms, Some(1719000000000));
}

#[test]
fn sort_index_increments_per_row() {
    let mut store = ProvidersStoreV4::default();
    for (n, name) in ["First", "Second", "Third"].iter().enumerate() {
        let created = create_provider(&mut store, make_provider(name)).unwrap();
        assert_eq!(created.sort_index, n as u32);
    }
}

// ---------------------------------------------------------------------------
// replace_provider / delete_provider / reorder_providers
// ---------------------------------------------------------------------------

#[test]
fn replacing_a_row_keeps_its_position() {
    let mut store = ProvidersStoreV4::default();
    create_provider(&mut store, make_provider("First")).unwrap();
    let second = create_provider(&mut store, make_provider("Second")).unwrap();
    create_provider(&mut store, make_provider("Third")).unwrap();

    let mut edited = second.clone();
    edited.name = "Second (renamed)".to_string();
    replace_provider(&mut store, edited).unwrap();

    assert_eq!(store.providers[1].id, second.id);
    assert_eq!(store.providers[1].name, "Second (renamed)");
}

#[test]
fn replacing_an_absent_row_is_an_error() {
    let mut store = ProvidersStoreV4::default();
    let err = replace_provider(&mut store, make_provider("Ghost")).unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
}

#[test]
fn deleting_a_provider_removes_it() {
    let mut store = ProvidersStoreV4::default();
    let created = create_provider(&mut store, make_provider("Doomed")).unwrap();
    delete_provider(&mut store, &created.id).unwrap();
    assert!(store.providers.is_empty());
}

#[test]
fn deleting_an_absent_provider_is_an_error() {
    let mut store = ProvidersStoreV4::default();
    let err = delete_provider(&mut store, "ghost").unwrap_err();
    assert!(err.to_string().contains("not found"), "{err}");
}

#[test]
fn deleting_a_provider_takes_its_bindings_and_its_roles_with_it() {
    let mut store = ProvidersStoreV4::default();
    let keep = create_provider(&mut store, make_provider("Keep")).unwrap();
    let doomed = create_provider(&mut store, make_provider("Doomed")).unwrap();

    bind_provider(&mut store, "omp", &keep.id, Some("model-a"), None).unwrap();
    bind_provider(&mut store, "omp", &doomed.id, Some("model-a"), None).unwrap();
    set_agent_roles(
        &mut store,
        "omp",
        [
            ("default".to_string(), ModelRef::new(&keep.id, "model-a")),
            ("fast".to_string(), ModelRef::new(&doomed.id, "model-a")),
        ]
        .into_iter()
        .collect(),
    )
    .unwrap();

    delete_provider(&mut store, &doomed.id).unwrap();

    let binding = &store.bindings["omp"];
    assert_eq!(binding.entries.len(), 1);
    assert_eq!(binding.entries[0].provider_id, keep.id);
    assert_eq!(
        binding.roles.keys().collect::<Vec<_>>(),
        vec!["default"],
        "a role pointing at a deleted provider writes a dangling key into the agent config"
    );
    assert_eq!(binding.active_index, 0);
}

#[test]
fn deleting_a_provider_leaves_other_agents_alone() {
    let mut store = ProvidersStoreV4::default();
    let keep = create_provider(&mut store, make_provider("Keep")).unwrap();
    let doomed = create_provider(&mut store, make_provider("Doomed")).unwrap();

    bind_provider(&mut store, "claude-code", &keep.id, Some("model-a"), None).unwrap();
    bind_provider(&mut store, "omp", &doomed.id, Some("model-a"), None).unwrap();

    delete_provider(&mut store, &doomed.id).unwrap();

    assert_eq!(store.bindings["claude-code"].entries.len(), 1);
    assert!(store.bindings["omp"].is_empty());
}

#[test]
fn reordering_assigns_sort_index_by_position() {
    let mut store = ProvidersStoreV4::default();
    let a = create_provider(&mut store, make_provider("A")).unwrap();
    let b = create_provider(&mut store, make_provider("B")).unwrap();
    let c = create_provider(&mut store, make_provider("C")).unwrap();

    reorder_providers(&mut store, &[c.id.clone(), a.id.clone(), b.id.clone()]).unwrap();

    let index_of = |id: &str| store.provider(id).unwrap().sort_index;
    assert_eq!(index_of(&c.id), 0);
    assert_eq!(index_of(&a.id), 1);
    assert_eq!(index_of(&b.id), 2);
}

#[test]
fn reordering_leaves_unnamed_rows_where_they_were() {
    let mut store = ProvidersStoreV4::default();
    let a = create_provider(&mut store, make_provider("A")).unwrap();
    let b = create_provider(&mut store, make_provider("B")).unwrap();
    let before = store.provider(&b.id).unwrap().sort_index;

    reorder_providers(&mut store, &[a.id.clone()]).unwrap();

    assert_eq!(store.provider(&a.id).unwrap().sort_index, 0);
    assert_eq!(store.provider(&b.id).unwrap().sort_index, before);
}

#[test]
fn reordering_rejects_an_unknown_id_without_touching_anything() {
    let mut store = ProvidersStoreV4::default();
    let a = create_provider(&mut store, make_provider("A")).unwrap();
    let before = store.provider(&a.id).unwrap().sort_index;

    let err = reorder_providers(&mut store, &[a.id.clone(), "ghost".to_string()]).unwrap_err();

    assert!(err.to_string().contains("not found"), "{err}");
    assert_eq!(
        store.provider(&a.id).unwrap().sort_index,
        before,
        "validation runs before any write, so a bad list is a no-op"
    );
}

#[test]
fn reordering_an_empty_list_is_a_no_op() {
    let mut store = ProvidersStoreV4::default();
    create_provider(&mut store, make_provider("A")).unwrap();
    assert!(reorder_providers(&mut store, &[]).is_ok());
}

// ---------------------------------------------------------------------------
// Migration chain (v1 / v2 / v3 → the current flat format)
// ---------------------------------------------------------------------------

#[test]
fn test_migrate_store_if_needed_file_not_found() {
    let (_tmp, path) = setup_temp_store();
    let store = migrate_store_if_needed(&path).unwrap();
    assert_eq!(store.version, 3);
    assert!(store.providers.is_empty());
    assert!(store.tool_activations.is_empty());
}
#[test]
fn test_migrate_store_if_needed_already_v3() {
    let (_tmp, path) = setup_temp_store();
    let original = FlatProvidersStore {
        version: 3,
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
                ToolBinding::single(ToolActivation {
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
    assert_eq!(result.version, 3);
    assert_eq!(result.providers.len(), 1);
    assert_eq!(result.providers[0].id, "existing-id");
    assert_eq!(result.providers[0].name, "Existing Provider");
    assert_eq!(
        result
            .tool_activations
            .get("claude-code")
            .unwrap()
            .active()
            .unwrap()
            .provider_id,
        "existing-id"
    );
}
#[test]
fn test_migrate_store_if_needed_v2_to_v3() {
    let (_tmp, path) = setup_temp_store();
    // Write a raw v2 store (single Option<ToolActivation> per tool, null for none).
    let v2_json = serde_json::json!({
        "version": 2,
        "providers": [{
            "id": "p-v2",
            "name": "V2 Provider",
            "base_url_openai": "https://api.example.com/v1",
            "base_url_anthropic": "",
            "models_url": "",
            "api_key": "sk-key",
            "models": ["model-a"],
            "default_model": "model-a",
            "sort_index": 0,
            "codex_wire_api": "responses",
            "codex_auth_mode": "api_key"
        }],
        "tool_activations": {
            "claude-code": { "provider_id": "p-v2", "model": "model-a" },
            "codex": null
        }
    });
    std::fs::write(&path, serde_json::to_string_pretty(&v2_json).unwrap()).unwrap();

    let result = migrate_store_if_needed(&path).unwrap();
    assert_eq!(result.version, 3);
    assert_eq!(result.providers.len(), 1);
    // Non-null activation → single-entry binding.
    let claude = result.tool_activations.get("claude-code").unwrap();
    assert_eq!(claude.entries.len(), 1);
    assert_eq!(claude.active().unwrap().provider_id, "p-v2");
    // Null activation → empty binding.
    assert!(result.tool_activations.get("codex").unwrap().is_empty());
    // A .bak of the original v2 file should exist.
    assert!(path.with_extension("json.bak").exists());
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
    assert_eq!(result.version, 3);
    assert_eq!(result.providers.len(), 1);
    assert_eq!(result.providers[0].name, "DeepSeek");
    assert_eq!(
        result.providers[0].base_url_openai,
        "https://api.example.com/v1"
    );
    assert_eq!(result.providers[0].api_key, "sk-test-key-12345");
    assert_eq!(result.providers[0].models, vec!["model-a"]);

    // tool_activations should map claude → claude-code
    let claude_activation = result.tool_activations.get("claude-code");
    assert!(claude_activation.is_some());
    let activation = claude_activation.unwrap().active().unwrap();
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
    store
        .claude
        .providers
        .insert("p1".to_string(), entry_claude);
    store.claude.current = Some("p1".to_string());
    store.codex.providers.insert("p2".to_string(), entry_codex);
    store.codex.current = Some("p2".to_string());
    write_store_to(&store, &path).unwrap();

    let result = migrate_store_if_needed(&path).unwrap();
    // Should be deduplicated to 1 provider (same base_url + api_key)
    assert_eq!(result.providers.len(), 1);

    // Both tool_activations should point to the same provider
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
    assert_eq!(claude_act.provider_id, codex_act.provider_id);
}


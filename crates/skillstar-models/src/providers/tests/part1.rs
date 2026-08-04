use super::*;
use tempfile::TempDir;

#[test]
fn test_read_flat_store_missing_file() {
    let (_tmp, path) = setup_temp_store();
    let store = read_flat_store(&path).unwrap();
    assert_eq!(store.version, FLAT_STORE_VERSION);
    assert!(store.providers.is_empty());
    assert!(store.tool_activations.is_empty());
}

#[test]
fn test_model_catalog_merges_provider_ids_with_registry_metadata() {
    let provider_body = serde_json::json!({
        "data": [
            { "id": "gpt-4o" },
            { "id": "deepseek-chat" }
        ]
    });
    let registry_body = serde_json::json!({
        "openai": {
            "models": {
                "gpt-4o": {
                    "id": "gpt-4o",
                    "name": "GPT-4o",
                    "limit": { "context": 128000, "output": 16384 },
                    "cost": { "input": 2.5, "output": 10.0 }
                }
            }
        },
        "deepseek": [
            {
                "id": "deepseek-chat",
                "display_name": "DeepSeek Chat",
                "context_length": 64000,
                "max_completion_tokens": 8192
            }
        ]
    });

    let provider_catalog = catalog_from_provider_models(&provider_body);
    let registry_catalog = catalog_from_registry(&registry_body);
    let result = merge_model_catalog(provider_catalog, &[registry_catalog]);

    assert_eq!(result.models, vec!["gpt-4o", "deepseek-chat"]);
    let gpt_4o = result
        .catalog
        .iter()
        .find(|entry| entry.id == "gpt-4o")
        .unwrap();
    assert_eq!(gpt_4o.display_name.as_deref(), Some("GPT-4o"));
    assert_eq!(gpt_4o.context_length, Some(128000));
    assert_eq!(gpt_4o.max_completion_tokens, Some(16384));
    assert_eq!(
        gpt_4o
            .cost
            .as_ref()
            .and_then(|cost| cost.get("output"))
            .and_then(Value::as_f64),
        Some(10.0)
    );
}

#[test]
fn test_read_flat_store_malformed_json() {
    let (_tmp, path) = setup_temp_store();
    std::fs::write(&path, "not valid json {{{").unwrap();
    let store = read_flat_store(&path).unwrap();
    assert_eq!(store.version, FLAT_STORE_VERSION);
    assert!(store.providers.is_empty());
    assert!(store.tool_activations.is_empty());
}
#[test]
fn test_read_flat_store_with_bom() {
    let (_tmp, path) = setup_temp_store();
    let store = FlatProvidersStore {
        version: FLAT_STORE_VERSION,
        providers: vec![ProviderEntryFlat {
            id: "test-id".to_string(),
            name: "Test".to_string(),
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
            created_at: None,
            meta: None,
            codex_wire_api: "responses".to_string(),
            codex_auth_mode: "api_key".to_string(),
        }],
        tool_activations: HashMap::new(),
    };
    let json = serde_json::to_string_pretty(&store).unwrap();
    let content = format!("\u{FEFF}{}", json);
    std::fs::write(&path, content).unwrap();

    let loaded = read_flat_store(&path).unwrap();
    assert_eq!(loaded.providers.len(), 1);
    assert_eq!(loaded.providers[0].id, "test-id");
}
#[test]
fn test_write_and_read_flat_store() {
    let (_tmp, path) = setup_temp_store();
    let store = FlatProvidersStore {
        version: FLAT_STORE_VERSION,
        providers: vec![ProviderEntryFlat {
            id: "p1".to_string(),
            name: "Provider 1".to_string(),
            base_url_openai: "https://api.deepseek.com/v1".to_string(),
            base_url_anthropic: "https://api.deepseek.com/anthropic".to_string(),
            models_url: "https://api.deepseek.com/v1/models".to_string(),
            api_key: "sk-test".to_string(),
            models: vec!["deepseek-chat".to_string()],
            default_model: "deepseek-chat".to_string(),
            sort_index: 0,
            preset_id: Some("deepseek".to_string()),
            icon_color: Some("#4D6BFE".to_string()),
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
                    provider_id: "p1".to_string(),
                    model: "deepseek-chat".to_string(),
                    settings: None,
                    last_sync_at: None,
                }),
            );
            map.insert("codex".to_string(), ToolBinding::default());
            map
        },
    };

    write_flat_store(&store, &path).unwrap();
    let loaded = read_flat_store(&path).unwrap();

    assert_eq!(loaded.version, FLAT_STORE_VERSION);
    assert_eq!(loaded.providers.len(), 1);
    assert_eq!(loaded.providers[0].id, "p1");
    assert_eq!(loaded.providers[0].name, "Provider 1");
    assert_eq!(
        loaded.providers[0].base_url_openai,
        "https://api.deepseek.com/v1"
    );
    assert_eq!(
        loaded.providers[0].base_url_anthropic,
        "https://api.deepseek.com/anthropic"
    );
    assert_eq!(loaded.providers[0].api_key, "sk-test");
    assert_eq!(loaded.providers[0].models, vec!["deepseek-chat"]);
    assert_eq!(loaded.providers[0].default_model, "deepseek-chat");
    assert_eq!(loaded.providers[0].sort_index, 0);
    assert_eq!(loaded.providers[0].preset_id, Some("deepseek".to_string()));
    assert_eq!(loaded.providers[0].icon_color, Some("#4D6BFE".to_string()));
    assert_eq!(loaded.providers[0].created_at, Some(1719000000000));

    // Check tool_activations
    let claude_activation = loaded.tool_activations.get("claude-code").unwrap();
    assert!(!claude_activation.is_empty());
    let activation = claude_activation.active().unwrap();
    assert_eq!(activation.provider_id, "p1");
    assert_eq!(activation.model, "deepseek-chat");

    let codex_activation = loaded.tool_activations.get("codex").unwrap();
    assert!(codex_activation.is_empty());
}
#[test]
fn test_write_flat_store_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("nested").join("dir").join("store.json");
    let store = FlatProvidersStore::default();
    write_flat_store(&store, &path).unwrap();
    assert!(path.exists());
}
#[test]
fn test_write_flat_store_atomic_no_temp_file_left() {
    let (_tmp, path) = setup_temp_store();
    let store = FlatProvidersStore::default();
    write_flat_store(&store, &path).unwrap();

    // The temp file should not exist after a successful write
    let temp_path = path.with_extension("json.tmp");
    assert!(!temp_path.exists());
}
#[test]
fn test_read_flat_store_empty_file() {
    let (_tmp, path) = setup_temp_store();
    std::fs::write(&path, "").unwrap();
    let store = read_flat_store(&path).unwrap();
    assert_eq!(store.version, FLAT_STORE_VERSION);
    assert!(store.providers.is_empty());
}
#[test]
fn test_read_store_missing_file() {
    let (_tmp, path) = setup_temp_store();
    let store = read_store_from(&path).unwrap();
    assert!(store.claude.providers.is_empty());
    assert!(store.codex.providers.is_empty());
}
#[test]
fn test_read_store_malformed_json() {
    let (_tmp, path) = setup_temp_store();
    std::fs::write(&path, "not valid json {{{").unwrap();
    let store = read_store_from(&path).unwrap();
    assert!(store.claude.providers.is_empty());
    assert!(store.codex.providers.is_empty());
}
#[test]
fn test_read_store_with_bom() {
    let (_tmp, path) = setup_temp_store();
    let json = r#"{"claude":{"providers":{},"current":null},"codex":{"providers":{},"current":null},"opencode":{"providers":{},"current":null},"gemini":{"providers":{},"current":null}}"#;
    let content = format!("\u{FEFF}{}", json);
    std::fs::write(&path, content).unwrap();
    let store = read_store_from(&path).unwrap();
    assert!(store.claude.providers.is_empty());
}
#[test]
fn test_write_and_read_store() {
    let (_tmp, path) = setup_temp_store();
    let mut store = ProvidersStore::default();
    store.claude.current = Some("test-id".to_string());
    write_store_to(&store, &path).unwrap();

    let loaded = read_store_from(&path).unwrap();
    assert_eq!(loaded.claude.current, Some("test-id".to_string()));
}
#[test]
fn test_atomic_write_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("nested").join("dir").join("store.json");
    let store = ProvidersStore::default();
    write_store_to(&store, &path).unwrap();
    assert!(path.exists());
}
#[test]
fn test_get_all_presets_flat_count() {
    let presets = get_all_presets_flat();
    assert_eq!(presets.len(), 13);
}
#[test]
fn test_get_all_presets_flat_unique_ids() {
    let presets = get_all_presets_flat();
    let ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
    let mut unique_ids = ids.clone();
    unique_ids.sort();
    unique_ids.dedup();
    assert_eq!(ids.len(), unique_ids.len(), "All preset IDs must be unique");
}
#[test]
fn test_get_all_presets_flat_categories() {
    let presets = get_all_presets_flat();
    let domestic: Vec<_> = presets
        .iter()
        .filter(|p| p.category == "domestic")
        .collect();
    let relay: Vec<_> = presets.iter().filter(|p| p.category == "relay").collect();
    assert_eq!(domestic.len(), 8);
    assert_eq!(relay.len(), 2);
}
#[test]
fn test_get_all_presets_flat_deepseek() {
    let presets = get_all_presets_flat();
    let ds = presets.iter().find(|p| p.id == "deepseek").unwrap();
    assert_eq!(ds.name, "DeepSeek");
    assert_eq!(ds.base_url_openai, "https://api.deepseek.com/v1");
    assert_eq!(ds.base_url_anthropic, "https://api.deepseek.com/anthropic");
    assert!(ds.models.is_empty());
    assert_eq!(ds.icon_color, "#4D6BFE");
    assert!(ds.balance_endpoint.is_some());
    assert!(ds.balance_parser.is_some());
}
#[test]
fn test_get_all_presets_flat_kimi_coding() {
    let presets = get_all_presets_flat();
    let kc = presets.iter().find(|p| p.id == "kimi-coding").unwrap();
    assert_eq!(kc.name, "Kimi For Coding");
    assert_eq!(kc.base_url_openai, "https://api.kimi.com/coding/v1");
    assert_eq!(kc.base_url_anthropic, "https://api.kimi.com/coding/");
    assert!(kc.models.is_empty());
}
#[test]
fn test_get_all_presets_flat_openrouter() {
    let presets = get_all_presets_flat();
    let or = presets.iter().find(|p| p.id == "openrouter").unwrap();
    assert_eq!(or.name, "OpenRouter");
    assert_eq!(or.category, "relay");
    assert_eq!(or.base_url_openai, "https://openrouter.ai/api/v1");
    assert!(or.base_url_anthropic.is_empty());
    assert!(or.models.is_empty());
    assert!(or.balance_endpoint.is_some());
}
#[test]
fn test_get_all_presets_flat_siliconflow() {
    let presets = get_all_presets_flat();
    let sf = presets.iter().find(|p| p.id == "siliconflow").unwrap();
    assert_eq!(sf.name, "SiliconFlow");
    assert_eq!(sf.category, "relay");
    assert_eq!(sf.base_url_openai, "https://api.siliconflow.cn/v1");
    assert!(sf.base_url_anthropic.is_empty());
    assert!(sf.models.is_empty());
}
#[test]
fn test_create_from_preset_flat_deepseek() {
    let result = create_from_preset_flat("deepseek", "sk-test-key-123").unwrap();
    assert_eq!(result.name, "DeepSeek");
    assert_eq!(result.base_url_openai, "https://api.deepseek.com/v1");
    assert_eq!(
        result.base_url_anthropic,
        "https://api.deepseek.com/anthropic"
    );
    assert_eq!(result.api_key, "sk-test-key-123");
    assert!(result.models.is_empty());
    assert_eq!(result.default_model, "");
    assert_eq!(result.preset_id, Some("deepseek".to_string()));
    assert_eq!(result.icon_color, Some("#4D6BFE".to_string()));
    assert!(result.created_at.is_some());
    // ID should be a valid UUID
    assert!(uuid::Uuid::parse_str(&result.id).is_ok());
}

#[test]
fn test_native_official_presets_empty_endpoints_no_key_url() {
    let presets = get_all_presets_flat();
    for id in [CLAUDE_OFFICIAL_ID, CODEX_OFFICIAL_ID] {
        let p = presets.iter().find(|p| p.id == id).unwrap();
        assert_eq!(p.category, "official");
        assert!(p.base_url_openai.is_empty());
        assert!(p.base_url_anthropic.is_empty());
        assert!(p.models_url.is_empty());
        assert!(p.api_key_url.is_none());
        assert!(p.balance_endpoint.is_none());
    }
}

#[test]
fn test_create_from_preset_flat_official_stable_ids() {
    let claude = create_from_preset_flat(CLAUDE_OFFICIAL_ID, "").unwrap();
    assert_eq!(claude.id, CLAUDE_OFFICIAL_ID);
    assert_eq!(claude.preset_id.as_deref(), Some(CLAUDE_OFFICIAL_ID));
    assert!(claude.api_key.is_empty());
    assert!(claude.base_url_anthropic.is_empty());

    let codex = create_from_preset_flat(CODEX_OFFICIAL_ID, "").unwrap();
    assert_eq!(codex.id, CODEX_OFFICIAL_ID);
    assert_eq!(codex.preset_id.as_deref(), Some(CODEX_OFFICIAL_ID));
    assert_eq!(codex.codex_auth_mode, "oauth");
    assert!(codex.base_url_openai.is_empty());
}

#[test]
fn test_ensure_official_providers_idempotent() {
    let mut store = FlatProvidersStore::default();
    assert!(ensure_official_providers(&mut store));
    assert_eq!(store.providers.len(), 2);
    assert!(store.providers.iter().any(|p| p.id == CLAUDE_OFFICIAL_ID));
    assert!(store.providers.iter().any(|p| p.id == CODEX_OFFICIAL_ID));

    // Second call is a no-op (does not duplicate or overwrite).
    assert!(!ensure_official_providers(&mut store));
    assert_eq!(store.providers.len(), 2);

    // Renamed Official row still counts as present.
    store
        .providers
        .iter_mut()
        .find(|p| p.id == CLAUDE_OFFICIAL_ID)
        .unwrap()
        .name = "My Claude Login".to_string();
    assert!(!ensure_official_providers(&mut store));
    assert_eq!(
        store
            .providers
            .iter()
            .find(|p| p.id == CLAUDE_OFFICIAL_ID)
            .unwrap()
            .name,
        "My Claude Login"
    );
}

#[test]
fn test_activate_claude_official_skips_url_gate() {
    let mut store = FlatProvidersStore::default();
    assert!(ensure_official_providers(&mut store));
    let activation = activate_tool(
        &mut store,
        CLAUDE_OFFICIAL_ID,
        "claude-code",
        None,
        None,
    )
    .unwrap();
    assert_eq!(activation.provider_id, CLAUDE_OFFICIAL_ID);
}

#[test]
fn test_activate_codex_official_forces_oauth_settings() {
    let mut store = FlatProvidersStore::default();
    assert!(ensure_official_providers(&mut store));
    let activation = activate_tool(&mut store, CODEX_OFFICIAL_ID, "codex", None, None).unwrap();
    assert_eq!(activation.provider_id, CODEX_OFFICIAL_ID);
    let settings = activation.settings.expect("oauth settings");
    assert_eq!(
        settings.get("auth_mode").and_then(|v| v.as_str()),
        Some("oauth")
    );
}

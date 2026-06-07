use super::*;
use tempfile::TempDir;

    #[test]
    fn test_read_flat_store_missing_file() {
        let (_tmp, path) = setup_temp_store();
        let store = read_flat_store(&path).unwrap();
        assert_eq!(store.version, 2);
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
        let gpt_4o = result.catalog.iter().find(|entry| entry.id == "gpt-4o").unwrap();
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
        assert_eq!(store.version, 2);
        assert!(store.providers.is_empty());
        assert!(store.tool_activations.is_empty());
    }
    #[test]
    fn test_read_flat_store_with_bom() {
        let (_tmp, path) = setup_temp_store();
        let store = FlatProvidersStore {
            version: 2,
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
            version: 2,
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
                    Some(ToolActivation {
                        provider_id: "p1".to_string(),
                        model: "deepseek-chat".to_string(),
                        settings: None,
                        last_sync_at: None,
                    }),
                );
                map.insert("codex".to_string(), None);
                map
            },
        };

        write_flat_store(&store, &path).unwrap();
        let loaded = read_flat_store(&path).unwrap();

        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].id, "p1");
        assert_eq!(loaded.providers[0].name, "Provider 1");
        assert_eq!(loaded.providers[0].base_url_openai, "https://api.deepseek.com/v1");
        assert_eq!(loaded.providers[0].base_url_anthropic, "https://api.deepseek.com/anthropic");
        assert_eq!(loaded.providers[0].api_key, "sk-test");
        assert_eq!(loaded.providers[0].models, vec!["deepseek-chat"]);
        assert_eq!(loaded.providers[0].default_model, "deepseek-chat");
        assert_eq!(loaded.providers[0].sort_index, 0);
        assert_eq!(loaded.providers[0].preset_id, Some("deepseek".to_string()));
        assert_eq!(loaded.providers[0].icon_color, Some("#4D6BFE".to_string()));
        assert_eq!(loaded.providers[0].created_at, Some(1719000000000));

        // Check tool_activations
        let claude_activation = loaded.tool_activations.get("claude-code").unwrap();
        assert!(claude_activation.is_some());
        let activation = claude_activation.as_ref().unwrap();
        assert_eq!(activation.provider_id, "p1");
        assert_eq!(activation.model, "deepseek-chat");

        let codex_activation = loaded.tool_activations.get("codex").unwrap();
        assert!(codex_activation.is_none());
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
        assert_eq!(store.version, 2);
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
    fn test_create_provider_valid() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "My Provider");
        let result = create_provider_at("claude", entry, &path).unwrap();
        assert_eq!(result.id, "p1");
        assert!(result.created_at.is_some());
        assert_eq!(result.sort_index, Some(0));

        // Should be auto-activated (first provider)
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("p1".to_string()));
    }
    #[test]
    fn test_create_provider_empty_name() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "");
        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name must not be empty"));
    }
    #[test]
    fn test_create_provider_name_too_long() {
        let (_tmp, path) = setup_temp_store();
        let long_name = "a".repeat(65);
        let entry = make_valid_entry("p1", &long_name);
        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at most 64 characters"));
    }
    #[test]
    fn test_create_provider_name_exactly_64_chars() {
        let (_tmp, path) = setup_temp_store();
        let name = "a".repeat(64);
        let entry = make_valid_entry("p1", &name);
        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_ok());
    }
    #[test]
    fn test_create_provider_invalid_url() {
        let (_tmp, path) = setup_temp_store();
        let mut entry = make_valid_entry("p1", "Test");
        let mut settings: ProviderSettings =
            serde_json::from_value(entry.settings_config.clone()).unwrap();
        settings.base_url = "not-a-url".to_string();
        entry.settings_config = serde_json::to_value(&settings).unwrap();

        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid base_url"));
    }
    #[test]
    fn test_create_provider_no_models() {
        let (_tmp, path) = setup_temp_store();
        let mut entry = make_valid_entry("p1", "Test");
        let mut settings: ProviderSettings =
            serde_json::from_value(entry.settings_config.clone()).unwrap();
        settings.models = vec![];
        entry.settings_config = serde_json::to_value(&settings).unwrap();

        let result = create_provider_at("claude", entry, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("At least one model"));
    }
    #[test]
    fn test_create_provider_duplicate_id() {
        let (_tmp, path) = setup_temp_store();
        let entry1 = make_valid_entry("p1", "Provider 1");
        create_provider_at("claude", entry1, &path).unwrap();

        let entry2 = make_valid_entry("p1", "Provider 2");
        let result = create_provider_at("claude", entry2, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }
    #[test]
    fn test_first_provider_auto_activation() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("first", "First Provider");
        create_provider_at("codex", entry, &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert_eq!(store.codex.current, Some("first".to_string()));

        // Second provider should NOT change current
        let entry2 = make_valid_entry("second", "Second Provider");
        create_provider_at("codex", entry2, &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert_eq!(store.codex.current, Some("first".to_string()));
    }
    #[test]
    fn test_update_provider() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Original");
        create_provider_at("claude", entry, &path).unwrap();

        let patch = ProviderPatch {
            name: Some("Updated Name".to_string()),
            ..Default::default()
        };
        let updated = update_provider_at("claude", "p1", patch, &path).unwrap();
        assert_eq!(updated.name, "Updated Name");

        // Verify persistence
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.providers["p1"].name, "Updated Name");
    }
    #[test]
    fn test_update_provider_settings() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Test");
        create_provider_at("claude", entry, &path).unwrap();

        let new_settings = ProviderSettings {
            base_url: "https://new-api.example.com/v1".to_string(),
            api_key: "new-key".to_string(),
            models: vec![ModelMapping {
                source_model: "new-model".to_string(),
                target_model: "new-model".to_string(),
                enabled: true,
            }],
            timeout_ms: Some(5000),
            max_retries: None,
        };
        let patch = ProviderPatch {
            settings_config: Some(serde_json::to_value(&new_settings).unwrap()),
            ..Default::default()
        };
        let updated = update_provider_at("claude", "p1", patch, &path).unwrap();
        let loaded_settings: ProviderSettings =
            serde_json::from_value(updated.settings_config).unwrap();
        assert_eq!(loaded_settings.base_url, "https://new-api.example.com/v1");
        assert_eq!(loaded_settings.api_key, "new-key");
    }
    #[test]
    fn test_update_provider_not_found() {
        let (_tmp, path) = setup_temp_store();
        let patch = ProviderPatch {
            name: Some("New".to_string()),
            ..Default::default()
        };
        let result = update_provider_at("claude", "nonexistent", patch, &path);
        assert!(result.is_err());
    }
    #[test]
    fn test_update_provider_invalid_name() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "Original");
        create_provider_at("claude", entry, &path).unwrap();

        let patch = ProviderPatch {
            name: Some("".to_string()),
            ..Default::default()
        };
        let result = update_provider_at("claude", "p1", patch, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name must not be empty"));
    }
    #[test]
    fn test_delete_provider() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("p1", "To Delete");
        create_provider_at("claude", entry, &path).unwrap();

        delete_provider_at("claude", "p1", &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert!(!store.claude.providers.contains_key("p1"));
    }
    #[test]
    fn test_delete_active_provider_nullifies_current() {
        let (_tmp, path) = setup_temp_store();
        let entry = make_valid_entry("active", "Active Provider");
        create_provider_at("claude", entry, &path).unwrap();

        // Verify it's active
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("active".to_string()));

        // Delete it
        delete_provider_at("claude", "active", &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, None);
    }
    #[test]
    fn test_delete_non_active_provider_keeps_current() {
        let (_tmp, path) = setup_temp_store();
        let entry1 = make_valid_entry("p1", "Provider 1");
        let entry2 = make_valid_entry("p2", "Provider 2");
        create_provider_at("claude", entry1, &path).unwrap();
        create_provider_at("claude", entry2, &path).unwrap();

        // p1 is current (first created)
        delete_provider_at("claude", "p2", &path).unwrap();

        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("p1".to_string()));
    }
    #[test]
    fn test_delete_provider_not_found() {
        let (_tmp, path) = setup_temp_store();
        let result = delete_provider_at("claude", "nonexistent", &path);
        assert!(result.is_err());
    }
    #[test]
    fn test_switch_active_provider() {
        let (_tmp, path) = setup_temp_store();
        let entry1 = make_valid_entry("p1", "Provider 1");
        let entry2 = make_valid_entry("p2", "Provider 2");
        create_provider_at("claude", entry1, &path).unwrap();
        create_provider_at("claude", entry2, &path).unwrap();

        // p1 is auto-activated as first
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("p1".to_string()));

        // Switch to p2
        switch_active_provider_at("claude", "p2", &path).unwrap();
        let store = read_store_from(&path).unwrap();
        assert_eq!(store.claude.current, Some("p2".to_string()));
    }
    #[test]
    fn test_switch_active_provider_not_found() {
        let (_tmp, path) = setup_temp_store();
        let result = switch_active_provider_at("claude", "nonexistent", &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
    #[test]
    fn test_get_provider_presets_count() {
        let presets = get_provider_presets();
        assert_eq!(presets.len(), 5);
    }
    #[test]
    fn test_get_provider_presets_official_anthropic() {
        let presets = get_provider_presets();
        let anthropic = presets.iter().find(|p| p.name == "Official (Anthropic)").unwrap();
        assert_eq!(anthropic.id, "official");
        assert_eq!(anthropic.base_url, "https://api.anthropic.com");
        assert_eq!(anthropic.icon_color, "#D97757");
        assert_eq!(anthropic.models.len(), 2);
    }
    #[test]
    fn test_get_provider_presets_official_openai() {
        let presets = get_provider_presets();
        let openai = presets.iter().find(|p| p.name == "Official (OpenAI)").unwrap();
        assert_eq!(openai.id, "official");
        assert_eq!(openai.base_url, "https://api.openai.com/v1");
        assert_eq!(openai.icon_color, "#10A37F");
        assert_eq!(openai.models.len(), 3);
    }
    #[test]
    fn test_create_from_preset_claude_official() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("claude", "official", "sk-test-key", &path).unwrap();
        assert_eq!(result.name, "Official (Anthropic)");
        assert_eq!(result.preset_id, Some("official".to_string()));
        assert_eq!(result.icon_color, Some("#D97757".to_string()));
        assert_eq!(
            result.api_key_url,
            Some("https://console.anthropic.com/settings/keys".to_string())
        );

        let settings: ProviderSettings =
            serde_json::from_value(result.settings_config).unwrap();
        assert_eq!(settings.base_url, "https://api.anthropic.com");
        assert_eq!(settings.api_key, "sk-test-key");
        assert_eq!(settings.models.len(), 2);
    }
    #[test]
    fn test_create_from_preset_codex_official() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("codex", "official", "sk-openai-key", &path).unwrap();
        assert_eq!(result.name, "Official (OpenAI)");
        assert_eq!(result.preset_id, Some("official".to_string()));
        assert_eq!(result.icon_color, Some("#10A37F".to_string()));

        let settings: ProviderSettings =
            serde_json::from_value(result.settings_config).unwrap();
        assert_eq!(settings.base_url, "https://api.openai.com/v1");
        assert_eq!(settings.api_key, "sk-openai-key");
        assert_eq!(settings.models.len(), 3);
    }
    #[test]
    fn test_create_from_preset_deepseek() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("claude", "deepseek", "ds-key", &path).unwrap();
        assert_eq!(result.name, "DeepSeek");
        assert_eq!(result.preset_id, Some("deepseek".to_string()));
        assert_eq!(result.icon_color, Some("#4D6BFE".to_string()));

        let settings: ProviderSettings =
            serde_json::from_value(result.settings_config).unwrap();
        assert_eq!(settings.base_url, "https://api.deepseek.com/v1");
        assert_eq!(settings.models.len(), 2);
    }
    #[test]
    fn test_create_from_preset_kimi() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("codex", "kimi", "kimi-key", &path).unwrap();
        assert_eq!(result.name, "Kimi");
        assert_eq!(result.preset_id, Some("kimi".to_string()));
        assert_eq!(result.icon_color, Some("#5B45E0".to_string()));
    }
    #[test]
    fn test_create_from_preset_glm() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("claude", "glm", "glm-key", &path).unwrap();
        assert_eq!(result.name, "GLM");
        assert_eq!(result.preset_id, Some("glm".to_string()));
        assert_eq!(result.icon_color, Some("#3366FF".to_string()));
    }
    #[test]
    fn test_create_from_preset_invalid() {
        let (_tmp, path) = setup_temp_store();
        let result = create_from_preset_at("claude", "nonexistent", "key", &path);
        assert!(result.is_err());
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
        let domestic: Vec<_> = presets.iter().filter(|p| p.category == "domestic").collect();
        let relay: Vec<_> = presets.iter().filter(|p| p.category == "relay").collect();
        assert_eq!(domestic.len(), 10);
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
        assert_eq!(result.base_url_anthropic, "https://api.deepseek.com/anthropic");
        assert_eq!(result.api_key, "sk-test-key-123");
        assert!(result.models.is_empty());
        assert_eq!(result.default_model, "");
        assert_eq!(result.preset_id, Some("deepseek".to_string()));
        assert_eq!(result.icon_color, Some("#4D6BFE".to_string()));
        assert!(result.created_at.is_some());
        // ID should be a valid UUID
        assert!(uuid::Uuid::parse_str(&result.id).is_ok());
    }

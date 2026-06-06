//! tool_sync tests — part2 (split out of the original inline test module).

use super::*;


    #[test]
    fn test_merge_json_write_new_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.json");

        let fields: Vec<(&str, Value)> = vec![
            ("key1", Value::String("value1".to_string())),
            ("key2", Value::Number(42.into())),
        ];
        merge_json_write(&path, &fields).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.get("key1").unwrap().as_str().unwrap(), "value1");
        assert_eq!(parsed.get("key2").unwrap().as_i64().unwrap(), 42);
    }

    #[test]
    fn test_merge_json_write_preserves_existing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.json");

        // Write existing content
        let existing = serde_json::json!({"existing": "preserved", "key1": "old_value"});
        std::fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        let fields: Vec<(&str, Value)> = vec![
            ("key1", Value::String("new_value".to_string())),
            ("key2", Value::String("added".to_string())),
        ];
        merge_json_write(&path, &fields).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed.get("existing").unwrap().as_str().unwrap(),
            "preserved"
        );
        assert_eq!(
            parsed.get("key1").unwrap().as_str().unwrap(),
            "new_value"
        );
        assert_eq!(parsed.get("key2").unwrap().as_str().unwrap(), "added");
    }

    #[test]
    fn test_merge_json_env_write_new_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");

        let fields: Vec<(&str, Value)> = vec![
            (
                "ANTHROPIC_BASE_URL",
                Value::String("https://api.test.com".to_string()),
            ),
            (
                "ANTHROPIC_AUTH_TOKEN",
                Value::String("sk-test".to_string()),
            ),
        ];
        merge_json_env_write(&path, &fields).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let env = parsed.get("env").unwrap().as_object().unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").unwrap().as_str().unwrap(),
            "https://api.test.com"
        );
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").unwrap().as_str().unwrap(),
            "sk-test"
        );
    }

    #[test]
    fn test_merge_json_env_write_preserves_all_fields() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");

        // Write existing config with top-level and env fields
        let existing = serde_json::json!({
            "theme": "dark",
            "version": 1,
            "env": {
                "MY_VAR": "my_value",
                "ANTHROPIC_BASE_URL": "old_url"
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        let fields: Vec<(&str, Value)> = vec![
            (
                "ANTHROPIC_BASE_URL",
                Value::String("new_url".to_string()),
            ),
            (
                "ANTHROPIC_MODEL",
                Value::String("model-x".to_string()),
            ),
        ];
        merge_json_env_write(&path, &fields).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Top-level fields preserved
        assert_eq!(parsed.get("theme").unwrap().as_str().unwrap(), "dark");
        assert_eq!(parsed.get("version").unwrap().as_i64().unwrap(), 1);

        // Env block: managed fields updated, custom field preserved
        let env = parsed.get("env").unwrap().as_object().unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").unwrap().as_str().unwrap(),
            "new_url"
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").unwrap().as_str().unwrap(),
            "model-x"
        );
        assert_eq!(
            env.get("MY_VAR").unwrap().as_str().unwrap(),
            "my_value"
        );
    }

    #[test]
    fn test_unsync_claude_code_removes_managed_fields() {
        let tmp = TempDir::new().unwrap();
        let claude_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let config_path = claude_dir.join("settings.json");

        // Write a config with managed + custom fields
        let existing = serde_json::json!({
            "theme": "dark",
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "sk-test",
                "ANTHROPIC_MODEL": "model-a",
                "MY_CUSTOM_VAR": "keep_me"
            }
        });
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        // Simulate unsync logic (same as unsync_claude_code but with custom path)
        create_rolling_backup(&config_path).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let mut json: Value = serde_json::from_str(&content).unwrap();

        if let Some(env_obj) = json.get_mut("env").and_then(|v| v.as_object_mut()) {
            for key in CLAUDE_MANAGED_ENV_KEYS {
                env_obj.remove(*key);
            }
        }

        let output = serde_json::to_string_pretty(&json).unwrap();
        std::fs::write(&config_path, output).unwrap();

        // Verify
        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Top-level preserved
        assert_eq!(parsed.get("theme").unwrap().as_str().unwrap(), "dark");

        // Managed fields removed, custom field preserved
        let env = parsed.get("env").unwrap().as_object().unwrap();
        assert!(!env.contains_key("ANTHROPIC_BASE_URL"));
        assert!(!env.contains_key("ANTHROPIC_AUTH_TOKEN"));
        assert!(!env.contains_key("ANTHROPIC_MODEL"));
        assert_eq!(
            env.get("MY_CUSTOM_VAR").unwrap().as_str().unwrap(),
            "keep_me"
        );
    }

    #[test]
    fn test_unsync_codex_removes_managed_fields() {
        let tmp = TempDir::new().unwrap();
        let codex_dir = tmp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();

        // Write config.toml with managed + custom sections
        let config_path = codex_dir.join("config.toml");
        let config_content = r#"
model_provider = "skillstar"
model = "model-a"

[general]
theme = "dark"

[model_providers.skillstar]
name = "SkillStar"
base_url = "https://api.example.com/v1"
wire_api = "responses"
requires_openai_auth = true

[model_providers.custom]
name = "Custom"
base_url = "https://custom.example.com"
"#;
        std::fs::write(&config_path, config_content).unwrap();

        // Simulate unsync_codex logic for config.toml
        create_rolling_backup(&config_path).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        let mut table: toml::Table = toml::from_str(&content).unwrap();
        table.remove("model_provider");
        table.remove("model");
        if let Some(model_providers) = table.get_mut("model_providers")
            && let Some(mp_table) = model_providers.as_table_mut() {
                mp_table.remove(CODEX_MANAGED_PROVIDER_KEY);
            }
        std::fs::write(&config_path, toml::to_string_pretty(&table).unwrap()).unwrap();

        // Verify config.toml
        let config_result = std::fs::read_to_string(&config_path).unwrap();
        let config_parsed: toml::Table = toml::from_str(&config_result).unwrap();
        assert!(!config_parsed.contains_key("model_provider"));
        assert!(!config_parsed.contains_key("model"));

        // general section preserved
        let general = config_parsed.get("general").unwrap().as_table().unwrap();
        assert_eq!(general.get("theme").unwrap().as_str().unwrap(), "dark");

        // model_providers.custom preserved, skillstar removed
        let mp = config_parsed
            .get("model_providers")
            .unwrap()
            .as_table()
            .unwrap();
        assert!(!mp.contains_key("skillstar"));
        assert!(mp.contains_key("custom"));
    }

    #[test]
    fn test_rolling_backup_keeps_last_5() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("settings.json");
        std::fs::write(&config_path, "{}").unwrap();

        // Create 7 backups manually
        for i in 0..7u128 {
            let backup_name = format!(
                "{}.bak.{}",
                config_path.to_string_lossy(),
                1000 + i
            );
            std::fs::write(&backup_name, format!("backup {}", i)).unwrap();
        }

        // Run cleanup
        cleanup_old_backups(&config_path, 5).unwrap();

        // Count remaining backups
        let prefix = "settings.json.bak.";
        let remaining: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(prefix)
            })
            .collect();

        assert_eq!(remaining.len(), 5);

        // Verify the 5 most recent are kept (timestamps 1002..1006)
        for i in 2..7u128 {
            let backup_name = format!(
                "{}.bak.{}",
                config_path.to_string_lossy(),
                1000 + i
            );
            assert!(
                Path::new(&backup_name).exists(),
                "Backup {} should exist",
                1000 + i
            );
        }

        // Verify the 2 oldest are removed (timestamps 1000, 1001)
        for i in 0..2u128 {
            let backup_name = format!(
                "{}.bak.{}",
                config_path.to_string_lossy(),
                1000 + i
            );
            assert!(
                !Path::new(&backup_name).exists(),
                "Backup {} should be removed",
                1000 + i
            );
        }
    }

    #[test]
    fn test_create_rolling_backup_creates_file() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("settings.json");
        std::fs::write(&config_path, r#"{"key": "value"}"#).unwrap();

        let backup_path = create_rolling_backup(&config_path).unwrap();

        // Backup file exists
        assert!(backup_path.exists());

        // Backup has the original content
        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, r#"{"key": "value"}"#);

        // Original file still exists
        assert!(config_path.exists());
    }

    #[test]
    fn test_codex_auth_json_merge_write() {
        let tmp = TempDir::new().unwrap();
        let auth_path = tmp.path().join("auth.json");

        // Write existing auth.json with extra fields
        let existing = serde_json::json!({
            "OTHER_KEY": "keep_me",
            "OPENAI_API_KEY": "old-key"
        });
        std::fs::write(&auth_path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        // Merge write new API key
        let fields: Vec<(&str, Value)> = vec![
            ("OPENAI_API_KEY", Value::String("new-key-12345".to_string())),
        ];
        merge_json_write(&auth_path, &fields).unwrap();

        let content = std::fs::read_to_string(&auth_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed.get("OPENAI_API_KEY").unwrap().as_str().unwrap(),
            "new-key-12345"
        );
        assert_eq!(
            parsed.get("OTHER_KEY").unwrap().as_str().unwrap(),
            "keep_me"
        );
    }

    #[test]
    fn test_resync_active_tools_syncs_correct_tools() {
        use crate::providers::{FlatProvidersStore, ToolActivation};

        // Sandbox: resync writes real config files; keep them off the dev's home.
        use_sandbox_home();
        let provider = make_test_provider_flat();
        let store = FlatProvidersStore {
            version: 2,
            providers: vec![provider.clone()],
            tool_activations: {
                let mut map = HashMap::new();
                map.insert(
                    "claude-code".to_string(),
                    Some(ToolActivation {
                        provider_id: "test-uuid-1234".to_string(),
                        model: "model-a".to_string(),
                        settings: None,
                        last_sync_at: None,
                    }),
                );
                map.insert(
                    "codex".to_string(),
                    Some(ToolActivation {
                        provider_id: "test-uuid-1234".to_string(),
                        model: "model-b".to_string(),
                        settings: None,
                        last_sync_at: None,
                    }),
                );
                map
            },
        };

        // resync_active_tools writes to real config paths; `use_sandbox_home()`
        // above re-roots them under a temp dir. We verify it returns results for
        // both tools.
        let results = resync_active_tools(&store, "test-uuid-1234");
        assert_eq!(results.len(), 2);

        // Both tools should be attempted
        let tool_ids: Vec<&str> = results.iter().map(|r| r.tool_id.as_str()).collect();
        assert!(tool_ids.contains(&"claude-code"));
        assert!(tool_ids.contains(&"codex"));
    }

    #[test]
    fn test_resync_active_tools_provider_not_found() {
        use crate::providers::FlatProvidersStore;

        let store = FlatProvidersStore::default();
        let results = resync_active_tools(&store, "nonexistent-id");

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0]
            .error
            .as_ref()
            .unwrap()
            .contains("not found"));
    }

    #[test]
    fn test_resync_active_tools_skips_other_providers() {
        use crate::providers::{FlatProvidersStore, ToolActivation};

        // Sandbox: resync writes real config files; keep them off the dev's home.
        use_sandbox_home();
        let provider = make_test_provider_flat();
        let store = FlatProvidersStore {
            version: 2,
            providers: vec![provider.clone()],
            tool_activations: {
                let mut map = HashMap::new();
                // Claude Code uses a different provider
                map.insert(
                    "claude-code".to_string(),
                    Some(ToolActivation {
                        provider_id: "other-provider-id".to_string(),
                        model: "other-model".to_string(),
                        settings: None,
                        last_sync_at: None,
                    }),
                );
                // Codex uses our provider
                map.insert(
                    "codex".to_string(),
                    Some(ToolActivation {
                        provider_id: "test-uuid-1234".to_string(),
                        model: "model-a".to_string(),
                        settings: None,
                        last_sync_at: None,
                    }),
                );
                map
            },
        };

        let results = resync_active_tools(&store, "test-uuid-1234");
        // Should only sync codex (the one using our provider)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_id, "codex");
    }


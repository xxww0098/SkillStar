//! tool_sync tests — part1 (split out of the original inline test module).

use super::*;

#[test]
fn test_resolve_tool_config_path_claude_code() {
    let path = resolve_tool_config_path("claude-code").unwrap();
    let path_str = path.to_string_lossy();
    assert!(path_str.contains(".claude"));
    assert!(path_str.ends_with("settings.json"));
}

#[test]
fn test_resolve_tool_config_path_codex() {
    let path = resolve_tool_config_path("codex").unwrap();
    let path_str = path.to_string_lossy();
    assert!(path_str.contains(".codex"));
    assert!(path_str.ends_with("config.toml"));
}

#[test]
fn test_resolve_tool_config_path_unknown() {
    let result = resolve_tool_config_path("unknown-tool");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown tool_id"));
}

#[test]
fn test_generate_claude_code_config() {
    let settings = make_test_settings();
    let json_str = generate_claude_code_config(&settings).unwrap();
    let parsed: HashMap<String, Value> = serde_json::from_str(&json_str).unwrap();
    assert_eq!(
        parsed.get("apiUrl").unwrap().as_str().unwrap(),
        "https://api.example.com/v1"
    );
    assert_eq!(
        parsed.get("apiKey").unwrap().as_str().unwrap(),
        "sk-test-key-12345"
    );
}

#[test]
fn test_generate_codex_config() {
    let settings = make_test_settings();
    let toml_str = generate_codex_config(&settings).unwrap();
    let parsed: toml::Table = toml::from_str(&toml_str).unwrap();
    let provider = parsed.get("provider").unwrap().as_table().unwrap();
    assert_eq!(
        provider.get("base_url").unwrap().as_str().unwrap(),
        "https://api.example.com/v1"
    );
    assert_eq!(
        provider.get("api_key").unwrap().as_str().unwrap(),
        "sk-test-key-12345"
    );
}

#[test]
fn test_write_claude_code_config_new_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    let settings = make_test_settings();

    write_claude_code_config(&path, &settings).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: HashMap<String, Value> = serde_json::from_str(&content).unwrap();
    assert_eq!(
        parsed.get("apiUrl").unwrap().as_str().unwrap(),
        settings.base_url
    );
    assert_eq!(
        parsed.get("apiKey").unwrap().as_str().unwrap(),
        settings.api_key
    );
}

#[test]
fn test_write_claude_code_config_merges_existing() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");

    // Write existing config with extra fields
    let existing = serde_json::json!({
        "theme": "dark",
        "existingField": 42
    });
    std::fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

    let settings = make_test_settings();
    write_claude_code_config(&path, &settings).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: HashMap<String, Value> = serde_json::from_str(&content).unwrap();
    // New fields are present
    assert_eq!(
        parsed.get("apiUrl").unwrap().as_str().unwrap(),
        settings.base_url
    );
    assert_eq!(
        parsed.get("apiKey").unwrap().as_str().unwrap(),
        settings.api_key
    );
    // Existing fields are preserved
    assert_eq!(parsed.get("theme").unwrap().as_str().unwrap(), "dark");
    assert_eq!(parsed.get("existingField").unwrap().as_i64().unwrap(), 42);
}

#[test]
fn test_write_codex_config_new_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    let settings = make_test_settings();

    write_codex_config(&path, &settings).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Table = toml::from_str(&content).unwrap();
    let provider = parsed.get("provider").unwrap().as_table().unwrap();
    assert_eq!(
        provider.get("base_url").unwrap().as_str().unwrap(),
        settings.base_url
    );
    assert_eq!(
        provider.get("api_key").unwrap().as_str().unwrap(),
        settings.api_key
    );
}

#[test]
fn test_write_codex_config_merges_existing() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");

    // Write existing config with extra sections
    let existing = r#"
[general]
theme = "dark"

[provider]
base_url = "https://old-api.example.com"
api_key = "old-key"
"#;
    std::fs::write(&path, existing).unwrap();

    let settings = make_test_settings();
    write_codex_config(&path, &settings).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Table = toml::from_str(&content).unwrap();
    // Provider section is updated
    let provider = parsed.get("provider").unwrap().as_table().unwrap();
    assert_eq!(
        provider.get("base_url").unwrap().as_str().unwrap(),
        settings.base_url
    );
    assert_eq!(
        provider.get("api_key").unwrap().as_str().unwrap(),
        settings.api_key
    );
    // Existing sections are preserved
    let general = parsed.get("general").unwrap().as_table().unwrap();
    assert_eq!(general.get("theme").unwrap().as_str().unwrap(), "dark");
}

#[test]
fn test_sync_provider_to_tool_creates_backup() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let config_path = claude_dir.join("settings.json");

    // Write an existing config
    let existing = serde_json::json!({"existingKey": "existingValue"});
    std::fs::write(&config_path, serde_json::to_string(&existing).unwrap()).unwrap();

    // We can't easily test sync_provider_to_tool directly because it uses
    // resolve_tool_config_path which points to the real home dir.
    // Instead, test the inner write + backup logic directly.
    let settings = make_test_settings();

    // Simulate backup
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let backup_path = format!("{}.bak.{}", config_path.display(), timestamp);
    std::fs::copy(&config_path, &backup_path).unwrap();

    // Write new config
    write_claude_code_config(&config_path, &settings).unwrap();

    // Verify backup has original content
    let backup_content = std::fs::read_to_string(&backup_path).unwrap();
    let backup_parsed: Value = serde_json::from_str(&backup_content).unwrap();
    assert_eq!(
        backup_parsed.get("existingKey").unwrap().as_str().unwrap(),
        "existingValue"
    );

    // Verify new config has updated content
    let new_content = std::fs::read_to_string(&config_path).unwrap();
    let new_parsed: HashMap<String, Value> = serde_json::from_str(&new_content).unwrap();
    assert_eq!(
        new_parsed.get("apiUrl").unwrap().as_str().unwrap(),
        settings.base_url
    );
}

#[test]
fn test_sync_provider_to_all_tools_isolation() {
    // Test that sync_provider_to_all_tools returns results for each tool
    // even if some fail (unknown tool_id will fail)
    let provider = make_test_provider();
    let tool_ids = vec![
        "unknown-tool".to_string(),    // This will fail
        "another-unknown".to_string(), // This will also fail
    ];

    let results = sync_provider_to_all_tools(&provider, &tool_ids);
    assert_eq!(results.len(), 2);
    // Both should fail but not panic
    assert!(!results[0].success);
    assert!(!results[1].success);
    assert!(results[0].error.is_some());
    assert!(results[1].error.is_some());
}

#[test]
fn test_get_tool_config_targets_returns_both_tools() {
    let targets = get_tool_config_targets().unwrap();
    assert_eq!(targets.len(), 5);

    let claude_target = targets.iter().find(|t| t.tool_id == "claude-code").unwrap();
    assert_eq!(claude_target.display_name, "Claude Code");
    assert!(claude_target.config_path.contains(".claude"));

    let codex_target = targets.iter().find(|t| t.tool_id == "codex").unwrap();
    assert_eq!(codex_target.display_name, "Codex");
    assert!(codex_target.config_path.contains(".codex"));

    let gemini_target = targets.iter().find(|t| t.tool_id == "gemini").unwrap();
    assert_eq!(gemini_target.display_name, "Gemini CLI");
    assert!(gemini_target.config_path.contains(".gemini"));
}

// =========================================================================
// Flat store sync tests (v2 architecture)
// =========================================================================

#[test]
fn test_sync_to_gemini_inner_new_file() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join(".gemini").join(".env");
    let provider = make_test_provider_flat();

    let result = sync_to_gemini_inner(&provider, "model-b", &config_path).unwrap();
    assert!(result.is_none(), "no backup when file is new");

    let pairs = parse_env_file(&std::fs::read_to_string(&config_path).unwrap());
    let get = |k: &str| {
        pairs
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(
        get("GOOGLE_GEMINI_BASE_URL").as_deref(),
        Some("https://api.example.com/v1")
    );
    assert_eq!(
        get("GEMINI_API_KEY").as_deref(),
        Some("sk-test-key-flat-12345")
    );
    assert_eq!(get("GEMINI_MODEL").as_deref(), Some("model-b"));
}

#[test]
fn test_sync_to_gemini_inner_preserves_user_keys_and_backs_up() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join(".gemini");
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join(".env");
    std::fs::write(
        &config_path,
        "# comment\nMY_CUSTOM=keepme\nGEMINI_API_KEY=old\n",
    )
    .unwrap();

    let provider = make_test_provider_flat();
    let backup = sync_to_gemini_inner(&provider, "", &config_path).unwrap();
    assert!(backup.is_some(), "existing file should be backed up");

    let pairs = parse_env_file(&std::fs::read_to_string(&config_path).unwrap());
    let get = |k: &str| {
        pairs
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };
    // Unmanaged key preserved
    assert_eq!(get("MY_CUSTOM").as_deref(), Some("keepme"));
    // Managed key overwritten
    assert_eq!(
        get("GEMINI_API_KEY").as_deref(),
        Some("sk-test-key-flat-12345")
    );
    // Empty model falls back to provider default_model ("model-a")
    assert_eq!(get("GEMINI_MODEL").as_deref(), Some("model-a"));
}

#[test]
fn test_sync_to_gemini_inner_fails_without_base_url() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join(".gemini").join(".env");
    let mut provider = make_test_provider_flat();
    provider.base_url_openai = String::new();
    assert!(sync_to_gemini_inner(&provider, "model-a", &config_path).is_err());
}

#[test]
fn test_build_opencode_provider_block_uses_model_catalog_metadata() {
    let mut provider = make_test_provider_flat();
    provider.models = vec!["model-a".to_string(), "model-b".to_string()];
    provider.meta = Some(serde_json::json!({
        "model_catalog": [
            {
                "id": "model-a",
                "display_name": "Model A Display",
                "context_length": 200000,
                "max_completion_tokens": 65536,
                "cost": { "input": 0.2, "output": 0.8 }
            },
            {
                "id": "model-b",
                "display_name": "Model B Display",
                "context_length": 128000
            }
        ]
    }));

    let block = build_opencode_provider_block(&provider, "model-a");
    let model_a = block
        .get("models")
        .and_then(|v| v.get("model-a"))
        .expect("model-a entry");

    assert_eq!(
        model_a.get("name").and_then(Value::as_str),
        Some("Model A Display")
    );
    assert_eq!(
        model_a
            .get("limit")
            .and_then(|v| v.get("context"))
            .and_then(Value::as_u64),
        Some(200000)
    );
    assert_eq!(
        model_a
            .get("limit")
            .and_then(|v| v.get("output"))
            .and_then(Value::as_u64),
        Some(65536)
    );
    assert_eq!(
        model_a
            .get("cost")
            .and_then(|v| v.get("input"))
            .and_then(Value::as_f64),
        Some(0.2)
    );
}

#[test]
fn test_sync_to_claude_code_inner_new_file() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join(".claude").join("settings.json");
    let provider = make_test_provider_flat();

    let result = sync_to_claude_code_inner(&provider, "model-a", &config_path).unwrap();

    // No backup since file didn't exist
    assert!(result.is_none());

    // Verify the written content
    let content = std::fs::read_to_string(&config_path).unwrap();
    let parsed: Value = serde_json::from_str(&content).unwrap();
    let env = parsed.get("env").unwrap().as_object().unwrap();
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL").unwrap().as_str().unwrap(),
        "https://api.example.com/anthropic"
    );
    assert_eq!(
        env.get("ANTHROPIC_AUTH_TOKEN").unwrap().as_str().unwrap(),
        "sk-test-key-flat-12345"
    );
    assert_eq!(
        env.get("ANTHROPIC_MODEL").unwrap().as_str().unwrap(),
        "model-a"
    );
}

#[test]
fn test_sync_to_claude_code_inner_merges_existing() {
    let tmp = TempDir::new().unwrap();
    let claude_dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let config_path = claude_dir.join("settings.json");

    // Write existing config with extra fields
    let existing = serde_json::json!({
        "theme": "dark",
        "env": {
            "MY_CUSTOM_VAR": "custom_value",
            "ANTHROPIC_BASE_URL": "old_url"
        }
    });
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    let provider = make_test_provider_flat();
    let backup = sync_to_claude_code_inner(&provider, "model-b", &config_path).unwrap();

    // Backup should exist
    assert!(backup.is_some());
    assert!(backup.unwrap().exists());

    // Verify the written content
    let content = std::fs::read_to_string(&config_path).unwrap();
    let parsed: Value = serde_json::from_str(&content).unwrap();

    // Top-level fields preserved
    assert_eq!(parsed.get("theme").unwrap().as_str().unwrap(), "dark");

    // Env block: managed fields updated, custom field preserved
    let env = parsed.get("env").unwrap().as_object().unwrap();
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL").unwrap().as_str().unwrap(),
        "https://api.example.com/anthropic"
    );
    assert_eq!(
        env.get("ANTHROPIC_AUTH_TOKEN").unwrap().as_str().unwrap(),
        "sk-test-key-flat-12345"
    );
    assert_eq!(
        env.get("ANTHROPIC_MODEL").unwrap().as_str().unwrap(),
        "model-b"
    );
    assert_eq!(
        env.get("MY_CUSTOM_VAR").unwrap().as_str().unwrap(),
        "custom_value"
    );
}

#[test]
fn test_sync_to_claude_code_inner_fails_without_anthropic_url() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("settings.json");

    let mut provider = make_test_provider_flat();
    provider.base_url_anthropic = String::new();

    let result = sync_to_claude_code_inner(&provider, "model-a", &config_path);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Anthropic-compatible endpoint")
    );
}

#[test]
fn test_write_codex_config_flat_new_file() {
    let tmp = TempDir::new().unwrap();
    let codex_dir = tmp.path().join(".codex");
    let config_path = codex_dir.join("config.toml");

    let provider = make_test_provider_flat();
    let activation = ToolActivation {
        provider_id: provider.id.clone(),
        model: "model-a".to_string(),
        settings: None,
        last_sync_at: None,
    };
    write_codex_config_flat(
        &config_path,
        &provider,
        &activation,
        &CodexSettings::default(),
    )
    .unwrap();

    let content = std::fs::read_to_string(&config_path).unwrap();
    let parsed: toml::Table = toml::from_str(&content).unwrap();

    assert_eq!(
        parsed.get("model_provider").unwrap().as_str().unwrap(),
        "skillstar"
    );
    assert_eq!(parsed.get("model").unwrap().as_str().unwrap(), "model-a");

    let mp = parsed.get("model_providers").unwrap().as_table().unwrap();
    let skillstar = mp.get("skillstar").unwrap().as_table().unwrap();
    assert_eq!(
        skillstar.get("name").unwrap().as_str().unwrap(),
        "SkillStar"
    );
    assert_eq!(
        skillstar.get("base_url").unwrap().as_str().unwrap(),
        "https://api.example.com/v1"
    );
    assert_eq!(
        skillstar.get("wire_api").unwrap().as_str().unwrap(),
        "responses"
    );
    assert!(
        skillstar
            .get("requires_openai_auth")
            .unwrap()
            .as_bool()
            .unwrap()
    );
}

#[test]
fn test_write_codex_config_flat_merges_existing() {
    let tmp = TempDir::new().unwrap();
    let codex_dir = tmp.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let config_path = codex_dir.join("config.toml");

    // Write existing config with extra sections
    let existing = r#"
[general]
theme = "dark"
auto_update = true

[model_providers.custom]
name = "Custom Provider"
base_url = "https://custom.example.com"
"#;
    std::fs::write(&config_path, existing).unwrap();

    let provider = make_test_provider_flat();
    let activation = ToolActivation {
        provider_id: provider.id.clone(),
        model: "model-b".to_string(),
        settings: None,
        last_sync_at: None,
    };
    write_codex_config_flat(
        &config_path,
        &provider,
        &activation,
        &CodexSettings::default(),
    )
    .unwrap();

    let content = std::fs::read_to_string(&config_path).unwrap();
    let parsed: toml::Table = toml::from_str(&content).unwrap();

    // Managed fields are set
    assert_eq!(
        parsed.get("model_provider").unwrap().as_str().unwrap(),
        "skillstar"
    );
    assert_eq!(parsed.get("model").unwrap().as_str().unwrap(), "model-b");

    // Existing sections preserved
    let general = parsed.get("general").unwrap().as_table().unwrap();
    assert_eq!(general.get("theme").unwrap().as_str().unwrap(), "dark");
    assert!(general.get("auto_update").unwrap().as_bool().unwrap());

    // Existing model_providers.custom preserved
    let mp = parsed.get("model_providers").unwrap().as_table().unwrap();
    let custom = mp.get("custom").unwrap().as_table().unwrap();
    assert_eq!(
        custom.get("name").unwrap().as_str().unwrap(),
        "Custom Provider"
    );

    // New model_providers.skillstar added
    let skillstar = mp.get("skillstar").unwrap().as_table().unwrap();
    assert_eq!(
        skillstar.get("base_url").unwrap().as_str().unwrap(),
        "https://api.example.com/v1"
    );
}

// ---------------------------------------------------------------------------
// ZCode (OpenCode-schema profile under ~/.zcode/v2/config.json)
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_tool_config_path_zcode() {
    use_sandbox_home();
    let path = resolve_tool_config_path("zcode").unwrap();
    let path_str = path.to_string_lossy();
    assert!(
        path_str.contains(".zcode"),
        "expected .zcode in path, got {path_str}"
    );
    assert!(
        path_str.contains("v2"),
        "expected v2 subdir, got {path_str}"
    );
    assert!(path_str.ends_with("config.json"));
}

#[test]
fn test_sync_to_zcode_writes_provider_block() {
    use_sandbox_home();
    let provider = make_test_provider_flat();
    let result = sync_to_zcode(&provider, "model-a").unwrap();

    assert!(result.success, "sync should succeed: {:?}", result.error);
    assert_eq!(result.tool_id, "zcode");

    let config_path = resolve_zcode_config_path().unwrap();
    let content = std::fs::read_to_string(&config_path).unwrap();
    let json: Value = serde_json::from_str(&content).unwrap();

    // Preserves the OpenCode schema marker.
    assert_eq!(
        json.get("$schema").and_then(Value::as_str),
        Some("https://opencode.ai/config.json")
    );

    // provider.skillstar block injected with the provider's name + baseURL.
    let block = json
        .get("provider")
        .and_then(|p| p.get("skillstar"))
        .expect("provider.skillstar block should exist");
    assert_eq!(
        block.get("name").and_then(Value::as_str),
        Some("Test Provider")
    );
    assert_eq!(
        block
            .get("options")
            .and_then(|o| o.get("baseURL"))
            .and_then(Value::as_str),
        Some("https://api.example.com/v1")
    );
}

#[test]
fn test_unsync_zcode_removes_only_managed_block() {
    // Use an isolated TempDir instead of the shared sandbox: `unsync_opencode_at`
    // is the shared core that zcode reuses, and a standalone path avoids
    // cross-test races on the global sandbox's config.json.
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.json");
    // Seed a config with both the managed block and a user-owned provider.
    let seeded = serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "provider": {
            "skillstar": { "name": "SkillStar", "options": { "baseURL": "https://a" } },
            "user-owned": { "name": "Mine", "options": { "baseURL": "https://b" } }
        }
    });
    std::fs::write(&config_path, serde_json::to_string_pretty(&seeded).unwrap()).unwrap();

    // `unsync_opencode_at` is the shared removal core that `unsync_zcode`
    // delegates to (ZCode uses the OpenCode schema verbatim). Driving it on an
    // isolated path exercises the real removal logic without touching the
    // shared sandbox HOME.
    unsync_opencode_at(&config_path).unwrap();

    let content = std::fs::read_to_string(&config_path).unwrap();
    let json: Value = serde_json::from_str(&content).unwrap();
    let providers = json
        .get("provider")
        .and_then(Value::as_object)
        .expect("provider node should remain");

    assert!(
        !providers.contains_key("skillstar"),
        "managed block must be removed"
    );
    assert!(
        providers.contains_key("user-owned"),
        "user-owned provider must be preserved"
    );
}

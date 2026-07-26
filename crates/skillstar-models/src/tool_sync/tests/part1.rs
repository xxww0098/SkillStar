//! tool_sync tests — part1 (split out of the original inline test module).

use super::*;
use crate::providers::{ToolActivation, ToolBinding};

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

    let pi_target = targets.iter().find(|t| t.tool_id == "pi").unwrap();
    assert_eq!(pi_target.display_name, "Pi");
    assert!(pi_target.config_path.contains(".pi"));
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
fn test_sync_to_claude_code_inner_empty_model_skips_key() {
    // Regression: an empty model (provider with no default_model, activated
    // without an explicit model) must NOT be written as `"ANTHROPIC_MODEL": ""`
    // — that produces an invalid Claude Code config. Instead the key is dropped
    // (Null → removed by merge_json_env_write), while BASE_URL/AUTH_TOKEN still
    // land in the env block.
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join(".claude").join("settings.json");
    let provider = make_test_provider_flat();

    let result = sync_to_claude_code_inner(&provider, "", &config_path).unwrap();
    assert!(result.is_none(), "no backup expected for a new file");

    let content = std::fs::read_to_string(&config_path).unwrap();
    let parsed: Value = serde_json::from_str(&content).unwrap();
    let env = parsed.get("env").unwrap().as_object().unwrap();

    // Credentials still written.
    assert_eq!(
        env.get("ANTHROPIC_BASE_URL").unwrap().as_str().unwrap(),
        "https://api.example.com/anthropic"
    );
    assert_eq!(
        env.get("ANTHROPIC_AUTH_TOKEN").unwrap().as_str().unwrap(),
        "sk-test-key-flat-12345"
    );
    // Empty model is dropped, not written as "".
    assert!(
        !env.contains_key("ANTHROPIC_MODEL"),
        "expected ANTHROPIC_MODEL to be absent for empty model, got: {env:?}"
    );
}


// ---------------------------------------------------------------------------
// Three-state auth_mode (api_key / oauth / third_party)
// ---------------------------------------------------------------------------

/// Helper: build a `ToolActivation` with explicit Codex settings.
fn make_codex_activation(provider: &ProviderEntryFlat, settings: CodexSettings) -> ToolActivation {
    ToolActivation {
        provider_id: provider.id.clone(),
        model: "model-a".to_string(),
        settings: Some(serde_json::to_value(&settings).unwrap()),
        last_sync_at: None,
    }
}

#[test]
fn test_codex_third_party_writes_env_key_and_disables_openai_auth() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");

    let provider = make_test_provider_flat();
    let settings = CodexSettings {
        wire_api: "chat".to_string(),
        auth_mode: CODEX_AUTH_MODE_THIRD_PARTY.to_string(),
    };
    let binding = ToolBinding {
        entries: vec![make_codex_activation(&provider, settings)],
        active_index: 0,
    };

    sync_codex_binding_inner(&binding, std::slice::from_ref(&provider), &config_path).unwrap();

    let parsed: toml::Table =
        toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let managed = parsed
        .get("model_providers")
        .unwrap()
        .get(skillstar_managed_key(&provider.id).as_str())
        .unwrap()
        .as_table()
        .unwrap();

    // third_party ⇒ requires_openai_auth = false
    assert!(
        !managed
            .get("requires_openai_auth")
            .unwrap()
            .as_bool()
            .unwrap()
    );
    // env_key is written and follows the SKILLSTAR_<prefix>_KEY rule.
    let env_key = managed.get("env_key").unwrap().as_str().unwrap();
    assert!(
        env_key.starts_with("SKILLSTAR_"),
        "env_key must be namespaced: got {env_key}"
    );
    assert!(env_key.ends_with("_KEY"));
    // Provider id "test-uuid-1234" → prefix "test-uui" → "TEST_UUI".
    assert_eq!(env_key, "SKILLSTAR_TEST_UUI_KEY");
}

#[test]
fn test_codex_oauth_enables_openai_auth_and_no_env_key() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");

    let provider = make_test_provider_flat();
    let settings = CodexSettings {
        wire_api: "responses".to_string(),
        auth_mode: CODEX_AUTH_MODE_OAUTH.to_string(),
    };
    let binding = ToolBinding {
        entries: vec![make_codex_activation(&provider, settings)],
        active_index: 0,
    };

    sync_codex_binding_inner(&binding, std::slice::from_ref(&provider), &config_path).unwrap();

    let parsed: toml::Table =
        toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let managed = parsed
        .get("model_providers")
        .unwrap()
        .get(skillstar_managed_key(&provider.id).as_str())
        .unwrap()
        .as_table()
        .unwrap();

    // oauth ⇒ requires_openai_auth = true (routes through ChatGPT token)
    assert!(
        managed
            .get("requires_openai_auth")
            .unwrap()
            .as_bool()
            .unwrap()
    );
    // oauth never emits env_key
    assert!(managed.get("env_key").is_none());
}

#[test]
fn test_codex_oauth_and_third_party_preserve_existing_auth_json() {
    // Regression guard: oauth AND third_party modes must NEVER touch auth.json.
    // A pre-existing ChatGPT OAuth token object must survive both syncs.
    // (Both cases are checked in one test to avoid them racing each other on
    // the shared sandbox's single ~/.codex/auth.json.)
    use_sandbox_home();
    let codex_dir = resolve_codex_auth_path()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::create_dir_all(&codex_dir).unwrap();
    let auth_path = codex_dir.join("auth.json");

    let provider = make_test_provider_flat();

    let check_mode = |mode: &str| {
        // Seed a realistic ChatGPT OAuth auth.json before each sync.
        let oauth_blob = serde_json::json!({
            "OPENAI_API_KEY": null,
            "tokens": {
                "access_token": format!("eyJchatgpt-access-{mode}"),
                "refresh_token": format!("eyJchatgpt-refresh-{mode}"),
                "id_token": "eyJchatgpt-id",
                "account_id": "acct_123"
            }
        });
        std::fs::write(&auth_path, oauth_blob.to_string()).unwrap();

        let settings = CodexSettings {
            wire_api: "responses".to_string(),
            auth_mode: mode.to_string(),
        };
        let binding = ToolBinding {
            entries: vec![make_codex_activation(&provider, settings)],
            active_index: 0,
        };

        let _ = sync_codex_binding(&binding, std::slice::from_ref(&provider));

        // auth.json is byte-identical (neither mode writes it).
        let after = std::fs::read_to_string(&auth_path).unwrap();
        let after_json: serde_json::Value = serde_json::from_str(&after).unwrap();
        assert_eq!(
            after_json, oauth_blob,
            "OAuth token must survive {mode} sync"
        );
    };

    check_mode(CODEX_AUTH_MODE_OAUTH);
    check_mode(CODEX_AUTH_MODE_THIRD_PARTY);
}

#[test]
fn test_codex_env_key_rule_is_stable_and_shell_safe() {
    // Non-alphanumeric chars in the id (dashes from a UUID) collapse to '_'.
    let mut p = make_test_provider_flat();
    p.id = "a1b2c3d4-rest-of-uuid".to_string();
    assert_eq!(codex_env_key_for(&p), "SKILLSTAR_A1B2C3D4_KEY");

    // Empty / pathological id still yields a usable var name.
    p.id = "".to_string();
    let fallback = codex_env_key_for(&p);
    assert!(fallback.starts_with("SKILLSTAR_") && fallback.ends_with("_KEY"));
}

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
fn test_get_tool_config_targets_returns_all_tools() {
    let targets = get_tool_config_targets().unwrap();
    assert_eq!(targets.len(), 6);

    let claude_target = targets.iter().find(|t| t.tool_id == "claude-code").unwrap();
    assert_eq!(claude_target.display_name, "Claude Code");
    assert!(claude_target.config_path.contains(".claude"));

    let desktop_target = targets
        .iter()
        .find(|t| t.tool_id == "claude-desktop")
        .unwrap();
    assert_eq!(desktop_target.display_name, "Claude Desktop");
    assert!(desktop_target.config_path.contains(".claude-desktop"));

    let codex_target = targets.iter().find(|t| t.tool_id == "codex").unwrap();
    assert_eq!(codex_target.display_name, "Codex");
    assert!(codex_target.config_path.contains(".codex"));

    let pi_target = targets.iter().find(|t| t.tool_id == "pi").unwrap();
    assert_eq!(pi_target.display_name, "Pi");
    assert!(pi_target.config_path.contains(".pi"));

    let omp_target = targets.iter().find(|t| t.tool_id == "omp").unwrap();
    assert_eq!(omp_target.display_name, "Oh My Pi");
    assert!(omp_target.config_path.contains(".omp"));
}

// Provider-store sync no longer writes Agent configs. Codex loopback config
// belongs to skillstar-gateway.

fn assert_absent(path: &std::path::Path) {
    assert!(!path.exists(), "sync must not create {}", path.display());
}

#[test]
fn sync_to_claude_code_inner_does_not_create_a_file() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("settings.json");
    let provider = make_test_provider_flat();

    let result =
        sync_to_claude_code_inner(&provider, "model-a", &no_roles(), &config_path).unwrap();

    assert!(result.is_none());
    assert_absent(&config_path);
}

#[test]
fn sync_to_claude_code_inner_leaves_existing_bytes() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("settings.json");
    let before = r#"{
          "env": {
            "ANTHROPIC_BASE_URL": "https://proxy.example/anthropic",
            "ANTHROPIC_AUTH_TOKEN": "sk-managed",
            "ANTHROPIC_MODEL": "claude-sonnet",
            "MY_CUSTOM": "keep-me"
          },
          "other": true
        }"#;
    std::fs::write(&config_path, before).unwrap();

    let provider = make_test_provider_flat();
    let backup =
        sync_to_claude_code_inner(&provider, "model-b", &no_roles(), &config_path).unwrap();

    assert!(backup.is_none());
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
}

#[test]
fn sync_codex_binding_inner_does_not_create_a_file() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    let provider = make_test_provider_flat();
    let binding = AgentBinding::single(entry(&provider.id, "model-a"));

    sync_codex_binding_inner(&binding, std::slice::from_ref(&provider), &config_path).unwrap();

    assert_absent(&config_path);
}

#[test]
fn sync_codex_binding_inner_leaves_vendor_bytes() {
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("config.toml");
    let before = "\
model_provider = \"skillstar_old-provider\"
model = \"gpt-5\"

[model_providers.skillstar_old-provider]
name = \"SkillStar\"
base_url = \"https://api.example.com/v1\"
wire_api = \"chat\"
requires_openai_auth = false
";
    std::fs::write(&config_path, before).unwrap();

    let provider = make_test_provider_flat();
    let binding = AgentBinding::single(entry(&provider.id, "model-a"));
    sync_codex_binding_inner(&binding, std::slice::from_ref(&provider), &config_path).unwrap();

    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), before);
}

#[test]
fn sync_claude_desktop_binding_inner_does_not_create_a_marker() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("skillstar-binding.json");
    let provider = make_test_provider_flat();
    let binding = AgentBinding::single(entry(&provider.id, "model-a"));

    let result =
        sync_claude_desktop_binding_inner(&binding, std::slice::from_ref(&provider), &path)
            .unwrap();

    assert!(result.is_none());
    assert_absent(&path);
}

#[test]
fn removed_opencode_block_builder_is_not_part_of_sync() {
    // The catalog-metadata block builder used to be the OpenCode write path.
    // Sync now returns without creating the file, including when the provider
    // has no Anthropic endpoint.
    let tmp = TempDir::new().unwrap();
    let config_path = tmp.path().join("opencode.json");
    let mut provider = make_test_provider_flat();
    provider.endpoints.anthropic_messages = None;
    let binding = AgentBinding::single(entry(&provider.id, "model-a"));

    sync_opencode_binding_inner(&binding, std::slice::from_ref(&provider), &config_path).unwrap();
    assert_absent(&config_path);
}

// Kept below: auth.json must survive a Codex sync, and the env-key helper is
// still used by unsync's managed-key spelling.

/// Helper: build a binding entry with explicit Codex settings.
fn make_codex_activation(provider: &Provider, settings: CodexSettings) -> BindingEntry {
    BindingEntry {
        provider_id: provider.id.clone(),
        model: "model-a".to_string(),
        settings: Some(serde_json::to_value(&settings).unwrap()),
        last_sync_at_ms: None,
    }
}

#[test]
fn test_codex_oauth_and_third_party_preserve_existing_auth_json() {
    // Regression guard: oauth AND third_party modes must NEVER touch auth.json.
    // A pre-existing ChatGPT OAuth token object must survive both syncs.
    // (Both cases share one test body only for brevity — the sandbox HOME is
    // per-test now, so they no longer race anyone on ~/.codex/auth.json.)
    let _sandbox = use_sandbox_home();
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
            auth_mode: mode.to_string(),
        };
        let binding = AgentBinding {
            entries: vec![make_codex_activation(&provider, settings)],
            roles: Default::default(),
            active_index: 0,
            settings: None,
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
    assert_eq!(codex_env_key_for(&p.id), "SKILLSTAR_A1B2C3D4_KEY");

    // Empty / pathological id still yields a usable var name.
    p.id = "".to_string();
    let fallback = codex_env_key_for(&p.id);
    assert!(fallback.starts_with("SKILLSTAR_") && fallback.ends_with("_KEY"));
}

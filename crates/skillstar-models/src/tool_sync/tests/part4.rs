//! Multi-provider sync no longer writes Codex, OpenCode, or Pi configs.
//! Unsync still removes managed keys. Paths are isolated temp files.

use super::*;

#[test]
fn managed_key_is_prefixed_and_sanitized() {
    assert_eq!(skillstar_managed_key("abcd1234-xyz"), "skillstar_abcd1234");
    assert_eq!(skillstar_managed_key("AB!cd"), "skillstar_ab_cd");
    assert!(is_skillstar_managed_key("skillstar"));
    assert!(is_skillstar_managed_key("skillstar_abcd1234"));
    assert!(!is_skillstar_managed_key("skillstarx"));
    assert!(!is_skillstar_managed_key("other"));
}

#[test]
fn codex_binding_sync_does_not_create_a_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");

    let providers = vec![
        responses_capable("aaaa1111", "alpha"),
        responses_capable("bbbb2222", "beta"),
    ];
    let binding = AgentBinding {
        entries: vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        roles: Default::default(),
        active_index: 1,
        settings: None,
    };

    sync_codex_binding_inner(&binding, &providers, &path).unwrap();
    assert!(!path.exists());
}

#[test]
fn codex_binding_sync_leaves_user_and_stale_tables() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    let before = "model = \"old\"\n\
         [model_providers.mycustom]\nname = \"Mine\"\nbase_url = \"https://x\"\n\
         [model_providers.skillstar_dead0000]\nname = \"Stale\"\nbase_url = \"https://stale\"\n";
    std::fs::write(&path, before).unwrap();

    let providers = vec![responses_capable("aaaa1111", "alpha")];
    let binding = AgentBinding::single(entry("aaaa1111", "model-a"));
    sync_codex_binding_inner(&binding, &providers, &path).unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

#[test]
fn opencode_binding_sync_does_not_create_a_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("opencode.json");

    let providers = vec![flat("aaaa1111", "alpha"), flat("bbbb2222", "beta")];
    let binding = AgentBinding {
        entries: vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        roles: Default::default(),
        active_index: 0,
        settings: None,
    };

    sync_opencode_binding_inner(&binding, &providers, &path).unwrap();
    assert!(!path.exists());
}

#[test]
fn pi_binding_sync_leaves_seeded_files() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.json");
    let settings_path = tmp.path().join("settings.json");
    let models = r#"{ "providers": { "ollama": { "baseUrl": "http://localhost:11434/v1", "apiKey": "ollama" }, "skillstar_dead0000": { "baseUrl": "https://stale", "apiKey": "sk-dead" } } }"#;
    let settings = r#"{ "defaultThinkingLevel": "medium" }"#;
    std::fs::write(&models_path, models).unwrap();
    std::fs::write(&settings_path, settings).unwrap();

    let providers = vec![flat("aaaa1111", "alpha"), flat("bbbb2222", "beta")];
    let binding = AgentBinding {
        entries: vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        roles: Default::default(),
        active_index: 1,
        settings: None,
    };

    sync_pi_binding_inner(&binding, &providers, &models_path, &settings_path).unwrap();

    assert_eq!(std::fs::read_to_string(&models_path).unwrap(), models);
    assert_eq!(std::fs::read_to_string(&settings_path).unwrap(), settings);
}

#[test]
fn pi_unsync_removes_managed_blocks_and_managed_pointer_only() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.json");
    let settings_path = tmp.path().join("settings.json");

    std::fs::write(
        &models_path,
        r#"{"providers":{"skillstar_aaaa1111":{"apiKey":"sk-aaaa1111","baseUrl":"https://alpha.example.com/v1"},"mine":{"baseUrl":"https://mine"}}}"#,
    )
    .unwrap();
    std::fs::write(
        &settings_path,
        r#"{"defaultProvider":"skillstar_aaaa1111","defaultModel":"model-a"}"#,
    )
    .unwrap();

    unsync_pi_all_at(&models_path, &settings_path).unwrap();

    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&models_path).unwrap()).unwrap();
    let provider_map = after.get("providers").unwrap().as_object().unwrap();
    assert!(provider_map.contains_key("mine"));
    assert!(!provider_map.keys().any(|k| is_skillstar_managed_key(k)));

    // Managed pointer cleared from settings.json.
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert!(settings.get("defaultProvider").is_none());
    assert!(settings.get("defaultModel").is_none());
}

#[test]
fn pi_unsync_leaves_user_owned_default_pointer_alone() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.json");
    let settings_path = tmp.path().join("settings.json");
    std::fs::write(
        &settings_path,
        r#"{ "defaultProvider": "anthropic", "defaultModel": "claude-sonnet-4" }"#,
    )
    .unwrap();

    unsync_pi_all_at(&models_path, &settings_path).unwrap();

    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(
        settings.get("defaultProvider").unwrap().as_str().unwrap(),
        "anthropic"
    );
    assert_eq!(
        settings.get("defaultModel").unwrap().as_str().unwrap(),
        "claude-sonnet-4"
    );
}

#[test]
fn unsync_removes_all_managed_keys_only() {
    // Isolated temp paths (not the shared sandbox HOME) so this never races
    // other sync tests on ~/.codex/config.toml.
    let tmp = TempDir::new().unwrap();
    let codex_path = tmp.path().join("config.toml");
    let auth_path = tmp.path().join("auth.json");

    std::fs::write(
        &codex_path,
        "model_provider = \"skillstar_aaaa1111\"\n\
         model = \"model-a\"\n\n\
         [model_providers.skillstar_aaaa1111]\n\
         name = \"SkillStar\"\n\
         base_url = \"https://alpha.example.com/v1\"\n\n\
         [model_providers.skillstar_bbbb2222]\n\
         name = \"SkillStar\"\n\
         base_url = \"https://beta.example.com/v1\"\n\n\
         [model_providers.mine]\n\
         name = \"Mine\"\n",
    )
    .unwrap();
    std::fs::write(
        &auth_path,
        r#"{"OPENAI_API_KEY":"sk-secret","tokens":{"access_token":"keep"}}"#,
    )
    .unwrap();

    unsync_codex_all_at(&auth_path, &codex_path).unwrap();

    let after: toml::Table =
        toml::from_str(&std::fs::read_to_string(&codex_path).unwrap()).unwrap();
    assert!(after.get("model_provider").is_none());
    assert!(after.get("model").is_none());
    let mp_after = after.get("model_providers").unwrap().as_table().unwrap();
    assert!(mp_after.contains_key("mine"));
    assert!(!mp_after.keys().any(|k| is_skillstar_managed_key(k)));

    let auth: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&auth_path).unwrap()).unwrap();
    assert!(auth.get("OPENAI_API_KEY").is_none());
    assert_eq!(
        auth.pointer("/tokens/access_token")
            .and_then(|v| v.as_str()),
        Some("keep")
    );
}

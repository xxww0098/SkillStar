//! Unit tests (part 1): config load/save, runtime
//! readiness, URL builders, provider-ref validation, and TOML parsing.
//!
//! Split verbatim out of the inline `#[cfg(test)] mod tests` in `mod.rs`.

use super::*;

#[test]
fn load_config_returns_default_when_json_is_corrupted() {
    with_temp_data_root(|_dir| {
        let config_path = super::config_path();
        std::fs::create_dir_all(config_path.parent().expect("config dir"))
            .expect("create config dir");
        std::fs::write(&config_path, "{not-valid-json").expect("write corrupt json");
        super::invalidate_config_cache();
        let loaded = super::load_config();
        let defaults = super::AiConfig::default();
        assert_eq!(loaded.enabled, defaults.enabled);
        assert_eq!(loaded.model, defaults.model);
        assert_eq!(loaded.api_key, defaults.api_key);
    });
}

#[test]
fn save_and_load_config_async_roundtrip_keeps_plain_api_key() {
    with_temp_data_root(|_dir| {
        let rt = tokio::runtime::Runtime::new().expect("create runtime");
        rt.block_on(async {
            let cfg = super::AiConfig {
                enabled: true,
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: "test-secret-key".to_string(),
                model: "gpt-5.4".to_string(),
                ..Default::default()
            };

            super::save_config_async(&cfg)
                .await
                .expect("save config async should succeed");
            let loaded = super::load_config_async().await;

            assert!(loaded.enabled);
            assert_eq!(loaded.base_url, cfg.base_url);
            assert_eq!(loaded.api_key, cfg.api_key);
            assert_eq!(loaded.model, cfg.model);
        });
    });
}

#[test]
fn load_config_ignores_retired_translation_fields() {
    with_temp_data_root(|_dir| {
        let config_path = super::config_path();
        std::fs::create_dir_all(config_path.parent().expect("config dir"))
            .expect("create config dir");
        std::fs::write(
            &config_path,
            r#"{
              "enabled": true,
              "api_format": "openai",
              "base_url": "https://api.openai.com/v1",
              "api_key": "",
              "model": "gpt-5.4",
              "target_language": "ja",
              "short_text_priority": {"mode": "short"},
              "context_window_k": 128,
              "max_concurrent_requests": 4
            }"#,
        )
        .expect("write legacy json");
        super::invalidate_config_cache();
        let loaded = super::load_config();
        assert!(loaded.enabled);
        assert_eq!(loaded.model, "gpt-5.4");
        super::save_config(&loaded).expect("save should succeed");
        let written = std::fs::read_to_string(super::config_path()).expect("read saved");
        assert!(
            !written.contains("target_language"),
            "retired translation field must not be rewritten: {written}"
        );
        assert!(
            !written.contains("short_text_priority"),
            "retired translation field must not be rewritten: {written}"
        );
    });
}

#[test]
fn language_display_name_follows_ui_locales() {
    assert_eq!(language_display_name("zh-CN"), "Simplified Chinese");
    assert_eq!(language_display_name(""), "Simplified Chinese");
    assert_eq!(language_display_name("en"), "English");
    assert_eq!(language_display_name("en-US"), "English");
    assert_eq!(language_display_name("ja"), "ja");
}

#[test]
fn ai_runtime_ready_false_when_disabled() {
    let cfg = AiConfig {
        enabled: false,
        ..Default::default()
    };
    assert!(!super::ai_runtime_ready(&cfg));
}

#[test]
fn ai_runtime_ready_true_with_api_key() {
    let cfg = AiConfig {
        enabled: true,
        api_key: "sk-test".to_string(),
        ..Default::default()
    };
    assert!(super::ai_runtime_ready(&cfg));
}

#[test]
fn ai_runtime_ready_true_for_local_format_without_key() {
    let cfg = AiConfig {
        enabled: true,
        api_format: ApiFormat::Local,
        api_key: String::new(),
        base_url: "http://127.0.0.1:11434".to_string(),
        ..Default::default()
    };
    assert!(super::ai_runtime_ready(&cfg));
}

#[test]
fn build_openai_chat_url_normalizes_various_bases() {
    assert_eq!(
        super::build_openai_chat_url("https://api.openai.com/v1"),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(
        super::build_openai_chat_url("http://localhost:11434"),
        "http://localhost:11434/v1/chat/completions"
    );
    assert_eq!(
        super::build_openai_chat_url("http://host:1234/v1/chat/completions"),
        "http://host:1234/v1/chat/completions"
    );
    assert_eq!(
        super::build_openai_chat_url(""),
        "https://api.openai.com/v1/chat/completions"
    );
}

#[test]
fn build_anthropic_messages_url_normalizes_various_bases() {
    assert_eq!(
        super::build_anthropic_messages_url("https://api.anthropic.com"),
        "https://api.anthropic.com/v1/messages"
    );
    assert_eq!(
        super::build_anthropic_messages_url("https://proxy.example.com/v1"),
        "https://proxy.example.com/v1/messages"
    );
    assert_eq!(
        super::build_anthropic_messages_url("https://proxy.example.com/messages"),
        "https://proxy.example.com/messages"
    );
    assert_eq!(
        super::build_anthropic_messages_url(""),
        "https://api.anthropic.com/v1/messages"
    );
}

#[test]
fn resolve_provider_ref_parts_rejects_empty_provider_id() {
    let mut cfg = AiConfig::default();
    let result = super::resolve_provider_ref_parts(&mut cfg, "claude", "");
    assert!(result.is_err());
}

#[test]
fn resolve_provider_ref_parts_rejects_unsupported_app() {
    let mut cfg = AiConfig::default();
    let result = super::resolve_provider_ref_parts(&mut cfg, "gemini", "some-id");
    assert!(result.is_err());
}

#[test]
fn effective_api_key_returns_ollama_for_local_format() {
    let cfg = AiConfig {
        api_format: ApiFormat::Local,
        api_key: String::new(),
        ..Default::default()
    };
    assert_eq!(super::effective_api_key(&cfg), "ollama");
}

#[test]
fn effective_api_key_returns_actual_key_for_non_local() {
    let cfg = AiConfig {
        api_format: ApiFormat::Openai,
        api_key: "sk-test".to_string(),
        ..Default::default()
    };
    assert_eq!(super::effective_api_key(&cfg), "sk-test");
}

// ── TOML parsing helpers ─────────────────────────────────────────

#[test]
fn parse_toml_string_field_finds_top_level_field() {
    let toml = r#"model = "gpt-4o"
base_url = "https://api.example.com/v1"
"#;
    assert_eq!(
        super::parse_toml_string_field(toml, "model"),
        Some("gpt-4o".to_string())
    );
    assert_eq!(
        super::parse_toml_string_field(toml, "base_url"),
        Some("https://api.example.com/v1".to_string())
    );
}

#[test]
fn parse_toml_string_field_finds_field_inside_section() {
    let toml = r#"model_provider = "ccswitch"

[model_providers.ccswitch]
name = "Custom"
base_url = "https://example.com/v1"
wire_api = "responses"
"#;
    assert_eq!(
        super::parse_toml_string_field(toml, "base_url"),
        Some("https://example.com/v1".to_string())
    );
    assert_eq!(
        super::parse_toml_string_field(toml, "wire_api"),
        Some("responses".to_string())
    );
}

#[test]
fn parse_codex_active_provider_base_url_reads_nested_table() {
    let toml = r#"model_provider = "myprovider"
model = "gpt-4o"

[model_providers.myprovider]
name = "My Provider"
base_url = "https://myprovider.ai/v1"
wire_api = "chat"
"#;
    assert_eq!(
        super::parse_codex_active_provider_base_url(toml),
        Some("https://myprovider.ai/v1".to_string())
    );
}

#[test]
fn parse_codex_active_provider_base_url_returns_none_when_no_model_provider() {
    let toml = r#"model = "gpt-4o"
base_url = "https://api.openai.com/v1"
"#;
    assert_eq!(super::parse_codex_active_provider_base_url(toml), None);
}

#[test]
fn parse_codex_active_provider_base_url_ignores_wrong_section() {
    let toml = r#"model_provider = "myp"

[model_providers.other]
base_url = "https://wrong.example/v1"
"#;
    // The active provider is "myp" but only [model_providers.other] exists
    assert_eq!(super::parse_codex_active_provider_base_url(toml), None);
}

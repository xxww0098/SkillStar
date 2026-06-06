//! Unit tests (part 2): flat-store and legacy-store provider resolution
//! plus config-persistence behavior.
//!
//! Split verbatim out of the inline `#[cfg(test)] mod tests` in `mod.rs`.

use super::*;

// ── Flat store resolution ─────────────────────────────────────────

#[test]
fn resolve_from_flat_store_claude_uses_anthropic_endpoint() {
    with_temp_data_root(|_dir| {
        use skillstar_models::providers::{FlatProvidersStore, ProviderEntryFlat, write_flat_store, flat_store_path};

        let entry = ProviderEntryFlat {
            id: "test-uuid-claude".to_string(),
            name: "Test Claude".to_string(),
            base_url_openai: "https://openai.example.com/v1".to_string(),
            base_url_anthropic: "https://anthropic.example.com".to_string(),
            models_url: String::new(),
            api_key: "sk-ant-test".to_string(),
            models: vec!["claude-sonnet-4-6".to_string()],
            default_model: "claude-sonnet-4-6".to_string(),
            sort_index: 0,
            codex_wire_api: "responses".to_string(),
            codex_auth_mode: "api_key".to_string(),
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: None,
            meta: Some(serde_json::json!({
                "claude_main_model": "claude-sonnet-4-6",
                "claude_haiku_model": "claude-haiku-4-5-20251001",
                "claude_sonnet_model": "claude-sonnet-4-6",
                "claude_opus_model": "claude-opus-4-7",
            })),
        };

        let store = FlatProvidersStore {
            version: 2,
            providers: vec![entry],
            tool_activations: std::collections::HashMap::new(),
        };
        write_flat_store(&store, &flat_store_path()).expect("write flat store");

        let mut cfg = AiConfig::default();
        let label = super::resolve_from_flat_store(&mut cfg, "claude", "test-uuid-claude")
            .expect("resolve should succeed");

        assert_eq!(label, "Test Claude");
        assert_eq!(cfg.api_format, ApiFormat::Anthropic);
        assert_eq!(cfg.api_key, "sk-ant-test");
        assert_eq!(cfg.base_url, "https://anthropic.example.com");
        assert_eq!(cfg.model, "claude-sonnet-4-6");
        assert_eq!(cfg.claude_haiku_model, Some("claude-haiku-4-5-20251001".to_string()));
        assert_eq!(cfg.claude_sonnet_model, Some("claude-sonnet-4-6".to_string()));
        assert_eq!(cfg.claude_opus_model, Some("claude-opus-4-7".to_string()));
    });
}

#[test]
fn resolve_from_flat_store_claude_falls_back_to_openai_url_when_anthropic_empty() {
    with_temp_data_root(|_dir| {
        use skillstar_models::providers::{FlatProvidersStore, ProviderEntryFlat, write_flat_store, flat_store_path};

        let entry = ProviderEntryFlat {
            id: "test-uuid-relay".to_string(),
            name: "Relay".to_string(),
            base_url_openai: "https://relay.example.com/anthropic".to_string(),
            base_url_anthropic: "".to_string(),
            models_url: String::new(),
            api_key: "relay-key".to_string(),
            models: vec![],
            default_model: String::new(),
            sort_index: 0,
            codex_wire_api: "responses".to_string(),
            codex_auth_mode: "api_key".to_string(),
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: None,
            meta: None,
        };

        let store = FlatProvidersStore {
            version: 2,
            providers: vec![entry],
            tool_activations: std::collections::HashMap::new(),
        };
        write_flat_store(&store, &flat_store_path()).expect("write flat store");

        let mut cfg = AiConfig::default();
        super::resolve_from_flat_store(&mut cfg, "claude", "test-uuid-relay")
            .expect("resolve should succeed");

        assert_eq!(cfg.base_url, "https://relay.example.com/anthropic");
        assert_eq!(cfg.model, "claude-sonnet-4-20250514"); // hard default
    });
}

#[test]
fn resolve_from_flat_store_codex_uses_openai_endpoint() {
    with_temp_data_root(|_dir| {
        use skillstar_models::providers::{FlatProvidersStore, ProviderEntryFlat, write_flat_store, flat_store_path};

        let entry = ProviderEntryFlat {
            id: "test-uuid-codex".to_string(),
            name: "Custom Codex".to_string(),
            base_url_openai: "https://codex.example.com/v1".to_string(),
            base_url_anthropic: "https://should-be-ignored.example.com".to_string(),
            models_url: String::new(),
            api_key: "sk-openai-test".to_string(),
            models: vec!["gpt-4o".to_string()],
            default_model: "gpt-4o".to_string(),
            sort_index: 0,
            codex_wire_api: "responses".to_string(),
            codex_auth_mode: "api_key".to_string(),
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: None,
            meta: None,
        };

        let store = FlatProvidersStore {
            version: 2,
            providers: vec![entry],
            tool_activations: std::collections::HashMap::new(),
        };
        write_flat_store(&store, &flat_store_path()).expect("write flat store");

        let mut cfg = AiConfig::default();
        let label = super::resolve_from_flat_store(&mut cfg, "codex", "test-uuid-codex")
            .expect("resolve should succeed");

        assert_eq!(label, "Custom Codex");
        assert_eq!(cfg.api_format, ApiFormat::Openai);
        assert_eq!(cfg.api_key, "sk-openai-test");
        assert_eq!(cfg.base_url, "https://codex.example.com/v1");
        assert_eq!(cfg.model, "gpt-4o");
    });
}

#[test]
fn resolve_from_flat_store_fails_when_api_key_missing() {
    with_temp_data_root(|_dir| {
        use skillstar_models::providers::{FlatProvidersStore, ProviderEntryFlat, write_flat_store, flat_store_path};

        let entry = ProviderEntryFlat {
            id: "test-no-key".to_string(),
            name: "No Key".to_string(),
            base_url_openai: "https://api.example.com/v1".to_string(),
            base_url_anthropic: String::new(),
            models_url: String::new(),
            api_key: "".to_string(),
            models: vec![],
            default_model: "gpt-4o".to_string(),
            sort_index: 0,
            codex_wire_api: "responses".to_string(),
            codex_auth_mode: "api_key".to_string(),
            preset_id: None,
            icon_color: None,
            notes: None,
            created_at: None,
            meta: None,
        };

        let store = FlatProvidersStore {
            version: 2,
            providers: vec![entry],
            tool_activations: std::collections::HashMap::new(),
        };
        write_flat_store(&store, &flat_store_path()).expect("write flat store");

        let mut cfg = AiConfig::default();
        let result = super::resolve_from_flat_store(&mut cfg, "codex", "test-no-key");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing an API key"));
    });
}

// ── Legacy store: Claude model variant fields ─────────────────────

#[test]
fn resolve_from_legacy_store_reads_claude_model_variants() {
    with_temp_data_root(|_dir| {
        use skillstar_models::providers::{ProvidersStore, AppProviders, ProviderEntry, write_store};
        use std::collections::HashMap;

        let settings_config = serde_json::json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-legacy",
                "ANTHROPIC_BASE_URL": "https://legacy.example.com",
                "ANTHROPIC_MODEL": "claude-sonnet-4-6",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "claude-haiku-4-5-20251001",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-4-6",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "claude-opus-4-7",
            }
        });

        let entry = ProviderEntry {
            id: "legacy-claude".to_string(),
            name: "Legacy Claude".to_string(),
            category: "custom".to_string(),
            settings_config,
            preset_id: None,
            website_url: None,
            api_key_url: None,
            icon_color: None,
            notes: None,
            created_at: None,
            sort_index: None,
            meta: None,
        };

        let mut providers_map = HashMap::new();
        providers_map.insert("legacy-claude".to_string(), entry);

        let store = ProvidersStore {
            claude: AppProviders { providers: providers_map, current: None },
            codex: AppProviders::default(),
            opencode: AppProviders::default(),
            gemini: AppProviders::default(),
        };
        write_store(&store).expect("write legacy store");

        let mut cfg = AiConfig::default();
        let label = super::resolve_from_legacy_store(&mut cfg, "claude", "legacy-claude")
            .expect("resolve should succeed");

        assert_eq!(label, "Legacy Claude");
        assert_eq!(cfg.api_format, ApiFormat::Anthropic);
        assert_eq!(cfg.api_key, "sk-ant-legacy");
        assert_eq!(cfg.model, "claude-sonnet-4-6");
        assert_eq!(cfg.claude_haiku_model, Some("claude-haiku-4-5-20251001".to_string()));
        assert_eq!(cfg.claude_sonnet_model, Some("claude-sonnet-4-6".to_string()));
        assert_eq!(cfg.claude_opus_model, Some("claude-opus-4-7".to_string()));
    });
}

// ── Claude model variants not persisted ──────────────────────────

#[test]
fn claude_model_variants_are_not_written_to_disk() {
    with_temp_data_root(|_dir| {
        let cfg = AiConfig {
            enabled: true,
            api_key: "sk-test".to_string(),
            claude_haiku_model: Some("claude-haiku-4-5-20251001".to_string()),
            claude_sonnet_model: Some("claude-sonnet-4-6".to_string()),
            claude_opus_model: Some("claude-opus-4-7".to_string()),
            ..Default::default()
        };

        super::save_config(&cfg).expect("save should succeed");
        let loaded = super::load_config();

        assert_eq!(loaded.claude_haiku_model, None);
        assert_eq!(loaded.claude_sonnet_model, None);
        assert_eq!(loaded.claude_opus_model, None);
    });
}

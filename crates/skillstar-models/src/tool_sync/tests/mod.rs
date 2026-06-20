//! Unit + property tests for the tool_sync module.
//!
//! Shared sandbox-home guard and builder helpers live here; the actual
//! test cases are split across `part1`/`part2`/`part3` to keep each file small.

mod part1;
mod part2;
mod part3;

use super::*;
use crate::providers::{ModelMapping, ProviderSettings};
use proptest::prelude::*;
use tempfile::TempDir;

/// A throwaway HOME sandbox shared by every test that invokes a
/// home-resolving sync path (`resync_active_tools`, `sync_to_*`, …).
///
/// Initialised exactly once under `LazyLock`, whose synchronization sets
/// [`TOOL_SYNC_HOME_ENV`] before any test observes it — so the real
/// `~/.claude`, `~/.codex`, `~/.gemini`, … are NEVER touched by the suite.
/// Any future test that drives a real sync MUST call [`use_sandbox_home`]
/// first, or it will write to the developer's live tool configs.
static TOOL_SYNC_SANDBOX: std::sync::LazyLock<TempDir> = std::sync::LazyLock::new(|| {
    let dir = TempDir::new().expect("create tool-sync sandbox home");
    // SAFETY: runs exactly once under LazyLock's one-time synchronization,
    // establishing happens-before with every later read of the env var; no
    // concurrent `set_var` occurs because the value is set only here.
    unsafe { std::env::set_var(TOOL_SYNC_HOME_ENV, dir.path()) };
    dir
});

/// Force the sandbox HOME override into effect. Call at the top of any test
/// that exercises a home-resolving sync function.
fn use_sandbox_home() {
    let _ = TOOL_SYNC_SANDBOX.path();
}

fn make_test_settings() -> ProviderSettings {
    ProviderSettings {
        base_url: "https://api.example.com/v1".to_string(),
        api_key: "sk-test-key-12345".to_string(),
        models: vec![ModelMapping {
            source_model: "model-a".to_string(),
            target_model: "model-a".to_string(),
            enabled: true,
        }],
        timeout_ms: None,
        max_retries: None,
    }
}

fn make_test_provider() -> ProviderEntry {
    ProviderEntry {
        id: "test-provider".to_string(),
        name: "Test Provider".to_string(),
        category: "cloud".to_string(),
        settings_config: serde_json::to_value(make_test_settings()).unwrap(),
        preset_id: None,
        website_url: None,
        api_key_url: None,
        icon_color: None,
        notes: None,
        created_at: None,
        sort_index: None,
        meta: None,
    }
}

fn make_test_provider_flat() -> ProviderEntryFlat {
    ProviderEntryFlat {
        id: "test-uuid-1234".to_string(),
        name: "Test Provider".to_string(),
        base_url_openai: "https://api.example.com/v1".to_string(),
        base_url_anthropic: "https://api.example.com/anthropic".to_string(),
        models_url: "https://api.example.com/v1/models".to_string(),
        api_key: "sk-test-key-flat-12345".to_string(),
        models: vec!["model-a".to_string(), "model-b".to_string()],
        default_model: "model-a".to_string(),
        sort_index: 0,
        preset_id: Some("test-preset".to_string()),
        icon_color: Some("#FF0000".to_string()),
        notes: None,
        created_at: Some(1719000000000),
        meta: None,
        codex_wire_api: "responses".to_string(),
        codex_auth_mode: "api_key".to_string(),
    }
}

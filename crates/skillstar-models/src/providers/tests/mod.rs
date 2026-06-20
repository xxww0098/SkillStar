//! Provider unit tests (split from the original inline `mod tests`).
//! Shared helpers live here; the `#[test]` fns live in `part1`/`part2`/`part3`.

use super::*;
use proptest::prelude::*;
use tempfile::TempDir;

// -----------------------------------------------------------------------
// Flat store read/write tests
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// V1 store tests (existing)
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// Flat preset registry tests (v2)
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// Flat store CRUD tests (v2)
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// Property 14: Concurrent Write Serialization
//
// Spawn multiple concurrent create_provider calls, assert final store is
// consistent with no corruption.
//
// **Validates: Requirement 7.2**
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// Migration tests (v1 → v2)
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// Property-based tests
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// Property-based test strategies
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// Tool activation/deactivation tests (v2)
// -----------------------------------------------------------------------

/// Helper: create a temp directory with a store file path inside it.
fn setup_temp_store() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("model_providers.json");
    (tmp, path)
}

fn make_valid_settings() -> Value {
    serde_json::to_value(ProviderSettings {
        base_url: "https://api.example.com/v1".to_string(),
        api_key: "sk-test-key-12345".to_string(),
        models: vec![ModelMapping {
            source_model: "model-a".to_string(),
            target_model: "model-a".to_string(),
            enabled: true,
        }],
        timeout_ms: None,
        max_retries: None,
    })
    .unwrap()
}

fn make_valid_entry(id: &str, name: &str) -> ProviderEntry {
    ProviderEntry {
        id: id.to_string(),
        name: name.to_string(),
        category: "cloud".to_string(),
        settings_config: make_valid_settings(),
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

fn make_flat_entry(name: &str) -> ProviderEntryFlat {
    ProviderEntryFlat {
        id: String::new(), // Will be overwritten by create
        name: name.to_string(),
        base_url_openai: "https://api.example.com/v1".to_string(),
        base_url_anthropic: "https://api.example.com/anthropic".to_string(),
        models_url: "https://api.example.com/v1/models".to_string(),
        api_key: "sk-test-key".to_string(),
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
    }
}

fn arb_provider_name() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 _-]{1,64}"
}

fn arb_app_id() -> impl Strategy<Value = String> {
    prop_oneof![Just("claude".to_string()), Just("codex".to_string())]
}

fn arb_provider_count() -> impl Strategy<Value = usize> {
    1usize..=5
}

mod part1;
mod part2;
mod part3;

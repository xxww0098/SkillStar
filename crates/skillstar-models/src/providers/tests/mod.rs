//! Provider unit tests (split from the original inline `mod tests`).
//! Shared helpers live here; the `#[test]` fns live in `part1`/`part2`/`part3`.

use super::*;
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

/// A v4 provider row: chat + Anthropic endpoints, one literal key.
///
/// Each call mints a fresh id, because v4 lets the caller own the id and
/// `create_provider` therefore rejects a duplicate instead of silently
/// reassigning one — which is the behaviour that makes a stable slug possible.
fn make_provider(name: &str) -> Provider {
    let mut provider = Provider::new(uuid::Uuid::new_v4().to_string(), name);
    provider.endpoints = Endpoints {
        openai_chat: Some("https://api.example.com/v1".to_string()),
        openai_responses: None,
        anthropic_messages: Some("https://api.example.com/anthropic".to_string()),
        models_list: Some("https://api.example.com/v1/models".to_string()),
    };
    provider.credential = Credential::single_key("k1", "sk-test-key");
    provider.models = vec!["model-a".to_string()];
    provider.default_model = Some("model-a".to_string());
    provider
}

/// The same, but able to serve Codex.
fn make_responses_provider(name: &str) -> Provider {
    let mut provider = make_provider(name);
    provider.endpoints.openai_responses = Some("https://api.example.com/v1".to_string());
    provider.caps.responses_api = Tri::Yes;
    provider
}

mod part1;
mod part2;
mod part3;
mod part4;
mod part5;
mod store_v4;

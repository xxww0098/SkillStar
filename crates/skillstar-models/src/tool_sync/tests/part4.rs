//! Multi-provider binding writer tests (Codex + OpenCode).
//!
//! These drive the `*_inner` writers against isolated temp paths (not the
//! shared sandbox HOME) so they can assert on exact file contents without
//! racing other tests.

use super::*;
use crate::providers::{ProviderEntryFlat, ToolActivation, ToolBinding};

fn flat(id: &str, name: &str) -> ProviderEntryFlat {
    ProviderEntryFlat {
        id: id.to_string(),
        name: name.to_string(),
        base_url_openai: format!("https://{name}.example.com/v1"),
        base_url_anthropic: String::new(),
        models_url: String::new(),
        api_key: format!("sk-{id}"),
        models: vec!["model-a".to_string(), "model-b".to_string()],
        default_model: "model-a".to_string(),
        sort_index: 0,
        preset_id: None,
        icon_color: None,
        notes: None,
        created_at: None,
        meta: None,
        codex_wire_api: "chat".to_string(),
        codex_auth_mode: "third_party".to_string(),
    }
}

fn entry(provider_id: &str, model: &str) -> ToolActivation {
    ToolActivation {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        settings: None,
        last_sync_at: None,
    }
}

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
fn codex_binding_writes_one_table_per_provider_plus_pointer() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");

    let providers = vec![flat("aaaa1111", "alpha"), flat("bbbb2222", "beta")];
    let binding = ToolBinding {
        entries: vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        active_index: 1,
    };

    sync_codex_binding_inner(&binding, &providers, &path).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let table: toml::Table = toml::from_str(&content).unwrap();

    // Pointer follows active_index → beta.
    assert_eq!(
        table.get("model_provider").unwrap().as_str().unwrap(),
        "skillstar_bbbb2222"
    );
    assert_eq!(table.get("model").unwrap().as_str().unwrap(), "model-b");

    // Both managed tables exist.
    let mp = table.get("model_providers").unwrap().as_table().unwrap();
    assert!(mp.contains_key("skillstar_aaaa1111"));
    assert!(mp.contains_key("skillstar_bbbb2222"));
    assert_eq!(mp.len(), 2);
}

#[test]
fn codex_binding_preserves_user_provider_and_replaces_stale_managed() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");

    // Pre-existing config: a user-owned provider table + a stale managed one
    // from a previous single-provider sync.
    std::fs::write(
        &path,
        "model = \"old\"\n\
         [model_providers.mycustom]\nname = \"Mine\"\nbase_url = \"https://x\"\n\
         [model_providers.skillstar_dead0000]\nname = \"Stale\"\nbase_url = \"https://stale\"\n",
    )
    .unwrap();

    let providers = vec![flat("aaaa1111", "alpha")];
    let binding = ToolBinding::single(entry("aaaa1111", "model-a"));
    sync_codex_binding_inner(&binding, &providers, &path).unwrap();

    let table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mp = table.get("model_providers").unwrap().as_table().unwrap();
    // User table survives; stale managed table gone; new managed table present.
    assert!(mp.contains_key("mycustom"));
    assert!(!mp.contains_key("skillstar_dead0000"));
    assert!(mp.contains_key("skillstar_aaaa1111"));
}

#[test]
fn opencode_binding_writes_blocks_and_model_selector() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("opencode.json");

    let providers = vec![flat("aaaa1111", "alpha"), flat("bbbb2222", "beta")];
    let binding = ToolBinding {
        entries: vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        active_index: 0,
    };

    sync_opencode_binding_inner(&binding, &providers, &path).unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let provider_map = json.get("provider").unwrap().as_object().unwrap();
    assert!(provider_map.contains_key("skillstar_aaaa1111"));
    assert!(provider_map.contains_key("skillstar_bbbb2222"));
    // Active (index 0 → alpha) drives the top-level selector.
    assert_eq!(
        json.get("model").unwrap().as_str().unwrap(),
        "skillstar_aaaa1111/model-a"
    );
}

#[test]
fn unsync_removes_all_managed_keys_only() {
    // Isolated temp paths (not the shared sandbox HOME) so this never races
    // other sync tests on ~/.codex/config.toml.
    let tmp = TempDir::new().unwrap();
    let codex_path = tmp.path().join("config.toml");
    let auth_path = tmp.path().join("auth.json");

    let providers = vec![flat("aaaa1111", "alpha"), flat("bbbb2222", "beta")];
    let binding = ToolBinding {
        entries: vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        active_index: 0,
    };
    sync_codex_binding_inner(&binding, &providers, &codex_path).unwrap();

    // Inject a user-owned table that must survive unsync.
    let mut table: toml::Table = toml::from_str(&std::fs::read_to_string(&codex_path).unwrap()).unwrap();
    let mp = table.get_mut("model_providers").unwrap().as_table_mut().unwrap();
    mp.insert("mine".to_string(), toml::Value::Table(toml::Table::new()));
    std::fs::write(&codex_path, toml::to_string_pretty(&table).unwrap()).unwrap();

    unsync_codex_all_at(&auth_path, &codex_path).unwrap();

    let after: toml::Table = toml::from_str(&std::fs::read_to_string(&codex_path).unwrap()).unwrap();
    assert!(after.get("model_provider").is_none());
    let mp_after = after.get("model_providers").unwrap().as_table().unwrap();
    assert!(mp_after.contains_key("mine"));
    assert!(!mp_after.keys().any(|k| is_skillstar_managed_key(k)));
}

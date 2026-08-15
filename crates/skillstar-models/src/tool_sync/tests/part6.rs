//! What the Codex fix changed, and what the migration repairs on disk.
//!
//! The sibling of `golden.rs`: that file asserts the writers whose output must
//! not move, this one asserts the one whose output must.

use super::*;
use crate::providers::migrate::{DropReason, MigrationReport};
use crate::providers::{
    AgentBinding, Credential, FlatProvidersStore, ProviderEntryFlat, Tri, ToolActivation,
    ToolBinding, migrate::migrate_v3_to_v4,
};

fn codex_binding(entries: Vec<BindingEntry>) -> AgentBinding {
    AgentBinding {
        entries,
        active_index: 0,
        roles: Default::default(),
        settings: None,
    }
}

// ---------------------------------------------------------------------------
// The writer refuses hosts Codex cannot talk to
// ---------------------------------------------------------------------------

#[test]
fn a_chat_only_host_is_never_written_into_codex_config() {
    let _home = use_sandbox_home();
    let chat_only = flat("aaaa1111", "relay");
    let capable = responses_capable("bbbb2222", "openai");
    let binding = codex_binding(vec![
        entry(&capable.id, "gpt-5.4"),
        entry(&chat_only.id, "model-a"),
    ]);

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    sync_codex_binding_inner(&binding, &[capable.clone(), chat_only.clone()], &path).unwrap();

    let table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let providers = table["model_providers"].as_table().unwrap();
    assert!(
        providers.contains_key(&skillstar_managed_key(&capable.id)),
        "the host that speaks /v1/responses must still be written"
    );
    assert!(
        !providers.contains_key(&skillstar_managed_key(&chat_only.id)),
        "a chat-only host must be skipped, not written with wire_api = \"chat\""
    );
}

#[test]
fn every_table_codex_writes_says_responses() {
    let _home = use_sandbox_home();
    let capable = responses_capable("bbbb2222", "openai");
    let binding = codex_binding(vec![entry(&capable.id, "gpt-5.4")]);

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    sync_codex_binding_inner(&binding, &[capable], &path).unwrap();

    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains(r#"wire_api = "responses""#), "{written}");
    assert!(
        !written.contains(r#""chat""#),
        "the value that stops Codex booting must be unreachable: {written}"
    );
}

#[test]
fn codex_points_at_the_responses_endpoint_not_the_chat_one() {
    let _home = use_sandbox_home();
    let mut provider = flat("bbbb2222", "split");
    provider.endpoints.openai_chat = Some("https://split.example.com/v1".to_string());
    provider.endpoints.openai_responses = Some("https://split.example.com/responses".to_string());
    provider.caps.responses_api = Tri::Yes;

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    sync_codex_binding_inner(
        &codex_binding(vec![entry(&provider.id, "m")]),
        &[provider.clone()],
        &path,
    )
    .unwrap();

    let table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let block = table["model_providers"].as_table().unwrap()
        [&skillstar_managed_key(&provider.id)]
        .as_table()
        .unwrap();
    assert_eq!(
        block["base_url"].as_str().unwrap(),
        "https://split.example.com/responses",
        "Codex only calls /v1/responses; a chat base URL parses and then fails every request"
    );
}

#[test]
fn a_probe_that_disproved_responses_support_withdraws_the_host() {
    let _home = use_sandbox_home();
    let mut provider = responses_capable("bbbb2222", "openai");
    // The endpoint is still configured, but a probe came back negative.
    provider.caps.responses_api = Tri::No;

    assert!(
        !codex_can_serve(&provider),
        "an explicit No must override a configured endpoint"
    );
}

#[test]
fn an_unprobed_host_with_an_endpoint_is_still_bindable() {
    let mut provider = flat("bbbb2222", "relay");
    provider.endpoints.openai_responses = Some("https://relay.example.com/v1".to_string());
    provider.caps.responses_api = Tri::Unknown;

    assert!(
        codex_can_serve(&provider),
        "migration writes Unknown for every row; treating it as a denial \
         would unbind everyone on upgrade (R-2)"
    );
}

// ---------------------------------------------------------------------------
// Removing one stale table without touching the others
// ---------------------------------------------------------------------------

#[test]
fn unsyncing_one_entry_leaves_the_other_managed_tables_alone() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
model_provider = "skillstar_keepme12"
model = "model-a"

[model_providers.skillstar_keepme12]
name = "SkillStar"
base_url = "https://keep.example.com/v1"
wire_api = "responses"
requires_openai_auth = false

[model_providers.skillstar_dropme12]
name = "SkillStar"
base_url = "https://drop.example.com/v1"
wire_api = "chat"
requires_openai_auth = false

[model_providers.mine]
name = "Mine"
base_url = "https://mine.example.com/v1"
"#,
    )
    .unwrap();

    unsync_codex_entry_at("dropme12-xxxx", &path).unwrap();

    let table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let providers = table["model_providers"].as_table().unwrap();
    assert!(!providers.contains_key("skillstar_dropme12"));
    assert!(
        providers.contains_key("skillstar_keepme12"),
        "fixing one broken binding must not throw away a working one"
    );
    assert!(providers.contains_key("mine"), "user tables are untouchable");
    assert_eq!(
        table["model_provider"].as_str().unwrap(),
        "skillstar_keepme12",
        "the pointer named a different provider and must survive"
    );
}

#[test]
fn dropping_the_pointed_at_entry_also_drops_the_pointer() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
model_provider = "skillstar_dropme12"
model = "model-a"

[model_providers.skillstar_dropme12]
name = "SkillStar"
base_url = "https://drop.example.com/v1"
wire_api = "chat"
requires_openai_auth = false
"#,
    )
    .unwrap();

    unsync_codex_entry_at("dropme12-xxxx", &path).unwrap();

    let table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        table.get("model_provider").is_none(),
        "a pointer to a table that no longer exists is the same class of breakage"
    );
    assert!(table.get("model").is_none());
    assert!(table.get("model_providers").is_none());
}

// ---------------------------------------------------------------------------
// The §3.3 repair, end to end
// ---------------------------------------------------------------------------

fn v3_relay(id: &str, host: &str) -> ProviderEntryFlat {
    ProviderEntryFlat {
        id: id.to_string(),
        name: format!("{host} relay"),
        base_url_openai: format!("https://{host}/v1"),
        base_url_anthropic: String::new(),
        models_url: String::new(),
        api_key: format!("sk-{id}"),
        models: vec!["model-a".to_string()],
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

#[test]
fn the_repair_drops_unwritable_codex_entries_and_explains_itself() {
    let _home = use_sandbox_home();

    let chat_only = v3_relay("aaaa1111-x", "relay.example.com");
    let mut openai = v3_relay("bbbb2222-y", "api.openai.com");
    openai.base_url_openai = "https://api.openai.com/v1".to_string();

    let mut binding = ToolBinding::default();
    binding.entries = vec![
        ToolActivation {
            provider_id: chat_only.id.clone(),
            model: "model-a".to_string(),
            settings: None,
            last_sync_at: None,
        },
        ToolActivation {
            provider_id: openai.id.clone(),
            model: "gpt-5.4".to_string(),
            settings: None,
            last_sync_at: None,
        },
    ];
    binding.active_index = 1;
    let mut tool_activations = std::collections::HashMap::new();
    tool_activations.insert("codex".to_string(), binding);

    let outcome = migrate_v3_to_v4(
        FlatProvidersStore {
            version: 3,
            providers: vec![chat_only.clone(), openai.clone()],
            tool_activations,
        },
        &crate::providers::get_all_presets_flat(),
    );
    let mut store = outcome.store;
    let mut report = MigrationReport::default();

    let repair = repair_agent_configs(&mut store, &mut report);

    let codex = &store.bindings["codex"];
    assert_eq!(
        codex.entries.len(),
        1,
        "the chat-only entry must be gone from the store, not just from the file"
    );
    assert_eq!(codex.entries[0].provider_id, openai.id);
    assert_eq!(
        codex.active_index, 0,
        "the pointer must re-aim at the surviving entry, not keep a stale index"
    );

    assert_eq!(report.codex_dropped.len(), 1);
    let dropped = &report.codex_dropped[0];
    assert_eq!(dropped.provider_id, chat_only.id);
    assert_eq!(dropped.provider_name, chat_only.name);
    assert_eq!(dropped.model, "model-a");
    assert_eq!(dropped.reason, DropReason::CodexRequiresResponsesApi);
    assert!(
        report.needs_user_attention(),
        "losing a binding the user made is worth a modal, not a silent fix"
    );
    assert!(repair.resynced.contains(&"codex".to_string()));
}

#[test]
fn the_repair_clears_codex_entirely_when_no_entry_survives() {
    let _home = use_sandbox_home();

    // Pre-existing damage: the exact file shape v3 used to produce.
    let config = resolve_codex_config_path().unwrap();
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        &config,
        "model_provider = \"skillstar_aaaa1111\"\nmodel = \"model-a\"\n\n\
         [model_providers.skillstar_aaaa1111]\nname = \"SkillStar\"\n\
         base_url = \"https://relay.example.com/v1\"\nwire_api = \"chat\"\n\
         requires_openai_auth = false\n",
    )
    .unwrap();

    let chat_only = v3_relay("aaaa1111-x", "relay.example.com");
    let mut binding = ToolBinding::default();
    binding.entries = vec![ToolActivation {
        provider_id: chat_only.id.clone(),
        model: "model-a".to_string(),
        settings: None,
        last_sync_at: None,
    }];
    let mut tool_activations = std::collections::HashMap::new();
    tool_activations.insert("codex".to_string(), binding);

    let outcome = migrate_v3_to_v4(
        FlatProvidersStore {
            version: 3,
            providers: vec![chat_only],
            tool_activations,
        },
        &crate::providers::get_all_presets_flat(),
    );
    let mut store = outcome.store;
    let mut report = MigrationReport::default();

    repair_agent_configs(&mut store, &mut report);

    let written = std::fs::read_to_string(&config).unwrap();
    assert!(
        !written.contains("chat"),
        "the whole point: Codex must be able to parse its config again.\n{written}"
    );
    assert!(store.bindings["codex"].is_empty());
    assert_eq!(report.codex_dropped.len(), 1);
}

#[test]
fn a_native_login_row_survives_the_repair_untouched() {
    let _home = use_sandbox_home();

    let mut official = v3_relay(crate::providers::CODEX_OFFICIAL_ID, "unused");
    official.base_url_openai = String::new();
    official.api_key = String::new();
    official.codex_auth_mode = "oauth".to_string();

    let mut binding = ToolBinding::default();
    binding.entries = vec![ToolActivation {
        provider_id: official.id.clone(),
        model: String::new(),
        settings: None,
        last_sync_at: None,
    }];
    let mut tool_activations = std::collections::HashMap::new();
    tool_activations.insert("codex".to_string(), binding);

    let outcome = migrate_v3_to_v4(
        FlatProvidersStore {
            version: 3,
            providers: vec![official.clone()],
            tool_activations,
        },
        &crate::providers::get_all_presets_flat(),
    );
    let mut store = outcome.store;
    assert!(matches!(
        store.provider(&official.id).unwrap().credential,
        Credential::ExternalCli { .. }
    ));

    let mut report = MigrationReport::default();
    repair_agent_configs(&mut store, &mut report);

    assert!(
        report.codex_dropped.is_empty(),
        "an empty endpoint on a native-login row is the point of it, not a defect"
    );
    assert_eq!(store.bindings["codex"].entries.len(), 1);
}

#[test]
fn the_repair_removes_the_claude_desktop_marker_and_its_binding() {
    let _home = use_sandbox_home();

    let marker = resolve_claude_desktop_binding_path().unwrap();
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "{\"provider_name\":\"gone\"}").unwrap();

    let mut store = crate::providers::ProvidersStoreV4::default();
    store
        .bindings
        .insert("claude-desktop".to_string(), codex_binding(vec![]));
    let mut report = MigrationReport::default();

    repair_agent_configs(&mut store, &mut report);

    assert!(!marker.exists(), "Claude Desktop is planned, not implemented");
    assert!(!store.bindings.contains_key("claude-desktop"));
}

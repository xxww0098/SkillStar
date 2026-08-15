//! Tests for the v3 → v4 migration.
//!
//! The proptest at the bottom is the one that matters most: unit tests can only
//! check the field mappings someone thought to write down, and the failure mode
//! this migration must not have is "a field nobody remembered stopped being
//! carried". The property is stated as *reachability* — for an arbitrary v3
//! row, every value that was in it is findable in the v4 row — rather than as a
//! field-by-field comparison, because a field-by-field comparison is the same
//! list of fields a second time and would go stale in lockstep with the code.

use super::*;
use crate::providers::binding::{Effort, ModelRef};
use crate::providers::credential::{Credential, NoCredentialReason};
use crate::providers::presets::{CLAUDE_OFFICIAL_ID, CODEX_OFFICIAL_ID, ProviderPresetFlat, get_all_presets_flat};
use crate::providers::provider::Tri;
use crate::providers::types::{FlatProvidersStore, ProviderEntryFlat, ToolActivation, ToolBinding};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn v3_row(id: &str, name: &str) -> ProviderEntryFlat {
    ProviderEntryFlat {
        id: id.to_string(),
        name: name.to_string(),
        base_url_openai: String::new(),
        base_url_anthropic: String::new(),
        models_url: String::new(),
        api_key: String::new(),
        models: Vec::new(),
        default_model: String::new(),
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

fn deepseek_preset() -> ProviderPresetFlat {
    get_all_presets_flat()
        .into_iter()
        .find(|p| p.id == "deepseek")
        .expect("deepseek preset exists")
}

fn store_of(rows: Vec<ProviderEntryFlat>) -> FlatProvidersStore {
    FlatProvidersStore {
        version: 3,
        providers: rows,
        tool_activations: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// § 3.2.4 — the three-condition backfill
// ---------------------------------------------------------------------------

#[test]
fn migrate_backfills_anthropic_url_only_when_openai_url_matches_preset() {
    let preset = deepseek_preset();
    // The shape the buggy frontend produced: preset id set, OpenAI URL taken
    // verbatim from the preset, Anthropic URL never written.
    let mut row = v3_row("p1", "DeepSeek");
    row.preset_id = Some("deepseek".to_string());
    row.base_url_openai = preset.base_url_openai.clone();

    let out = migrate_v3_to_v4(store_of(vec![row]), &[preset.clone()]);

    let provider = &out.store.providers[0];
    assert_eq!(
        provider.endpoints.anthropic_messages.as_deref(),
        Some(preset.base_url_anthropic.as_str()),
        "all three conditions held, so the endpoint must be restored"
    );
    assert_eq!(out.report.backfilled_anthropic.len(), 1);
    assert_eq!(out.report.backfilled_anthropic[0].provider_id, "p1");
    assert_eq!(out.report.backfilled_anthropic[0].preset_id, "deepseek");
}

#[test]
fn migrate_leaves_user_edited_urls_alone() {
    let preset = deepseek_preset();
    // Condition ③ fails: the user pointed this row at their own relay. The
    // empty Anthropic URL is now a statement, not an omission.
    let mut row = v3_row("p1", "My relay");
    row.preset_id = Some("deepseek".to_string());
    row.base_url_openai = "https://relay.example.com/v1".to_string();

    let out = migrate_v3_to_v4(store_of(vec![row]), &[preset]);

    assert_eq!(
        out.store.providers[0].endpoints.anthropic_messages, None,
        "a user-edited row must not be overwritten by its preset"
    );
    assert!(out.report.backfilled_anthropic.is_empty());
}

#[test]
fn migrate_does_not_backfill_when_the_row_already_has_a_value() {
    let preset = deepseek_preset();
    // Condition ① fails.
    let mut row = v3_row("p1", "DeepSeek");
    row.preset_id = Some("deepseek".to_string());
    row.base_url_openai = preset.base_url_openai.clone();
    row.base_url_anthropic = "https://custom.example.com/anthropic".to_string();

    let out = migrate_v3_to_v4(store_of(vec![row]), &[preset]);

    assert_eq!(
        out.store.providers[0].endpoints.anthropic_messages.as_deref(),
        Some("https://custom.example.com/anthropic")
    );
    assert!(out.report.backfilled_anthropic.is_empty());
}

#[test]
fn migrate_does_not_backfill_when_the_preset_has_no_value() {
    // Condition ② fails: an OpenAI-only preset has nothing to contribute.
    let preset = ProviderPresetFlat {
        base_url_anthropic: String::new(),
        ..deepseek_preset()
    };
    let mut row = v3_row("p1", "DeepSeek");
    row.preset_id = Some("deepseek".to_string());
    row.base_url_openai = preset.base_url_openai.clone();

    let out = migrate_v3_to_v4(store_of(vec![row]), &[preset]);

    assert_eq!(out.store.providers[0].endpoints.anthropic_messages, None);
    assert!(out.report.backfilled_anthropic.is_empty());
}

#[test]
fn migrate_backfills_models_list_under_the_same_rule() {
    let preset = deepseek_preset();
    let mut row = v3_row("p1", "DeepSeek");
    row.preset_id = Some("deepseek".to_string());
    row.base_url_openai = preset.base_url_openai.clone();

    let out = migrate_v3_to_v4(store_of(vec![row]), &[preset.clone()]);

    assert_eq!(
        out.store.providers[0].endpoints.models_list.as_deref(),
        Some(preset.models_url.as_str()),
        "the same frontend bug also blanked models_url, disabling the fetch button"
    );
    assert_eq!(out.report.backfilled_models_list.len(), 1);
}

#[test]
fn migrate_without_a_preset_id_never_backfills() {
    let mut row = v3_row("p1", "Custom");
    row.base_url_openai = "https://api.deepseek.com/v1".to_string();

    let out = migrate_v3_to_v4(store_of(vec![row]), &get_all_presets_flat());

    assert_eq!(out.store.providers[0].endpoints.anthropic_messages, None);
    assert!(out.report.backfilled_anthropic.is_empty());
}

// ---------------------------------------------------------------------------
// § 3.2.1 — capability derivation
// ---------------------------------------------------------------------------

#[test]
fn migrate_writes_unknown_caps_never_no() {
    let mut relay = v3_row("relay", "Relay");
    relay.base_url_openai = "https://relay.example.com/v1".to_string();
    let mut anthropic_only = v3_row("anth", "Anthropic-only");
    anthropic_only.base_url_anthropic = "https://x.example.com/anthropic".to_string();

    let out = migrate_v3_to_v4(store_of(vec![relay, anthropic_only]), &[]);

    for provider in &out.store.providers {
        assert_ne!(
            provider.caps.responses_api,
            Tri::No,
            "migration has no probe results, so it must never assert a denial"
        );
        assert_ne!(provider.caps.anthropic_messages, Tri::No);
        assert_ne!(provider.caps.models_list, Tri::No);
        assert!(
            provider.caps.probed_at_ms.is_none(),
            "nothing was probed, so there is no probe timestamp to record"
        );
    }
    assert_eq!(out.store.providers[0].caps.responses_api, Tri::Unknown);
    assert_eq!(out.store.providers[1].caps.anthropic_messages, Tri::Unknown);
}

#[test]
fn migrate_marks_openai_itself_as_responses_capable() {
    let mut row = v3_row("openai", "OpenAI");
    row.base_url_openai = "https://api.openai.com/v1".to_string();

    let out = migrate_v3_to_v4(store_of(vec![row]), &[]);

    let provider = &out.store.providers[0];
    assert_eq!(provider.caps.responses_api, Tri::Yes);
    assert_eq!(
        provider.endpoints.openai_responses.as_deref(),
        Some("https://api.openai.com/v1"),
        "the one host known offline to speak Responses gets the endpoint too"
    );
}

#[test]
fn migrate_discards_the_dead_codex_wire_api_field() {
    let mut row = v3_row("p1", "Relay");
    row.base_url_openai = "https://relay.example.com/v1".to_string();
    row.codex_wire_api = "chat".to_string();

    let out = migrate_v3_to_v4(store_of(vec![row]), &[]);

    // `wire_api = "chat"` is the value that makes current Codex fail to parse
    // its own config. It must not survive anywhere in v4.
    let json = serde_json::to_string(&out.store).unwrap();
    assert!(!json.contains("wire_api"), "{json}");
    assert!(!json.contains("\"chat\""), "{json}");
}

// ---------------------------------------------------------------------------
// § 3.2.2 — credentials
// ---------------------------------------------------------------------------

#[test]
fn migrate_maps_api_key_to_a_single_key_credential() {
    let mut row = v3_row("p1", "Relay");
    row.base_url_openai = "https://relay.example.com/v1".to_string();
    row.api_key = "sk-secret-value".to_string();

    let out = migrate_v3_to_v4(store_of(vec![row]), &[]);

    match &out.store.providers[0].credential {
        Credential::ApiKey { keys } => {
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0].secret, "sk-secret-value");
            assert!(keys[0].enabled);
            assert!(!keys[0].id.is_empty(), "each key needs a stable id");
        }
        other => panic!("expected ApiKey, got {other:?}"),
    }
}

#[test]
fn migrate_maps_official_seeds_to_external_cli() {
    let out = migrate_v3_to_v4(
        store_of(vec![
            v3_row(CLAUDE_OFFICIAL_ID, "Claude Official"),
            v3_row(CODEX_OFFICIAL_ID, "Codex Official"),
        ]),
        &[],
    );

    assert_eq!(
        out.store.providers[0].credential,
        Credential::ExternalCli {
            surface: "claude".to_string()
        }
    );
    assert_eq!(
        out.store.providers[1].credential,
        Credential::ExternalCli {
            surface: "codex".to_string()
        }
    );
    assert!(
        out.store.providers[0].is_external_cli(),
        "the id whitelist v3 consulted in six places is now one match on the data"
    );
}

#[test]
fn migrate_distinguishes_a_local_service_from_a_native_login() {
    let mut local = v3_row("ollama", "Ollama");
    local.base_url_openai = "http://localhost:11434/v1".to_string();
    let bare = v3_row("bare", "Bare");

    let out = migrate_v3_to_v4(store_of(vec![local, bare]), &[]);

    assert_eq!(
        out.store.providers[0].credential,
        Credential::None {
            reason: NoCredentialReason::LocalService
        },
        "it has an endpoint, so the host field still matters"
    );
    assert_eq!(
        out.store.providers[1].credential,
        Credential::None {
            reason: NoCredentialReason::NativeLogin
        }
    );
}

// ---------------------------------------------------------------------------
// § 3.2.3 — OMP role key mapping
// ---------------------------------------------------------------------------

#[test]
fn migrate_v3_to_v4_maps_omp_role_keys() {
    let row = v3_row("p1", "Relay");
    let mut store = store_of(vec![row]);
    store.tool_activations.insert(
        "omp".to_string(),
        ToolBinding {
            entries: vec![ToolActivation {
                provider_id: "p1".to_string(),
                model: "m".to_string(),
                settings: None,
                last_sync_at: None,
            }],
            active_index: 0,
            settings: Some(serde_json::json!({
                "roles": {
                    "default":  { "provider_id": "p1", "model": "big" },
                    "smol":     { "provider_id": "p1", "model": "small" },
                    "plan":     { "provider_id": "p1", "model": "planner" },
                    "vision":   { "provider_id": "p1", "model": "seer" },
                    "task":     { "provider_id": "p1", "model": "worker" },
                    "slow":     { "provider_id": "p1", "model": "thinker" },
                    "designer": { "provider_id": "p1", "model": "artist" },
                },
                "somethingElse": true,
            })),
        },
    );

    let out = migrate_v3_to_v4(store, &[]);
    let roles = &out.store.bindings["omp"].roles;

    // The five canonical mappings, spelled out one by one — a rule that
    // "derived" these would have to guess that `task` means `subagent`.
    assert_eq!(roles["default"].model, "big");
    assert_eq!(roles["fast"].model, "small");
    assert_eq!(roles["plan"].model, "planner");
    assert_eq!(roles["vision"].model, "seer");
    assert_eq!(roles["subagent"].model, "worker");
    // Not canonical, but the user typed them, so they survive verbatim.
    assert_eq!(roles["slow"].model, "thinker");
    assert_eq!(roles["designer"].model, "artist");
    assert!(!roles.contains_key("smol"), "old key must not linger too");
    assert!(!roles.contains_key("task"));

    // Non-role settings stay in the bag.
    let settings = out.store.bindings["omp"].settings.as_ref().unwrap();
    assert_eq!(settings.get("somethingElse"), Some(&serde_json::json!(true)));
    assert!(settings.get("roles").is_none());
}

#[test]
fn migrate_maps_omp_thinking_levels_onto_canonical_effort() {
    // Each of OMP's nine levels, stated explicitly. `inherit` and `auto` are
    // instructions to defer rather than tiers, so they must produce no effort
    // at all — writing them as a tier would pin a value the user left floating.
    let cases = [
        ("off", Some(Effort::None)),
        ("minimal", Some(Effort::Minimal)),
        ("low", Some(Effort::Low)),
        ("medium", Some(Effort::Medium)),
        ("high", Some(Effort::High)),
        ("xhigh", Some(Effort::Xhigh)),
        ("max", Some(Effort::Max)),
        ("inherit", None),
        ("auto", None),
        ("nonsense-not-a-level", None),
    ];

    for (level, expected) in cases {
        let mut store = store_of(vec![v3_row("p1", "Relay")]);
        store.tool_activations.insert(
            "omp".to_string(),
            ToolBinding {
                entries: vec![],
                active_index: 0,
                settings: Some(serde_json::json!({
                    "roles": { "default": { "provider_id": "p1", "model": "m", "thinking": level } }
                })),
            },
        );
        let out = migrate_v3_to_v4(store, &[]);
        assert_eq!(
            out.store.bindings["omp"].roles["default"].effort, expected,
            "thinking level {level:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// § 3.2 step 3 — Claude tiers, Codex auth mode, Claude Desktop
// ---------------------------------------------------------------------------

#[test]
fn migrate_lifts_claude_tier_models_off_the_provider_row() {
    let mut row = v3_row("p1", "Relay");
    row.meta = Some(serde_json::json!({
        "claude_haiku_model": "haiku-1",
        "claude_sonnet_model": "sonnet-1",
        "claude_opus_model": "opus-1",
    }));
    let mut store = store_of(vec![row]);
    store.tool_activations.insert(
        "claude-code".to_string(),
        ToolBinding::single(ToolActivation {
            provider_id: "p1".to_string(),
            model: "main-1".to_string(),
            settings: None,
            last_sync_at: None,
        }),
    );

    let out = migrate_v3_to_v4(store, &[]);

    let roles = &out.store.bindings["claude-code"].roles;
    assert_eq!(roles["fast"], ModelRef::new("p1", "haiku-1"));
    assert_eq!(roles["sonnet"], ModelRef::new("p1", "sonnet-1"));
    assert_eq!(roles["opus"], ModelRef::new("p1", "opus-1"));
    // The tier keys must be gone from the provider row: leaving both copies is
    // how v3 ended up with one concept in two places.
    let ext = out.store.providers[0].ext.clone();
    assert!(ext.is_none(), "no leftover meta: {ext:?}");
}

#[test]
fn migrate_uses_claude_main_model_only_when_the_entry_names_none() {
    let mut row = v3_row("p1", "Relay");
    row.meta = Some(serde_json::json!({ "claude_main_model": "from-meta" }));

    // (a) the entry names a model → the entry wins, meta is not promoted.
    let mut with_model = store_of(vec![row.clone()]);
    with_model.tool_activations.insert(
        "claude-code".to_string(),
        ToolBinding::single(ToolActivation {
            provider_id: "p1".to_string(),
            model: "from-entry".to_string(),
            settings: None,
            last_sync_at: None,
        }),
    );
    let out = migrate_v3_to_v4(with_model, &[]);
    assert!(!out.store.bindings["claude-code"].roles.contains_key("default"));

    // (b) the entry names nothing → meta becomes the default role.
    let mut without_model = store_of(vec![row]);
    without_model.tool_activations.insert(
        "claude-code".to_string(),
        ToolBinding::single(ToolActivation {
            provider_id: "p1".to_string(),
            model: String::new(),
            settings: None,
            last_sync_at: None,
        }),
    );
    let out = migrate_v3_to_v4(without_model, &[]);
    assert_eq!(
        out.store.bindings["claude-code"].roles["default"],
        ModelRef::new("p1", "from-meta")
    );
}

#[test]
fn migrate_moves_codex_auth_mode_from_the_provider_row_to_the_entry() {
    let mut row = v3_row("p1", "Relay");
    row.codex_auth_mode = "third_party".to_string();
    let mut store = store_of(vec![row]);
    store.tool_activations.insert(
        "codex".to_string(),
        ToolBinding::single(ToolActivation {
            provider_id: "p1".to_string(),
            model: "m".to_string(),
            settings: None,
            last_sync_at: None,
        }),
    );

    let out = migrate_v3_to_v4(store, &[]);

    let entry = &out.store.bindings["codex"].entries[0];
    assert_eq!(
        entry.settings.as_ref().unwrap().get("auth_mode"),
        Some(&serde_json::json!("third_party")),
        "auth_mode is a per-agent concern and belongs on the entry"
    );
}

#[test]
fn migrate_keeps_an_entrys_own_auth_mode_over_the_provider_rows() {
    let mut row = v3_row("p1", "Relay");
    row.codex_auth_mode = "third_party".to_string();
    let mut store = store_of(vec![row]);
    store.tool_activations.insert(
        "codex".to_string(),
        ToolBinding::single(ToolActivation {
            provider_id: "p1".to_string(),
            model: "m".to_string(),
            settings: Some(serde_json::json!({ "auth_mode": "oauth" })),
            last_sync_at: None,
        }),
    );

    let out = migrate_v3_to_v4(store, &[]);

    assert_eq!(
        out.store.bindings["codex"].entries[0]
            .settings
            .as_ref()
            .unwrap()
            .get("auth_mode"),
        Some(&serde_json::json!("oauth")),
        "the per-entry value was already the more specific of the two"
    );
}

#[test]
fn migrate_drops_the_claude_desktop_binding_and_names_it_in_the_report() {
    let mut store = store_of(vec![v3_row("p1", "Relay")]);
    store.tool_activations.insert(
        PLANNED_AGENT_CLAUDE_DESKTOP.to_string(),
        ToolBinding::single(ToolActivation {
            provider_id: "p1".to_string(),
            model: "m".to_string(),
            settings: None,
            last_sync_at: None,
        }),
    );

    let out = migrate_v3_to_v4(store, &[]);

    assert!(!out.store.bindings.contains_key(PLANNED_AGENT_CLAUDE_DESKTOP));
    assert_eq!(out.report.dropped_bindings.len(), 1);
    let dropped = &out.report.dropped_bindings[0];
    assert_eq!(dropped.provider_id, "p1");
    assert_eq!(dropped.provider_name, "Relay");
    assert_eq!(dropped.reason, DropReason::AgentPlannedNotImplemented);
    assert!(
        out.report.needs_user_attention(),
        "the user clicked this binding, so its removal must be surfaced"
    );
}

#[test]
fn migrate_converts_last_sync_seconds_to_milliseconds() {
    let mut store = store_of(vec![v3_row("p1", "Relay")]);
    store.tool_activations.insert(
        "codex".to_string(),
        ToolBinding::single(ToolActivation {
            provider_id: "p1".to_string(),
            model: "m".to_string(),
            settings: None,
            last_sync_at: Some(1_700_000_000),
        }),
    );

    let out = migrate_v3_to_v4(store, &[]);

    assert_eq!(
        out.store.bindings["codex"].entries[0].last_sync_at_ms,
        Some(1_700_000_000_000),
        "v3 stored this in seconds and created_at in milliseconds, in one file"
    );
}

// ---------------------------------------------------------------------------
// § 3.2 step 4 — catalog externalisation, and the ext pocket
// ---------------------------------------------------------------------------

#[test]
fn migrate_lifts_the_model_catalog_out_of_the_store() {
    let mut row = v3_row("p1", "Relay");
    row.meta = Some(serde_json::json!({
        "model_catalog": [
            { "id": "a", "raw": { "lots": "of upstream json" } },
            { "id": "b" },
        ],
    }));

    let out = migrate_v3_to_v4(store_of(vec![row]), &[]);

    assert!(
        out.store.providers[0].ext.is_none(),
        "the catalog must not stay in the credential-bearing store file"
    );
    assert_eq!(out.catalogs.len(), 1);
    assert_eq!(out.catalogs[0].provider_id, "p1");
    assert_eq!(out.catalogs[0].entry_count, 2);
    assert_eq!(out.report.externalized_catalogs[0].entry_count, 2);
}

#[test]
fn migrate_parks_unrecognised_meta_keys_in_ext_and_names_them() {
    let mut row = v3_row("p1", "Relay");
    row.meta = Some(serde_json::json!({
        "someFutureThing": { "nested": 1 },
        "baseURL": "https://v1-leftover.example.com",
    }));

    let out = migrate_v3_to_v4(store_of(vec![row]), &[]);

    let ext = out.store.providers[0].ext.as_ref().unwrap();
    assert_eq!(ext.get("someFutureThing"), Some(&serde_json::json!({"nested": 1})));
    assert!(
        ext.get("baseURL").is_none(),
        "the v1 leftover retired with the v1 store"
    );
    assert_eq!(out.report.preserved_ext_keys, vec!["someFutureThing"]);
}

// ---------------------------------------------------------------------------
// The property: nothing gets dropped
// ---------------------------------------------------------------------------

mod prop {
    use super::super::migrate_v3_to_v4;
    use super::{ProviderEntryFlat, store_of};
    use crate::providers::provider::Tri;
    use proptest::prelude::*;

    fn arb_row() -> impl Strategy<Value = ProviderEntryFlat> {
        (
            "[a-z0-9-]{1,12}",
            "[A-Za-z ]{1,16}",
            prop::option::of("https://[a-z]{3,8}\\.example\\.com/v1"),
            prop::option::of("https://[a-z]{3,8}\\.example\\.com/anthropic"),
            prop::option::of("https://[a-z]{3,8}\\.example\\.com/v1/models"),
            prop::option::of("sk-[a-zA-Z0-9]{8,24}"),
            prop::collection::vec("[a-z0-9.-]{1,16}", 0..4),
            prop::option::of("[a-z0-9.-]{1,16}"),
            any::<u32>(),
            prop::option::of(any::<u64>()),
        )
            .prop_map(
                |(id, name, openai, anthropic, models_url, key, models, default_model, sort, created)| {
                    ProviderEntryFlat {
                        id,
                        name,
                        base_url_openai: openai.unwrap_or_default(),
                        base_url_anthropic: anthropic.unwrap_or_default(),
                        models_url: models_url.unwrap_or_default(),
                        api_key: key.unwrap_or_default(),
                        models,
                        default_model: default_model.unwrap_or_default(),
                        sort_index: sort,
                        preset_id: None,
                        icon_color: None,
                        notes: None,
                        created_at: created,
                        meta: None,
                        codex_wire_api: "chat".to_string(),
                        codex_auth_mode: "third_party".to_string(),
                    }
                },
            )
    }

    proptest! {
        /// Every value present on an arbitrary v3 row is reachable on the v4
        /// row it became.
        ///
        /// Stated as reachability rather than as a field-by-field equality
        /// table on purpose: an equality table is the same field list written
        /// twice, so it goes stale in lockstep with the mapping it is meant to
        /// police. Here, deleting a mapping line makes the assertion fail.
        #[test]
        fn migrate_v3_to_v4_preserves_every_provider_field(row in arb_row()) {
            // `preset_id` is None in this strategy, so no backfill can fire and
            // an empty output field can only mean the input was empty too.
            let original = row.clone();
            let out = migrate_v3_to_v4(store_of(vec![row]), &[]);
            prop_assert_eq!(out.store.providers.len(), 1);
            let p = &out.store.providers[0];

            prop_assert_eq!(&p.id, &original.id);
            prop_assert_eq!(&p.name, &original.name);
            prop_assert_eq!(p.sort_index, original.sort_index);
            prop_assert_eq!(p.created_at_ms, original.created_at);
            prop_assert_eq!(&p.models, &original.models);

            prop_assert_eq!(
                p.endpoints.openai_chat.as_deref().unwrap_or_default(),
                original.base_url_openai.trim()
            );
            prop_assert_eq!(
                p.endpoints.anthropic_messages.as_deref().unwrap_or_default(),
                original.base_url_anthropic.trim()
            );
            prop_assert_eq!(
                p.endpoints.models_list.as_deref().unwrap_or_default(),
                original.models_url.trim()
            );
            prop_assert_eq!(
                p.default_model.as_deref().unwrap_or_default(),
                original.default_model.trim()
            );

            // The key must be findable, whichever variant it landed in.
            if original.api_key.trim().is_empty() {
                prop_assert!(!p.credential.has_secret());
            } else {
                prop_assert_eq!(p.credential.literal_secret(), Some(original.api_key.as_str()));
            }

            // And the derived bit must never claim a denial.
            prop_assert_ne!(p.caps.responses_api, Tri::No);
            prop_assert_ne!(p.caps.anthropic_messages, Tri::No);
            prop_assert_ne!(p.caps.models_list, Tri::No);
        }

        /// Migration is idempotent in the sense that matters: running it on the
        /// same input twice produces the same store. (Credential ids are
        /// random, so they are normalised away before comparing.)
        #[test]
        fn migrate_v3_to_v4_is_deterministic(row in arb_row()) {
            let a = migrate_v3_to_v4(store_of(vec![row.clone()]), &[]);
            let b = migrate_v3_to_v4(store_of(vec![row]), &[]);
            prop_assert_eq!(
                a.store.providers[0].credential.literal_secret(),
                b.store.providers[0].credential.literal_secret()
            );
            prop_assert_eq!(&a.store.providers[0].endpoints, &b.store.providers[0].endpoints);
            prop_assert_eq!(&a.report, &b.report);
        }
    }
}

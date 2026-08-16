//! Role routing and the two settings bags.
//!
//! v3 kept role assignments inside `ToolBinding.settings` as an untyped blob
//! that only OMP could read, and kept Claude's tier models in `provider.meta`
//! where only Claude could read them. v4 has one typed [`AgentBinding::roles`]
//! field, so these tests are about that field and about the two remaining bags
//! staying out of each other's way: entry-level ("how this provider behaves
//! under this agent") and agent-level ("this agent's non-role config").
//!
//! The on-disk YAML side is covered in `tool_sync::tests::part5`.

use super::*;

fn role_map(roles: &[(&str, &str, &str)]) -> std::collections::BTreeMap<String, ModelRef> {
    roles
        .iter()
        .map(|(role, provider_id, model)| {
            ((*role).to_string(), ModelRef::new(*provider_id, *model))
        })
        .collect()
}

/// Role names currently assigned on an agent's binding, sorted.
fn assigned_roles(store: &ProvidersStoreV4, agent_id: &str) -> Vec<String> {
    let mut names: Vec<String> = store.bindings[agent_id].roles.keys().cloned().collect();
    names.sort();
    names
}

/// Bind two providers to OMP and return their ids.
fn setup_omp_store() -> (ProvidersStoreV4, String, String) {
    let mut store = ProvidersStoreV4::default();
    let a = create_provider(&mut store, make_provider("Alpha")).unwrap();
    let b = create_provider(&mut store, make_provider("Beta")).unwrap();
    bind_provider(&mut store, "omp", &a.id, Some("model-a"), None).unwrap();
    bind_provider(&mut store, "omp", &b.id, Some("model-a"), None).unwrap();
    (store, a.id, b.id)
}

#[test]
fn setting_roles_leaves_the_entries_alone() {
    let (mut store, a, b) = setup_omp_store();
    let before = store.bindings["omp"].clone();

    set_agent_roles(
        &mut store,
        "omp",
        role_map(&[("default", &a, "model-a"), ("smol", &b, "model-a")]),
    )
    .unwrap();

    let after = &store.bindings["omp"];
    assert_eq!(after.entries, before.entries);
    assert_eq!(after.active_index, before.active_index);
    assert_eq!(assigned_roles(&store, "omp"), vec!["default", "smol"]);
}

#[test]
fn the_agent_settings_bag_clears_on_null() {
    let (mut store, _a, _b) = setup_omp_store();
    update_agent_settings(&mut store, "omp", serde_json::json!({ "keep": 1 })).unwrap();

    update_agent_settings(&mut store, "omp", Value::Null).unwrap();

    assert!(
        store.bindings["omp"].settings.is_none(),
        "`null` empties the bag rather than storing a JSON null"
    );
}

#[test]
fn the_agent_settings_bag_requires_a_bound_agent() {
    let mut store = ProvidersStoreV4::default();
    let err = update_agent_settings(&mut store, "omp", Value::Null).unwrap_err();
    assert!(err.to_string().contains("not currently bound"), "{err}");
}

#[test]
fn the_two_settings_bags_do_not_touch_each_other() {
    let (mut store, a, _b) = setup_omp_store();
    set_agent_roles(&mut store, "omp", role_map(&[("default", &a, "model-a")])).unwrap();
    update_agent_settings(&mut store, "omp", serde_json::json!({ "agentWide": true })).unwrap();

    update_binding_entry_settings(&mut store, "omp", serde_json::json!({ "perEntry": true }))
        .unwrap();

    let binding = &store.bindings["omp"];
    assert_eq!(assigned_roles(&store, "omp"), vec!["default"]);
    assert_eq!(
        binding.settings,
        Some(serde_json::json!({ "agentWide": true })),
        "writing the entry bag must not clobber the agent bag — the v3 names \
         differed by one word and the two were routinely confused"
    );
    assert_eq!(
        binding.active().unwrap().settings,
        Some(serde_json::json!({ "perEntry": true }))
    );
}

#[test]
fn unbinding_one_provider_prunes_only_its_roles() {
    let (mut store, a, b) = setup_omp_store();
    set_agent_roles(
        &mut store,
        "omp",
        role_map(&[
            ("default", &a, "model-a"),
            ("smol", &b, "model-a"),
            ("slow", &b, "model-a"),
        ]),
    )
    .unwrap();

    unbind_provider(&mut store, "omp", &b).unwrap();

    assert_eq!(
        assigned_roles(&store, "omp"),
        vec!["default"],
        "a role on an unbound provider would write a dangling pointer"
    );
    assert_eq!(
        store.bindings["omp"].entries.len(),
        1,
        "unbinding one provider must not clear the agent — that was the v3 bug"
    );
}

#[test]
fn unbinding_a_provider_that_is_not_bound_is_an_error() {
    let (mut store, _a, _b) = setup_omp_store();
    let err = unbind_provider(&mut store, "omp", "ghost").unwrap_err();
    assert!(err.to_string().contains("not bound"), "{err}");
}

#[test]
fn deleting_a_provider_prunes_its_roles() {
    let (mut store, a, b) = setup_omp_store();
    set_agent_roles(
        &mut store,
        "omp",
        role_map(&[("default", &a, "model-a"), ("smol", &b, "model-a")]),
    )
    .unwrap();

    delete_provider(&mut store, &a).unwrap();

    assert_eq!(assigned_roles(&store, "omp"), vec!["smol"]);
}

#[test]
fn roles_survive_a_rebind() {
    let (mut store, a, b) = setup_omp_store();
    set_agent_roles(&mut store, "omp", role_map(&[("smol", &b, "model-a")])).unwrap();

    // Re-binding a provider (the user picks another model in the matrix)
    // rewrites an entry, not the role map.
    bind_provider(&mut store, "omp", &a, Some("model-b"), None).unwrap();

    assert_eq!(assigned_roles(&store, "omp"), vec!["smol"]);
}

#[test]
fn unbinding_the_agent_drops_its_roles_too() {
    let (mut store, a, _b) = setup_omp_store();
    set_agent_roles(&mut store, "omp", role_map(&[("default", &a, "model-a")])).unwrap();

    unbind_agent(&mut store, "omp").unwrap();

    let binding = &store.bindings["omp"];
    assert!(binding.is_empty());
    assert!(
        binding.roles.is_empty(),
        "an unbound agent has nothing to route"
    );
}

// ---------------------------------------------------------------------------
// The split commands
// ---------------------------------------------------------------------------

#[test]
fn set_active_binding_moves_the_pointer_and_nothing_else() {
    let (mut store, a, b) = setup_omp_store();
    update_binding_entry(&mut store, "omp", &a, Some("model-x"), None).unwrap();

    set_active_binding(&mut store, "omp", &a).unwrap();

    let binding = &store.bindings["omp"];
    assert_eq!(binding.active().unwrap().provider_id, a);
    assert_eq!(
        binding.active().unwrap().model,
        "model-x",
        "moving the pointer must not need the model re-supplied — that is what \
         made v3's single activate_tool unusable for this"
    );
    assert_eq!(binding.entries.len(), 2);
    assert!(binding.binds_provider(&b));
}

#[test]
fn set_active_binding_rejects_a_provider_that_is_not_bound() {
    let (mut store, _a, _b) = setup_omp_store();
    let err = set_active_binding(&mut store, "omp", "ghost").unwrap_err();
    assert!(err.to_string().contains("not bound"), "{err}");
}

#[test]
fn update_binding_entry_edits_without_moving_the_pointer() {
    let (mut store, a, b) = setup_omp_store();
    set_active_binding(&mut store, "omp", &b).unwrap();

    update_binding_entry(&mut store, "omp", &a, Some("model-z"), None).unwrap();

    let binding = &store.bindings["omp"];
    assert_eq!(binding.active().unwrap().provider_id, b, "pointer unmoved");
    let edited = binding.entries.iter().find(|e| e.provider_id == a).unwrap();
    assert_eq!(edited.model, "model-z");
}

#[test]
fn update_binding_entry_leaves_untouched_fields_alone() {
    let (mut store, a, _b) = setup_omp_store();
    update_binding_entry(
        &mut store,
        "omp",
        &a,
        None,
        Some(serde_json::json!({ "k": 1 })),
    )
    .unwrap();

    update_binding_entry(&mut store, "omp", &a, Some("model-y"), None).unwrap();

    let entry = store.bindings["omp"]
        .entries
        .iter()
        .find(|e| e.provider_id == a)
        .unwrap();
    assert_eq!(entry.model, "model-y");
    assert_eq!(
        entry.settings,
        Some(serde_json::json!({ "k": 1 })),
        "`None` means leave it alone, so a model change needs no read-modify-write"
    );
}

#[test]
fn a_single_provider_agent_holds_one_entry() {
    let mut store = ProvidersStoreV4::default();
    let a = create_provider(&mut store, make_provider("Alpha")).unwrap();
    let b = create_provider(&mut store, make_provider("Beta")).unwrap();

    bind_provider(&mut store, "claude-code", &a.id, Some("model-a"), None).unwrap();
    bind_provider(&mut store, "claude-code", &b.id, Some("model-a"), None).unwrap();

    let binding = &store.bindings["claude-code"];
    assert_eq!(binding.entries.len(), 1);
    assert_eq!(binding.entries[0].provider_id, b.id);
}

#[test]
fn binding_falls_back_to_the_providers_default_model() {
    let mut store = ProvidersStoreV4::default();
    let a = create_provider(&mut store, make_provider("Alpha")).unwrap();

    let entry = bind_provider(&mut store, "omp", &a.id, None, None).unwrap();

    assert_eq!(entry.model, "model-a");
}

// ---------------------------------------------------------------------------
// Capability gating at bind time
// ---------------------------------------------------------------------------

#[test]
fn a_chat_only_provider_cannot_be_bound_to_codex() {
    let mut store = ProvidersStoreV4::default();
    let relay = create_provider(&mut store, make_provider("Relay")).unwrap();

    let err = bind_provider(&mut store, "codex", &relay.id, Some("m"), None).unwrap_err();

    assert!(
        err.to_string().contains("/v1/responses"),
        "the refusal has to name the missing endpoint, or the user cannot act on it: {err}"
    );
    assert!(!store.bindings.contains_key("codex"));
}

#[test]
fn a_responses_capable_provider_binds_to_codex() {
    let mut store = ProvidersStoreV4::default();
    let provider = create_provider(&mut store, make_responses_provider("OpenAI")).unwrap();

    bind_provider(&mut store, "codex", &provider.id, Some("gpt-5.4"), None).unwrap();

    assert_eq!(store.bindings["codex"].entries.len(), 1);
}

#[test]
fn a_provider_with_no_anthropic_endpoint_cannot_be_bound_to_claude_code() {
    let mut store = ProvidersStoreV4::default();
    let mut provider = make_provider("Relay");
    provider.endpoints.anthropic_messages = None;
    let provider = create_provider(&mut store, provider).unwrap();

    let err = bind_provider(&mut store, "claude-code", &provider.id, Some("m"), None).unwrap_err();

    assert!(err.to_string().contains("Anthropic"), "{err}");
}

#[test]
fn a_probe_that_said_no_blocks_the_bind_even_with_an_endpoint() {
    let mut store = ProvidersStoreV4::default();
    let mut provider = make_responses_provider("Relay");
    provider.caps.responses_api = Tri::No;
    let provider = create_provider(&mut store, provider).unwrap();

    let err = bind_provider(&mut store, "codex", &provider.id, Some("m"), None).unwrap_err();

    assert!(err.to_string().contains("does not support"), "{err}");
}

#[test]
fn an_unprobed_provider_with_an_endpoint_still_binds() {
    let mut store = ProvidersStoreV4::default();
    let mut provider = make_responses_provider("Relay");
    provider.caps.responses_api = Tri::Unknown;
    let provider = create_provider(&mut store, provider).unwrap();

    assert!(
        bind_provider(&mut store, "codex", &provider.id, Some("m"), None).is_ok(),
        "migration writes Unknown for every row; denying on it would unbind everyone (R-2)"
    );
}

#[test]
fn a_native_login_row_binds_without_any_endpoint() {
    let mut store = ProvidersStoreV4::default();
    let official = create_provider(
        &mut store,
        create_provider_from_preset(CODEX_OFFICIAL_ID, "").unwrap(),
    )
    .unwrap();

    let entry = bind_provider(&mut store, "codex", &official.id, None, None).unwrap();

    assert_eq!(
        entry
            .settings
            .as_ref()
            .and_then(|s| s.get("auth_mode"))
            .and_then(|v| v.as_str()),
        Some("oauth"),
        "Codex Official binds a ChatGPT session; writing OPENAI_API_KEY would break it"
    );
}

// ---------------------------------------------------------------------------
// Reasoning capability: the data behind a narrowed thinking picker
// ---------------------------------------------------------------------------

/// A provider's own `/v1/models` list says nothing about reasoning, and the
/// absence has to stay distinguishable from a "no reasoning" answer — the
/// picker widens on the first and narrows on the second.
#[test]
fn a_plain_models_list_reports_unknown_reasoning_rather_than_none() {
    let body = serde_json::json!({ "data": [{ "id": "relay-model" }] });
    let catalog = catalog_from_provider_models(&body);
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].reasoning, None);
}

#[test]
fn a_registry_entry_carries_its_reasoning_tiers() {
    let body = serde_json::json!({
        "openai": { "models": {
            "gpt-5.4": {
                "id": "gpt-5.4",
                "reasoning": true,
                "reasoning_options": { "effort": ["low", "medium", "high"], "can_disable": false }
            },
            "chat-only": { "id": "chat-only", "reasoning": false }
        }}
    });
    let catalog = catalog_from_registry(&body);
    let by_id = |id: &str| {
        catalog
            .iter()
            .find(|entry| entry.id == id)
            .unwrap_or_else(|| panic!("{id} missing"))
            .reasoning
            .clone()
    };

    assert_eq!(
        by_id("gpt-5.4"),
        Some(Reasoning::Effort {
            values: vec![Effort::Low, Effort::Medium, Effort::High],
            default: None,
            can_disable: false,
        })
    );
    assert_eq!(
        by_id("chat-only"),
        Some(Reasoning::None),
        "an explicit `reasoning: false` is knowledge, not absence of it"
    );
}

/// A model that reasons but publishes no tier list is a toggle, not a nine-way
/// picker — the distinction v3 could not make.
#[test]
fn a_reasoning_model_without_tiers_is_a_toggle() {
    let body = serde_json::json!({ "data": [{ "id": "thinky", "reasoning": true }] });
    let catalog = catalog_from_provider_models(&body);
    assert_eq!(
        catalog[0].reasoning,
        Some(Reasoning::Toggle { can_disable: true })
    );
}

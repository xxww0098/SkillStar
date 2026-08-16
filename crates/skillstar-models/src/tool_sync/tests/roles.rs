//! Role routing across the registry: declarations, projection, and drops.
//!
//! Three claims are under test here, and each one is the sort that goes stale
//! silently if nobody pins it:
//!
//! 1. **Every declared role reaches disk.** The registry is a promise to the
//!    user that configuring a role changes a file. A role listed but never
//!    written is the exact defect this work package was sent to fix, so the
//!    check is generic over the registry rather than written per agent.
//! 2. **Roles that cannot be written are reported, not dropped in silence.**
//! 3. **Adding an agent needs a registry row and a writer, and nothing else.**

use super::*;
use crate::providers::{
    DroppedRole, ModelRef, Provider, ProvidersStoreV4, RoleDropReason, migrate,
};
use std::collections::BTreeMap;

/// Assign every role an agent declares, so the writer has nothing left to skip.
fn assign_every_role(spec: &AgentSpec, provider_id: &str) -> BTreeMap<String, ModelRef> {
    spec.roles
        .iter()
        .enumerate()
        .map(|(i, def)| {
            (
                def.id.to_string(),
                // A distinct model per role: identical values could not tell
                // "each key got its own" from "one value was broadcast".
                ModelRef::new(provider_id, format!("model-for-{}-{i}", def.id)),
            )
        })
        .collect()
}

/// Run an agent's writer against a sandbox home and return every byte it wrote.
fn write_all_configs(spec: &AgentSpec, store: &ProvidersStoreV4) -> (ToolSyncResultFlat, String) {
    let result = sync_binding_with_spec(spec, store);
    let mut written = String::new();
    for file in spec.files {
        let path = (file.resolve)().expect("resolve config path in the sandbox home");
        if let Ok(content) = std::fs::read_to_string(&path) {
            written.push_str(&content);
            written.push('\n');
        }
    }
    (result, written)
}

/// **The rule that keeps `AgentSpec::roles` honest.**
///
/// For each agent, assign every role it declares and assert the agent's own key
/// for that role appears in what the writer produced. A registry row that lists
/// a role its writer ignores is a UI that offers a setting with no effect —
/// which is what the OMP audit found and what a declaration-only abstraction
/// makes easy to reintroduce.
#[test]
fn every_declared_role_reaches_disk() {
    for spec in agent_specs() {
        if spec.roles.is_empty() {
            continue;
        }
        let _home = use_sandbox_home();
        let _cache = DataDirSandbox::new();

        let mut provider = flat("role-p1", "relay");
        provider.endpoints.anthropic_messages =
            Some("https://relay.example.com/anthropic".to_string());
        provider.endpoints.openai_responses = Some("https://relay.example.com/v1".to_string());

        let mut binding = AgentBinding::single(entry(&provider.id, "entry-model"));
        binding.roles = assign_every_role(spec, &provider.id);

        let mut store = ProvidersStoreV4 {
            providers: vec![provider],
            ..Default::default()
        };
        store.bindings.insert(spec.id.to_string(), binding.clone());

        let (result, written) = write_all_configs(spec, &store);
        assert!(result.success, "{}: {:?}", spec.id, result.error);
        assert!(
            result.dropped_roles.is_empty(),
            "{}: every role was assignable, yet {:?} were dropped",
            spec.id,
            result.dropped_roles
        );

        for def in spec.roles {
            assert!(
                written.contains(def.agent_key),
                "{}: role `{}` is declared with key `{}` but no config file mentions it — \
                 a declared role the writer ignores is a setting with no effect\n{written}",
                spec.id,
                def.id,
                def.agent_key
            );
        }
    }
}

/// The registry's role vocabulary has to stay usable by the shared UI: a role
/// promoted above the fold must be one the panel can label, i.e. one of the
/// canonical ids. Agent-private roles are fine, but they belong behind the
/// disclosure rather than in the four rows everyone sees.
#[test]
fn registry_role_ids_are_projectable() {
    for spec in agent_specs() {
        let mut seen = std::collections::HashSet::new();
        for def in spec.roles {
            assert!(
                seen.insert(def.id),
                "{}: role `{}` declared twice",
                spec.id,
                def.id
            );
            assert!(!def.agent_key.is_empty(), "{}/{}", spec.id, def.id);
        }
        // Every agent that has roles at all must be able to answer "which model
        // for a normal turn" — a role map with no default cannot be projected
        // onto any target, since every target has that one concept.
        assert!(
            spec.roles.is_empty()
                || spec
                    .roles
                    .iter()
                    .any(|def| def.id == crate::providers::ROLE_DEFAULT),
            "{}: declares roles but no `default`",
            spec.id
        );
    }
}

/// A fallback chain that loops would make `resolve_role` walk until its bound.
/// Ruling it out here means the bound is a backstop, not a load-bearing part.
#[test]
fn registry_role_chains_terminate_at_a_declared_role() {
    for spec in agent_specs() {
        for def in spec.roles {
            let mut current = def;
            for _ in 0..spec.roles.len() {
                let Some(next_id) = current.inherits else { break };
                let next = spec.roles.iter().find(|d| d.id == next_id);
                assert!(
                    next.is_some(),
                    "{}: role `{}` inherits `{next_id}`, which the agent does not declare",
                    spec.id,
                    def.id
                );
                current = next.unwrap();
            }
            assert!(
                current.inherits.is_none()
                    || spec.roles.iter().any(|d| Some(d.id) == current.inherits),
                "{}: role `{}` has a cyclic fallback chain",
                spec.id,
                def.id
            );
        }
    }
}

/// The registry's OMP spellings and the migration's rename table are two views
/// of one fact. They were written apart, so they are pinned together: a drift
/// would rename a role on migration and then write it under the other name.
#[test]
fn registry_agent_keys_match_the_migration_table() {
    let omp = agent_spec("omp").unwrap();
    for def in omp.roles {
        assert_eq!(
            def.agent_key,
            migrate::omp_role_key(def.id),
            "role `{}` spells itself two ways",
            def.id
        );
        assert_eq!(
            migrate::canonical_role_key(def.agent_key),
            def.id,
            "role `{}` does not survive the round trip",
            def.id
        );
    }
    // And the UI list the frontend mirrors is exactly the registry's keys.
    let keys: Vec<&str> = omp.roles.iter().map(|def| def.agent_key).collect();
    assert_eq!(keys, OMP_MODEL_ROLES.to_vec());
}

/// The three tiers the cross-project survey concluded with (02 §9.4). Pinned as
/// a literal because the tier is a product decision — "Pi has no roles, Claude
/// has tier aliases, OMP has the full map" — not something to be re-derived
/// from whatever the registry happens to say today.
#[test]
fn agents_land_in_the_three_role_tiers() {
    let tier = |id: &str| agent_spec(id).unwrap().roles.len();

    // Tier 1 — no role concept. Codex and OpenCode both have one upstream and
    // are here because their writers do not project it yet; the day one does,
    // this line changes in the same commit as the writer.
    for id in ["claude-desktop", "codex", "opencode", "pi"] {
        assert_eq!(tier(id), 0, "{id} should declare no roles");
    }
    // Tier 2 — a main model plus tier aliases.
    assert_eq!(tier("claude-code"), 5);
    // Tier 3 — the full map.
    assert_eq!(tier("omp"), 10);
}

// ---------------------------------------------------------------------------
// Claude Code: the mapping the frontend never persisted
// ---------------------------------------------------------------------------

/// End-to-end for 00 §1.3: what the role panel saves is what lands in the env
/// block. The chain the renderer drives is `roles` on the binding → store →
/// writer → `~/.claude/settings.json`, and the middle two links are what this
/// asserts; `roles_round_trip_through_the_v3_settings_bag` in the command layer
/// covers the renderer's end of it.
#[test]
fn claude_role_mapping_lands_in_the_env_block() {
    let _home = use_sandbox_home();

    let mut provider = flat("claude-p1", "relay");
    provider.endpoints.anthropic_messages = Some("https://relay.example.com/anthropic".to_string());

    let mut binding = AgentBinding::single(entry(&provider.id, "entry-model"));
    for (role, model) in [
        ("fast", "haiku-fast"),
        ("sonnet", "sonnet-mid"),
        ("opus", "opus-deep"),
        ("subagent", "subagent-model"),
    ] {
        binding
            .roles
            .insert(role.to_string(), ModelRef::new(&provider.id, model));
    }

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    sync_to_claude_code_inner(&provider, "entry-model", &binding.roles, &path).unwrap();

    let env: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
        .unwrap();
    let env = &env["env"];
    assert_eq!(env["ANTHROPIC_MODEL"], "entry-model");
    assert_eq!(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "haiku-fast");
    assert_eq!(env["ANTHROPIC_DEFAULT_SONNET_MODEL"], "sonnet-mid");
    assert_eq!(env["ANTHROPIC_DEFAULT_OPUS_MODEL"], "opus-deep");
    assert_eq!(env["CLAUDE_CODE_SUBAGENT_MODEL"], "subagent-model");
}

/// An explicit `default` role beats the entry's model. The entry is the older
/// statement of intent and stays the fallback, but a user who set the role
/// meant the role.
#[test]
fn an_assigned_default_role_wins_over_the_entry_model() {
    let _home = use_sandbox_home();
    let mut provider = flat("claude-p2", "relay");
    provider.endpoints.anthropic_messages = Some("https://relay.example.com/anthropic".to_string());

    let mut roles = BTreeMap::new();
    roles.insert(
        "default".to_string(),
        ModelRef::new(&provider.id, "role-model"),
    );

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    sync_to_claude_code_inner(&provider, "entry-model", &roles, &path).unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(json["env"]["ANTHROPIC_MODEL"], "role-model");
}

/// Claude's env block names one base URL, so a role pointing at some other
/// provider cannot be honoured — the model id would be sent to the bound host.
/// v3 wrote it anyway. The key is now left out **and** the skip is reported.
#[test]
fn a_claude_role_on_another_provider_is_skipped_and_reported() {
    let _home = use_sandbox_home();

    let mut bound = flat("claude-bound", "relay");
    bound.endpoints.anthropic_messages = Some("https://relay.example.com/anthropic".to_string());
    let other = flat("claude-other", "elsewhere");

    let mut binding = AgentBinding::single(entry(&bound.id, "entry-model"));
    binding
        .roles
        .insert("fast".to_string(), ModelRef::new(&other.id, "cheap-model"));
    // A role this agent has no env key for at all.
    binding.roles.insert(
        "designer".to_string(),
        ModelRef::new(&bound.id, "pretty-model"),
    );

    let mut store = ProvidersStoreV4 {
        providers: vec![bound.clone(), other.clone()],
        ..Default::default()
    };
    store.bindings.insert("claude-code".to_string(), binding);

    let result = sync_tool_binding(&store, "claude-code");
    assert!(result.success);
    assert_eq!(
        result.dropped_roles,
        vec![
            // Role order follows the store's map, which is sorted by role id.
            DroppedRole::new("designer", RoleDropReason::RoleNotSupported),
            DroppedRole::for_provider("fast", RoleDropReason::ProviderNotBound, &other.id),
        ],
        "a skipped role must come back named, not vanish"
    );

    let path = resolve_tool_config_path("claude-code").unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        json["env"].get("ANTHROPIC_DEFAULT_HAIKU_MODEL").is_none(),
        "the model of an unbindable provider must not be written for the bound one"
    );
}

// ---------------------------------------------------------------------------
// OMP: the drops that used to be silent (02 §9.3 gap 1)
// ---------------------------------------------------------------------------

/// The three conditions `resolve_omp_roles` skips on, each of which used to be
/// a bare `continue`, now each carrying a reason back to the caller.
#[test]
fn omp_reports_why_each_dropped_role_was_dropped() {
    let _home = use_sandbox_home();

    let bound = flat("omp-bound", "relay");
    let mut no_endpoint = flat("omp-noendpoint", "silent");
    no_endpoint.endpoints.openai_chat = None;
    let unbound = flat("omp-unbound", "elsewhere");

    let mut binding = AgentBinding {
        entries: vec![
            entry(&bound.id, "model-a"),
            entry(&no_endpoint.id, "model-a"),
        ],
        ..Default::default()
    };
    binding
        .roles
        .insert("default".to_string(), ModelRef::new(&bound.id, "model-a"));
    binding
        .roles
        .insert("fast".to_string(), ModelRef::new(&unbound.id, "cheap"));
    binding.roles.insert(
        "plan".to_string(),
        ModelRef::new(&no_endpoint.id, "planner"),
    );
    binding.roles.insert(
        "vision".to_string(),
        ModelRef::new("provider-that-was-deleted", "seer"),
    );
    binding
        .roles
        .insert("tiny".to_string(), ModelRef::new(&bound.id, "   "));

    let mut store = ProvidersStoreV4 {
        providers: vec![bound.clone(), no_endpoint.clone(), unbound.clone()],
        ..Default::default()
    };
    store.bindings.insert("omp".to_string(), binding);

    let result = sync_tool_binding(&store, "omp");
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        result.dropped_roles,
        vec![
            DroppedRole::for_provider("fast", RoleDropReason::ProviderNotBound, &unbound.id),
            DroppedRole::for_provider(
                "plan",
                RoleDropReason::ProviderHasNoEndpoint,
                &no_endpoint.id
            ),
            DroppedRole::new("tiny", RoleDropReason::NoModel),
            DroppedRole::for_provider(
                "vision",
                RoleDropReason::ProviderMissing,
                "provider-that-was-deleted"
            ),
        ]
    );

    // And the surviving role is still written — reporting drops must not turn
    // into refusing to write.
    let config = std::fs::read_to_string(resolve_omp_config_path().unwrap()).unwrap();
    assert!(config.contains("default:"), "{config}");
    assert!(!config.contains("smol:"), "{config}");
}

/// A binding whose roles are all writable reports nothing, so the UI has no
/// warning to draw. The negative case matters as much as the positive one: a
/// drop channel that always has something in it gets ignored.
#[test]
fn a_fully_writable_role_map_reports_no_drops() {
    let _home = use_sandbox_home();
    let bound = flat("omp-clean", "relay");

    let mut binding = AgentBinding::single(entry(&bound.id, "model-a"));
    binding
        .roles
        .insert("default".to_string(), ModelRef::new(&bound.id, "model-a"));
    binding
        .roles
        .insert("fast".to_string(), ModelRef::new(&bound.id, "model-b"));

    let mut store = ProvidersStoreV4 {
        providers: vec![bound],
        ..Default::default()
    };
    store.bindings.insert("omp".to_string(), binding);

    let result = sync_tool_binding(&store, "omp");
    assert!(result.success);
    assert!(result.dropped_roles.is_empty(), "{:?}", result.dropped_roles);
}

// ---------------------------------------------------------------------------
// G1: adding an agent touches two places
// ---------------------------------------------------------------------------

fn fake_sync(_binding: &AgentBinding, _providers: &[Provider]) -> Result<ToolSyncResultFlat> {
    Ok(ToolSyncResultFlat {
        tool_id: "fake-agent".to_string(),
        success: true,
        config_path: Some("/dev/null".to_string()),
        error: None,
        backup_path: None,
        dropped_roles: Vec::new(),
    })
}

fn fake_unsync() -> Result<()> {
    Ok(())
}

fn fake_detect(_path: &Path) -> Result<Option<String>> {
    Ok(None)
}

fn fake_resolve() -> Result<PathBuf> {
    Ok(sync_home_dir()?.join(".fake-agent").join("config.json"))
}

/// **The design goal, made falsifiable.**
///
/// A synthetic agent — an id no dispatch site has ever heard of — is pushed
/// through the generic sync path with nothing but a registry row and its writer
/// functions. It works because the dispatcher reads columns rather than
/// branching on the id; the day someone adds a `match tool_id` to that path,
/// this test is what fails.
#[test]
fn a_synthetic_agent_syncs_through_the_registry_alone() {
    let _home = use_sandbox_home();

    static FAKE_FILES: &[AgentConfigFileSpec] = &[AgentConfigFileSpec {
        file_id: "config",
        label: "config.json",
        format: "json",
        resolve: fake_resolve,
        default_content: "{}\n",
    }];
    static FAKE_ROLES: &[crate::providers::RoleDef] = &[crate::providers::RoleDef::primary(
        crate::providers::ROLE_DEFAULT,
        "model",
    )];
    let spec = AgentSpec {
        id: "fake-agent",
        display_name: "Fake Agent",
        binary_name: "fake",
        config_dir_probes: &[".fake-agent"],
        kind: AgentKind::Multi,
        required_wire: RequiredWire::OpenaiChat,
        roles: FAKE_ROLES,
        files: FAKE_FILES,
        sync_binding: fake_sync,
        unsync: fake_unsync,
        detect_provider: fake_detect,
    };

    let provider = flat("fake-p1", "relay");
    let mut store = ProvidersStoreV4 {
        providers: vec![provider.clone()],
        ..Default::default()
    };

    // Unbound → the registry's unsync column runs, no id knowledge needed.
    let empty = sync_binding_with_spec(&spec, &store);
    assert!(empty.success);

    // Bound → the registry's writer column runs.
    store.bindings.insert(
        "fake-agent".to_string(),
        AgentBinding::single(entry(&provider.id, "model-a")),
    );
    let bound = sync_binding_with_spec(&spec, &store);
    assert!(bound.success);
    assert_eq!(bound.tool_id, "fake-agent");
}

/// The other half of the same claim, from the other direction: count where an
/// agent id is spelled out at all.
///
/// The registry is place one and each agent's own writer module is place two.
/// The three entries below are neither, and are pinned individually so a fourth
/// cannot appear without someone deciding it should:
///
/// - `conflicts.rs` — the legacy `~/.claude.json` probe, which is a fact about
///   one historical file rather than about agents in general.
/// - `migrate_configs.rs` — the one-off v3 repair, which by definition knows
///   the agents that existed at v3.
/// - `paths_files.rs` — `.config/opencode` as a *directory* name; the string
///   happens to equal the agent id, which is a coincidence of naming.
#[test]
fn agent_ids_are_spelled_out_only_in_the_registry_and_the_writers() {
    const SOURCES: &[(&str, &str, usize)] = &[
        ("agents.rs", include_str!("../agents.rs"), usize::MAX),
        ("sync.rs", include_str!("../sync.rs"), usize::MAX),
        (
            "multi_provider.rs",
            include_str!("../multi_provider.rs"),
            usize::MAX,
        ),
        (
            "omp_provider.rs",
            include_str!("../omp_provider.rs"),
            usize::MAX,
        ),
        ("types.rs", include_str!("../types.rs"), usize::MAX),
        ("conflicts.rs", include_str!("../conflicts.rs"), 1),
        (
            "migrate_configs.rs",
            include_str!("../migrate_configs.rs"),
            6,
        ),
        ("paths_files.rs", include_str!("../paths_files.rs"), 2),
        ("backup_merge.rs", include_str!("../backup_merge.rs"), 0),
        ("view.rs", include_str!("../view.rs"), 0),
        ("mod.rs", include_str!("../mod.rs"), 0),
    ];

    let ids: Vec<String> = agent_specs()
        .iter()
        .map(|spec| format!("\"{}\"", spec.id))
        .collect();

    for (name, source, budget) in SOURCES {
        if *budget == usize::MAX {
            continue;
        }
        let count: usize = ids
            .iter()
            .map(|id| source.matches(id.as_str()).count())
            .sum();
        assert_eq!(
            count, *budget,
            "{name} spells an agent id {count} times, budget {budget}. \
             Adding an agent should touch the registry and that agent's writer — \
             if a third site is genuinely needed, raise the budget here and say why."
        );
    }
}

// ---------------------------------------------------------------------------
// Gap 2: thinking levels narrowed by what the model can do
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_model_keeps_the_whole_thinking_grammar() {
    // No catalogue entry means no knowledge, and taking levels away on no
    // evidence would remove ones that work.
    assert_eq!(omp_thinking_levels_for(None), OMP_THINKING_LEVELS.to_vec());
}

#[test]
fn a_model_without_reasoning_offers_no_tiers() {
    use crate::providers::Reasoning;
    assert_eq!(
        omp_thinking_levels_for(Some(&Reasoning::None)),
        vec!["inherit"],
        "offering `xhigh` for a model with no reasoning mode is a control that does nothing"
    );
}

#[test]
fn an_effort_model_offers_exactly_its_tiers_in_grammar_order() {
    use crate::providers::{Effort, Reasoning};
    let reasoning = Reasoning::Effort {
        // Deliberately out of order: the picker must read low → high regardless.
        values: vec![Effort::High, Effort::Low, Effort::Medium],
        default: Some(Effort::Medium),
        can_disable: true,
    };
    assert_eq!(
        omp_thinking_levels_for(Some(&reasoning)),
        vec!["inherit", "off", "low", "medium", "high", "auto"]
    );
}

#[test]
fn a_budget_model_maps_onto_the_tiers_omp_can_express() {
    use crate::providers::Reasoning;
    let reasoning = Reasoning::BudgetTokens {
        min: Some(1024),
        max: Some(32000),
        default: Some(4096),
    };
    let levels = omp_thinking_levels_for(Some(&reasoning));
    assert!(levels.contains(&"high"));
    assert!(
        !levels.contains(&"minimal"),
        "OMP's suffix grammar has no token count, so only the tiers it maps are offered: {levels:?}"
    );
}

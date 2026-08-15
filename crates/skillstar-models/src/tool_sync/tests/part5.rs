//! Oh My Pi (OMP) binding writer tests — YAML `models.yml` + `config.yml`.
//!
//! Split out of `part4` (which keeps Codex/OpenCode/Pi) purely to stay under
//! the file-size gate; the shared `flat` / `entry` builders moved up to
//! `tests/mod.rs` so both halves reach them through `use super::*`.

use super::*;
use crate::providers::{AgentBinding, Effort, ModelRef};

#[test]
fn omp_binding_writes_blocks_and_default_pointer() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");

    // Pre-existing files: a user-owned provider block + unrelated settings and
    // non-default roles must all survive the sync untouched.
    std::fs::write(
        &models_path,
        "providers:\n  ollama:\n    baseUrl: http://localhost:11434/v1\n    api: openai-completions\n    apiKey: ollama\n    models:\n      - id: llama3.1:8b\n  skillstar_dead0000:\n    baseUrl: https://stale\n",
    )
    .unwrap();
    std::fs::write(
        &config_path,
        "theme:\n  light: light\nmodelRoles:\n  slow: aiproxy/deepseek-v4-flash:xhigh\n  smol: anthropic/claude-opus-5:max\n",
    )
    .unwrap();

    let providers = vec![flat("aaaa1111", "alpha"), flat("bbbb2222", "beta")];
    let binding = AgentBinding {
        entries: vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        roles: Default::default(),
        active_index: 1,
        settings: None,
    };

    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&models_path).unwrap()).unwrap();
    let provider_map = parsed.get("providers").unwrap().as_mapping().unwrap();
    // User block survives; stale managed block gone; both managed blocks present.
    assert!(provider_map.contains_key(serde_yaml::Value::String("ollama".into())));
    assert!(!provider_map.contains_key(serde_yaml::Value::String("skillstar_dead0000".into())));
    let alpha = provider_map
        .get(serde_yaml::Value::String("skillstar_aaaa1111".into()))
        .unwrap();
    assert_eq!(
        alpha.get("baseUrl").unwrap().as_str().unwrap(),
        "https://alpha.example.com/v1"
    );
    assert_eq!(
        alpha.get("api").unwrap().as_str().unwrap(),
        "openai-completions"
    );
    assert_eq!(
        alpha.get("apiKey").unwrap().as_str().unwrap(),
        "sk-aaaa1111"
    );
    // Model entries are minimal `{ id }` objects (OMP supplies its own defaults).
    let alpha_models = alpha.get("models").unwrap().as_sequence().unwrap();
    assert_eq!(alpha_models.len(), 2);
    assert_eq!(
        alpha_models[0].get("id").unwrap().as_str().unwrap(),
        "model-a"
    );
    assert!(provider_map.contains_key(serde_yaml::Value::String("skillstar_bbbb2222".into())));

    // Active (index 1 → beta) drives modelRoles.default; other roles and
    // unrelated settings survive.
    let config: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let roles = config.get("modelRoles").unwrap().as_mapping().unwrap();
    assert_eq!(
        roles
            .get(serde_yaml::Value::String("default".into()))
            .unwrap()
            .as_str()
            .unwrap(),
        "skillstar_bbbb2222/model-b"
    );
    assert_eq!(
        roles
            .get(serde_yaml::Value::String("slow".into()))
            .unwrap()
            .as_str()
            .unwrap(),
        "aiproxy/deepseek-v4-flash:xhigh"
    );
    assert_eq!(
        roles
            .get(serde_yaml::Value::String("smol".into()))
            .unwrap()
            .as_str()
            .unwrap(),
        "anthropic/claude-opus-5:max"
    );
    assert_eq!(
        config
            .get("theme")
            .unwrap()
            .get("light")
            .unwrap()
            .as_str()
            .unwrap(),
        "light"
    );
}

#[test]
fn omp_binding_creates_files_when_absent() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");

    let providers = vec![flat("aaaa1111", "alpha")];
    let binding = AgentBinding::single(entry("aaaa1111", "model-a"));
    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&models_path).unwrap()).unwrap();
    assert!(
        parsed
            .get("providers")
            .unwrap()
            .as_mapping()
            .unwrap()
            .contains_key(serde_yaml::Value::String("skillstar_aaaa1111".into()))
    );
    let config: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(
        config
            .get("modelRoles")
            .unwrap()
            .get("default")
            .unwrap()
            .as_str()
            .unwrap(),
        "skillstar_aaaa1111/model-a"
    );
}

#[test]
fn omp_unsync_removes_managed_blocks_and_managed_pointer_only() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");

    let providers = vec![flat("aaaa1111", "alpha")];
    let binding = AgentBinding::single(entry("aaaa1111", "model-a"));
    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    // Inject a user-owned provider block and a user-owned default role pointer
    // that must both survive unsync.
    let mut parsed: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&models_path).unwrap()).unwrap();
    parsed
        .get_mut("providers")
        .unwrap()
        .as_mapping_mut()
        .unwrap()
        .insert(
            serde_yaml::Value::String("mine".into()),
            serde_yaml::to_value(serde_json::json!({ "baseUrl": "https://mine" })).unwrap(),
        );
    std::fs::write(&models_path, serde_yaml::to_string(&parsed).unwrap()).unwrap();

    unsync_omp_all_at(&models_path, &config_path).unwrap();

    let after: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&models_path).unwrap()).unwrap();
    let provider_map = after.get("providers").unwrap().as_mapping().unwrap();
    assert!(provider_map.contains_key(serde_yaml::Value::String("mine".into())));
    assert!(
        !provider_map
            .keys()
            .any(|k| k.as_str().is_some_and(is_skillstar_managed_key))
    );

    // Managed default pointer cleared; user-owned roles survive.
    let config: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let roles = config.get("modelRoles").unwrap().as_mapping().unwrap();
    assert!(!roles.contains_key(serde_yaml::Value::String("default".into())));
}

// ---------------------------------------------------------------------------
// OMP model roles (default / smol / slow / plan …)
// ---------------------------------------------------------------------------

/// Build an `AgentBinding` whose `roles` map assigns OMP roles.
/// `roles` items are `(role, provider_id, model, thinking)`.
///
/// v3 kept these in an OMP-private settings bag; v4 has them as a typed field
/// on the binding, so the thinking level arrives as an [`Effort`] rather than a
/// free string. An unrecognised level still yields "no effort", which is what
/// `from_omp_thinking` is for.
fn omp_binding_with_roles(
    entries: Vec<BindingEntry>,
    active_index: usize,
    roles: &[(&str, &str, &str, Option<&str>)],
) -> AgentBinding {
    let map: std::collections::BTreeMap<String, ModelRef> = roles
        .iter()
        .map(|(role, provider_id, model, thinking)| {
            (
                (*role).to_string(),
                ModelRef {
                    provider_id: (*provider_id).to_string(),
                    model: (*model).to_string(),
                    effort: thinking.and_then(Effort::from_omp_thinking),
                    ext: None,
                },
            )
        })
        .collect();
    AgentBinding {
        entries,
        active_index,
        roles: map,
        settings: None,
    }
}

fn role_of(config_path: &Path, role: &str) -> Option<String> {
    let config: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
    config
        .get("modelRoles")?
        .get(role)?
        .as_str()
        .map(str::to_string)
}

#[test]
fn omp_writes_every_assigned_role_with_thinking_suffix() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");

    let providers = vec![flat("aaaa1111", "alpha"), flat("bbbb2222", "beta")];
    // The classic OMP setup: cheap default, cheaper smol for sub-agent fan-out,
    // a reasoning model for slow/plan.
    let binding = omp_binding_with_roles(
        vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        0,
        &[
            ("default", "aaaa1111", "model-a", None),
            ("smol", "bbbb2222", "model-b", None),
            ("slow", "bbbb2222", "model-b", Some("xhigh")),
            ("plan", "aaaa1111", "model-b", Some("max")),
        ],
    );

    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    assert_eq!(
        role_of(&config_path, "default").as_deref(),
        Some("skillstar_aaaa1111/model-a")
    );
    assert_eq!(
        role_of(&config_path, "smol").as_deref(),
        Some("skillstar_bbbb2222/model-b")
    );
    // Thinking level is appended as OMP's `:level` suffix.
    assert_eq!(
        role_of(&config_path, "slow").as_deref(),
        Some("skillstar_bbbb2222/model-b:xhigh")
    );
    assert_eq!(
        role_of(&config_path, "plan").as_deref(),
        Some("skillstar_aaaa1111/model-b:max")
    );
}

#[test]
fn omp_role_default_falls_back_to_active_entry() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");

    let providers = vec![flat("aaaa1111", "alpha"), flat("bbbb2222", "beta")];
    // Only `smol` is assigned — `default` still tracks the active entry, which is
    // exactly how the binding behaved before roles existed.
    let binding = omp_binding_with_roles(
        vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        1,
        &[("smol", "aaaa1111", "model-a", None)],
    );

    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    assert_eq!(
        role_of(&config_path, "default").as_deref(),
        Some("skillstar_bbbb2222/model-b")
    );
    assert_eq!(
        role_of(&config_path, "smol").as_deref(),
        Some("skillstar_aaaa1111/model-a")
    );
}

#[test]
fn omp_declares_role_models_missing_from_the_provider_catalogue() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");

    // A relay provider with an empty catalogue: the user types the model by hand,
    // so the role names a model neither `entry.model` nor `provider.models` has.
    let mut relay = flat("aaaa1111", "alpha");
    relay.models = Vec::new();
    relay.default_model = None;
    let providers = vec![relay];

    let binding = omp_binding_with_roles(
        vec![entry("aaaa1111", "model-a")],
        0,
        &[
            ("default", "aaaa1111", "model-a", None),
            ("slow", "aaaa1111", "hand-typed-reasoner", Some("xhigh")),
        ],
    );

    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&models_path).unwrap()).unwrap();
    let ids: Vec<String> = parsed
        .get("providers")
        .unwrap()
        .get("skillstar_aaaa1111")
        .unwrap()
        .get("models")
        .unwrap()
        .as_sequence()
        .unwrap()
        .iter()
        .map(|m| m.get("id").unwrap().as_str().unwrap().to_string())
        .collect();

    // Both the bound model and the role-only model must be declared, otherwise
    // `slow` would point at a model OMP cannot resolve.
    assert!(ids.contains(&"model-a".to_string()), "got {ids:?}");
    assert!(
        ids.contains(&"hand-typed-reasoner".to_string()),
        "got {ids:?}"
    );
    assert_eq!(
        role_of(&config_path, "slow").as_deref(),
        Some("skillstar_aaaa1111/hand-typed-reasoner:xhigh")
    );
}

#[test]
fn omp_skips_roles_that_would_dangle() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");

    let mut urlless = flat("cccc3333", "gamma");
    urlless.endpoints.openai_chat = None;
    let providers = vec![flat("aaaa1111", "alpha"), urlless];

    let binding = omp_binding_with_roles(
        vec![entry("aaaa1111", "model-a"), entry("cccc3333", "model-a")],
        0,
        &[
            ("default", "aaaa1111", "model-a", None),
            // Provider is bound but has no OpenAI base URL, so it never reaches
            // models.yml — the role must not point at a missing block.
            ("smol", "cccc3333", "model-a", None),
            // Provider is not bound to OMP at all.
            ("slow", "dddd4444", "model-a", None),
            // Assigned provider, but no model chosen yet.
            ("plan", "aaaa1111", "   ", None),
            // Role name collides with OMP's `@role` alias grammar.
            ("@smol", "aaaa1111", "model-a", None),
            // Role name would corrupt the `provider/model` grammar.
            ("bad/name", "aaaa1111", "model-a", None),
        ],
    );

    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    assert_eq!(
        role_of(&config_path, "default").as_deref(),
        Some("skillstar_aaaa1111/model-a")
    );
    for role in ["smol", "slow", "plan", "@smol", "bad/name"] {
        assert_eq!(
            role_of(&config_path, role),
            None,
            "role {role} must be skipped"
        );
    }
}

#[test]
fn omp_unassigning_a_role_removes_it_but_keeps_user_roles() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");

    let providers = vec![flat("aaaa1111", "alpha")];
    let entries = vec![entry("aaaa1111", "model-a")];

    // First sync assigns smol; a hand-written role on the user's own provider
    // sits alongside it.
    std::fs::write(
        &config_path,
        "modelRoles:\n  vision: anthropic/claude-opus-5\ntheme:\n  light: light\n",
    )
    .unwrap();
    let with_smol = omp_binding_with_roles(
        entries.clone(),
        0,
        &[
            ("default", "aaaa1111", "model-a", None),
            ("smol", "aaaa1111", "model-b", None),
        ],
    );
    sync_omp_binding_inner(&with_smol, &providers, &models_path, &config_path).unwrap();
    assert_eq!(
        role_of(&config_path, "smol").as_deref(),
        Some("skillstar_aaaa1111/model-b")
    );

    // Second sync drops smol — the stale managed pointer must go with it.
    let without_smol =
        omp_binding_with_roles(entries, 0, &[("default", "aaaa1111", "model-a", None)]);
    sync_omp_binding_inner(&without_smol, &providers, &models_path, &config_path).unwrap();

    assert_eq!(role_of(&config_path, "smol"), None);
    assert_eq!(
        role_of(&config_path, "default").as_deref(),
        Some("skillstar_aaaa1111/model-a")
    );
    // The user's own role and unrelated settings are untouched.
    assert_eq!(
        role_of(&config_path, "vision").as_deref(),
        Some("anthropic/claude-opus-5")
    );
    let config: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(
        config
            .get("theme")
            .unwrap()
            .get("light")
            .unwrap()
            .as_str()
            .unwrap(),
        "light"
    );
}

#[test]
fn omp_unsync_removes_every_managed_role() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");

    let providers = vec![flat("aaaa1111", "alpha")];
    let binding = omp_binding_with_roles(
        vec![entry("aaaa1111", "model-a")],
        0,
        &[
            ("default", "aaaa1111", "model-a", None),
            ("smol", "aaaa1111", "model-b", None),
            ("slow", "aaaa1111", "model-b", Some("xhigh")),
        ],
    );
    std::fs::write(
        &config_path,
        "modelRoles:\n  plan: aiproxy/gpt-5.6-sol:max\n",
    )
    .unwrap();
    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    unsync_omp_all_at(&models_path, &config_path).unwrap();

    for role in ["default", "smol", "slow"] {
        assert_eq!(
            role_of(&config_path, role),
            None,
            "managed role {role} must be cleared"
        );
    }
    // A role on the user's own provider survives unsync.
    assert_eq!(
        role_of(&config_path, "plan").as_deref(),
        Some("aiproxy/gpt-5.6-sol:max")
    );
}

#[test]
fn an_unknown_thinking_level_never_reaches_the_role_value() {
    // v3 stored the level as a free string and filtered it at write time. v4
    // parses it into `Effort` at the boundary, so an unwritable value cannot be
    // held in the first place — the filtering moved from the writer to the type.
    assert_eq!(Effort::from_omp_thinking("turbo"), None);

    let target = ModelRef {
        provider_id: "aaaa1111".to_string(),
        model: "model-a".to_string(),
        effort: Effort::from_omp_thinking("turbo"),
        ext: None,
    };
    assert_eq!(
        omp_role_value(&target, "skillstar_aaaa1111").as_deref(),
        Some("skillstar_aaaa1111/model-a")
    );

    let valid = ModelRef {
        effort: Effort::from_omp_thinking("xhigh"),
        ..target.clone()
    };
    assert_eq!(
        omp_role_value(&valid, "skillstar_aaaa1111").as_deref(),
        Some("skillstar_aaaa1111/model-a:xhigh")
    );

    let modelless = ModelRef {
        model: String::new(),
        ..target
    };
    assert_eq!(
        omp_role_value(&modelless, "skillstar_aaaa1111"),
        None,
        "an incomplete role must not overwrite what the user has on disk"
    );
}

/// End-to-end: drive the real writer, then let the real `omp` binary validate
/// what it produced.
///
/// Ignored by default because it needs `omp` on PATH (it is not present in CI).
/// Run it after touching the OMP writer or bumping the OMP version:
///
/// ```text
/// cargo test -p skillstar-models omp_output_is_accepted_by_the_real_binary -- --ignored --nocapture
/// ```
///
/// `omp models --json` is OMP's own models.yml linter: schema failures go to
/// stderr as `models.yml validation failed` while the model list goes to stdout.
/// Base URLs point at a closed local port so nothing can leave the machine.
#[test]
#[ignore = "requires the omp binary on PATH"]
fn omp_output_is_accepted_by_the_real_binary() {
    let tmp = TempDir::new().unwrap();
    let agent_dir = tmp.path().join(".omp").join("agent");
    std::fs::create_dir_all(&agent_dir).unwrap();
    let models_path = agent_dir.join("models.yml");
    let config_path = agent_dir.join("config.yml");

    let mut alpha = flat("aaaa1111", "alpha");
    alpha.endpoints.openai_chat = Some("http://127.0.0.1:9/v1".to_string());
    let mut beta = flat("bbbb2222", "beta");
    beta.endpoints.openai_chat = Some("http://127.0.0.1:9/v1".to_string());
    let providers = vec![alpha, beta];

    let binding = omp_binding_with_roles(
        vec![entry("aaaa1111", "model-a"), entry("bbbb2222", "model-b")],
        0,
        &[
            ("default", "aaaa1111", "model-a", None),
            ("smol", "bbbb2222", "model-b", None),
            ("slow", "aaaa1111", "model-b", Some("xhigh")),
            ("plan", "aaaa1111", "model-b", Some("max")),
        ],
    );
    sync_omp_binding_inner(&binding, &providers, &models_path, &config_path).unwrap();

    let out = std::process::Command::new("omp")
        .args(["models", "--json"])
        .env("HOME", tmp.path())
        .output()
        .expect("omp must be on PATH for this test");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        !stderr.contains("validation failed"),
        "omp rejected the generated models.yml:\n{stderr}"
    );
    // Every managed model must round-trip through OMP's own registry.
    for expected in [
        "skillstar_aaaa1111",
        "skillstar_bbbb2222",
        "model-a",
        "model-b",
    ] {
        assert!(
            stdout.contains(expected),
            "omp did not resolve {expected}; stdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }

    // Roles must read back verbatim through OMP's own settings reader.
    let roles = std::process::Command::new("omp")
        .args(["config", "get", "modelRoles", "--json"])
        .env("HOME", tmp.path())
        .output()
        .expect("omp config get must run");
    let roles = String::from_utf8_lossy(&roles.stdout);
    assert!(
        roles.contains("skillstar_aaaa1111/model-a"),
        "default role missing: {roles}"
    );
    assert!(
        roles.contains("skillstar_bbbb2222/model-b"),
        "smol role missing: {roles}"
    );
    assert!(
        roles.contains("skillstar_aaaa1111/model-b:xhigh"),
        "slow thinking suffix missing: {roles}"
    );
    assert!(
        roles.contains("skillstar_aaaa1111/model-b:max"),
        "plan thinking suffix missing: {roles}"
    );
}

#[test]
fn omp_role_names_are_validated() {
    for good in ["default", "smol", "slow", "plan", "my-role", "my_role2"] {
        assert!(is_valid_omp_role_name(good), "{good} should be valid");
    }
    for bad in ["", "@smol", "a/b", "with space", "emoji🙂"] {
        assert!(!is_valid_omp_role_name(bad), "{bad} should be rejected");
    }
}

/// Cross-language interlock, the Rust half.
///
/// Mirrors `src/features/models/lib/__tests__/ompRoles.test.ts`, which pins the
/// same two lists on the frontend. Both sides must be edited together — without
/// this test the interlock would only fire in one direction. The lists
/// themselves mirror OMP's `MODEL_ROLE_IDS` (`src/config/model-roles.ts`) and
/// `ThinkingLevel` (`@oh-my-pi/pi-agent-core/src/thinking.ts`) plus the
/// coding-agent-only `auto` sentinel.
#[test]
fn omp_role_and_thinking_registries_match_the_frontend() {
    assert_eq!(
        OMP_MODEL_ROLES,
        [
            "default", "smol", "slow", "plan", "vision", "designer", "commit", "tiny", "task",
            "advisor"
        ]
    );
    assert_eq!(
        OMP_THINKING_LEVELS,
        [
            "inherit", "off", "minimal", "low", "medium", "high", "xhigh", "max", "auto"
        ]
    );
    // Every built-in role must be writable; the validator is what stands between
    // a role name and OMP's `provider/model` + `@alias` grammar.
    for role in OMP_MODEL_ROLES {
        assert!(
            is_valid_omp_role_name(role),
            "built-in role {role} must be writable"
        );
    }
}

#[test]
fn omp_unsync_leaves_user_owned_default_pointer_alone() {
    let tmp = TempDir::new().unwrap();
    let models_path = tmp.path().join("models.yml");
    let config_path = tmp.path().join("config.yml");
    std::fs::write(
        &config_path,
        "modelRoles:\n  default: opencode-go/deepseek-v4-flash:xhigh\n",
    )
    .unwrap();

    unsync_omp_all_at(&models_path, &config_path).unwrap();

    let config: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    assert_eq!(
        config
            .get("modelRoles")
            .unwrap()
            .get("default")
            .unwrap()
            .as_str()
            .unwrap(),
        "opencode-go/deepseek-v4-flash:xhigh"
    );
}

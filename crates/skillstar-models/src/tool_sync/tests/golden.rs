//! The v4 writers must produce the same bytes v3 did.
//!
//! ## What these fixtures are
//!
//! `golden_v3/` holds real output, captured by running the **v3** writers at
//! commit `0fab682` against the fixture built in [`v3_fixture`]. They are not
//! hand-written expectations, which is the whole point: a hand-written
//! expectation records what the author believed the old code did, and the port
//! is only trustworthy against what it actually did.
//!
//! ## What is being asserted
//!
//! The same fixture is fed through the real v3 → v4 migration and then through
//! the v4 writers, and the resulting files are compared byte for byte. That
//! makes the claim end-to-end: not "the new writer produces plausible output"
//! but "a user's existing configuration, migrated, still lands on disk
//! unchanged" — which is the promise a format change has to keep.
//!
//! ## Codex is the one exception, and it is deliberate
//!
//! Codex's output *must* change: v3 wrote `wire_api = "chat"` for every
//! third-party host, and that value no longer exists in Codex's enum, so a
//! `config.toml` containing it fails to parse and Codex does not start. The
//! comparison is therefore kept for the one case where nothing should change —
//! an `api.openai.com` provider, which v3 already wrote as `responses` — and
//! the changed cases are asserted separately, by name, in [`super::part6`].

use super::*;
use crate::providers::migrate::migrate_v3_to_v4;
use crate::providers::{
    AgentBinding, FlatProvidersStore, Provider, ProviderEntryFlat, ToolActivation, ToolBinding,
};

fn golden(name: &str) -> String {
    let raw = match name {
        "claude-settings.json" => include_str!("golden_v3/claude-settings.json"),
        "opencode.json" => include_str!("golden_v3/opencode.json"),
        "pi-models.json" => include_str!("golden_v3/pi-models.json"),
        "pi-settings.json" => include_str!("golden_v3/pi-settings.json"),
        "omp-models.yml" => include_str!("golden_v3/omp-models.yml"),
        "omp-config.yml" => include_str!("golden_v3/omp-config.yml"),
        "codex-config.toml" => include_str!("golden_v3/codex-config.toml"),
        other => panic!("no golden fixture named {other}"),
    };
    raw.to_string()
}

/// The v3 provider row the goldens were captured from. Byte-for-byte the same
/// builder as the capture run — if this drifts, the comparison stops meaning
/// anything, so it is written out in full rather than derived from a helper.
fn relay(id: &str, name: &str, anthropic: bool) -> ProviderEntryFlat {
    ProviderEntryFlat {
        id: id.to_string(),
        name: name.to_string(),
        base_url_openai: format!("https://{name}.example.com/v1"),
        base_url_anthropic: if anthropic {
            format!("https://{name}.example.com/anthropic")
        } else {
            String::new()
        },
        models_url: format!("https://{name}.example.com/v1/models"),
        api_key: format!("sk-{id}-secret"),
        models: vec!["model-a".to_string(), "model-b".to_string()],
        default_model: "model-a".to_string(),
        sort_index: 0,
        preset_id: None,
        icon_color: Some("#123456".to_string()),
        notes: Some("note".to_string()),
        created_at: Some(1_719_000_000_000),
        meta: Some(serde_json::json!({
            "claude_haiku_model": "haiku-fast",
            "claude_sonnet_model": "sonnet-mid",
            "claude_opus_model": "opus-deep",
            "model_catalog": [
                { "id": "model-a", "display_name": "Model A", "context_length": 128000,
                  "max_completion_tokens": 8192, "cost": { "input": 1.0, "output": 2.0 } },
                { "id": "model-b", "source_name": "vendor/model-b" }
            ],
            "custom_thing": { "keep": true }
        })),
        codex_wire_api: "chat".to_string(),
        codex_auth_mode: "third_party".to_string(),
    }
}

fn v3_entry(provider_id: &str, model: &str) -> ToolActivation {
    ToolActivation {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        settings: None,
        last_sync_at: Some(1_719_000_000),
    }
}

/// The v3 store the goldens were captured from, as a whole.
fn v3_fixture() -> FlatProvidersStore {
    let p1 = relay("aaaa1111-x", "alpha", true);
    let p2 = relay("bbbb2222-y", "beta", false);

    let mut multi = ToolBinding::default();
    multi.entries = vec![v3_entry(&p1.id, "model-a"), v3_entry(&p2.id, "model-b")];
    multi.active_index = 1;

    let mut claude = ToolBinding::default();
    claude.entries = vec![v3_entry(&p1.id, "model-a")];

    let mut omp = multi.clone();
    omp.settings = Some(serde_json::json!({
        "roles": {
            "default": { "provider_id": p1.id, "model": "model-a", "thinking": "high" },
            "smol":    { "provider_id": p2.id, "model": "model-c", "thinking": "low" },
            "plan":    { "provider_id": p1.id, "model": "model-b" }
        }
    }));

    let mut tool_activations = std::collections::HashMap::new();
    tool_activations.insert("claude-code".to_string(), claude);
    tool_activations.insert("opencode".to_string(), multi.clone());
    tool_activations.insert("pi".to_string(), multi);
    tool_activations.insert("omp".to_string(), omp);

    FlatProvidersStore {
        version: 3,
        providers: vec![p1, p2],
        tool_activations,
    }
}

/// Migrate the fixture and hand back the pieces the writers need.
///
/// The catalogs are written to the cache here rather than being passed in,
/// because that is what production does: the migration lifts them out of the
/// store and the OpenCode writer reads them back from the cache.
///
/// The unused `DataDirSandbox` parameter is the point. This function writes
/// files, and without an installed override it writes them into the
/// developer's real `~/.skillstar/cache` — which it did, until a test that had
/// not taken a guard ran first. Taking the guard by reference makes that
/// impossible to express rather than merely discouraged.
fn migrated(
    _cache: &DataDirSandbox,
) -> (Vec<Provider>, std::collections::HashMap<String, AgentBinding>) {
    let outcome = migrate_v3_to_v4(v3_fixture(), &crate::providers::get_all_presets_flat());
    for catalog in &outcome.catalogs {
        let entries: Vec<crate::providers::ModelCatalogEntry> =
            serde_json::from_value(catalog.raw.clone()).expect("catalog shape");
        crate::providers::catalog_cache::write_catalog(&catalog.provider_id, &entries)
            .expect("write catalog to the sandbox cache");
    }
    (outcome.store.providers, outcome.store.bindings)
}

#[test]
fn claude_code_writes_the_same_bytes_as_v3() {
    let cache = DataDirSandbox::new();
    let (providers, bindings) = migrated(&cache);
    let binding = &bindings["claude-code"];
    let provider = &providers[0];

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, "{\n  \"env\": { \"USER_KEY\": \"keep\" }\n}\n").unwrap();

    sync_to_claude_code_inner(provider, "model-a", &binding.roles, &path).unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        golden("claude-settings.json"),
        "the tier models moved from provider.meta to binding.roles; the env block must not notice"
    );
}

#[test]
fn opencode_writes_the_same_bytes_as_v3() {
    let cache = DataDirSandbox::new();
    let (providers, bindings) = migrated(&cache);

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("opencode.json");
    sync_opencode_binding_inner(&bindings["opencode"], &providers, &path).unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        golden("opencode.json"),
        "the model catalog moved out of the store into the cache; the blocks must be identical"
    );
}

#[test]
fn pi_writes_the_same_bytes_as_v3() {
    let cache = DataDirSandbox::new();
    let (providers, bindings) = migrated(&cache);

    let tmp = TempDir::new().unwrap();
    let models = tmp.path().join("models.json");
    let settings = tmp.path().join("settings.json");
    sync_pi_binding_inner(&bindings["pi"], &providers, &models, &settings).unwrap();

    assert_eq!(
        std::fs::read_to_string(&models).unwrap(),
        golden("pi-models.json")
    );
    assert_eq!(
        std::fs::read_to_string(&settings).unwrap(),
        golden("pi-settings.json")
    );
}

#[test]
fn omp_writes_the_same_bytes_as_v3() {
    let cache = DataDirSandbox::new();
    let (providers, bindings) = migrated(&cache);

    let tmp = TempDir::new().unwrap();
    let models = tmp.path().join("models.yml");
    let config = tmp.path().join("config.yml");
    sync_omp_binding_inner(&bindings["omp"], &providers, &models, &config).unwrap();

    assert_eq!(
        std::fs::read_to_string(&models).unwrap(),
        golden("omp-models.yml")
    );
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        golden("omp-config.yml"),
        "roles moved from an OMP-private settings bag to AgentBinding.roles; \
         the `provider/model:level` values must be unchanged"
    );
}

#[test]
fn codex_writes_the_same_bytes_as_v3_for_a_host_that_already_spoke_responses() {
    // The one Codex case where nothing is supposed to change. Everything else
    // about Codex changed on purpose — see part6.
    let mut openai = relay("cccc3333-z", "openai", false);
    openai.base_url_openai = "https://api.openai.com/v1".to_string();
    openai.codex_wire_api = "responses".to_string();
    openai.codex_auth_mode = "api_key".to_string();
    let id = openai.id.clone();

    let mut binding = ToolBinding::default();
    binding.entries = vec![v3_entry(&id, "gpt-5.4")];
    let mut tool_activations = std::collections::HashMap::new();
    tool_activations.insert("codex".to_string(), binding);

    let outcome = migrate_v3_to_v4(
        FlatProvidersStore {
            version: 3,
            providers: vec![openai],
            tool_activations,
        },
        &crate::providers::get_all_presets_flat(),
    );

    let _home = use_sandbox_home();
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    sync_codex_binding_inner(
        &outcome.store.bindings["codex"],
        &outcome.store.providers,
        &path,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        golden("codex-config.toml"),
        "an api.openai.com provider was already written as `responses`; it must be untouched"
    );
}

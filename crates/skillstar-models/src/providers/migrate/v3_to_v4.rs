//! The v3 → v4 migration, as a pure function.
//!
//! ## Why pure
//!
//! No I/O, no clock, no network. Three reasons, in order of how much they cost
//! when violated:
//!
//! 1. It can be exhaustively tested, including with proptest, which is the only
//!    way to gain real confidence that no field of a user's configuration is
//!    dropped.
//! 2. It can be run twice — once to build the report the UI shows *before*
//!    committing, once to actually write — without side effects.
//! 3. It works offline. Migration runs at startup; a migration that wants the
//!    network is a migration that fails on a plane.
//!
//! The catalog externalisation (v3's `meta.model_catalog` → a cache file) needs
//! to write files, so this function only *extracts* the rows and hands them
//! back; the caller does the writing and is allowed to fail at it without
//! aborting, because a catalog is refetchable and a binding is not.

use super::report::{
    BackfilledEndpoint, DropReason, DroppedBinding, ExternalizedCatalog, MigrationReport,
};
use crate::providers::binding::{AgentBinding, BindingEntry, ModelRef, ProvidersStoreV4, STORE_VERSION_V4};
use crate::providers::credential::{Credential, NoCredentialReason};
use crate::providers::presets::{CLAUDE_OFFICIAL_ID, CODEX_OFFICIAL_ID, ProviderPresetFlat};
use crate::providers::provider::{Endpoints, Provider, ProviderCaps, Tri};
use crate::providers::types::{FlatProvidersStore, ProviderEntryFlat};
use std::collections::BTreeMap;

/// The agent that lost its writer in v4. Its binding is reported, then dropped.
pub const PLANNED_AGENT_CLAUDE_DESKTOP: &str = "claude-desktop";

/// Canonical role ids that have cross-agent meaning. Everything else is kept
/// verbatim as an extra role — the on-disk schema is an open map precisely so
/// that a role nobody anticipated survives a migration.
pub const ROLE_DEFAULT: &str = "default";
pub const ROLE_FAST: &str = "fast";
pub const ROLE_PLAN: &str = "plan";
pub const ROLE_VISION: &str = "vision";
pub const ROLE_SUBAGENT: &str = "subagent";

/// Claude's tiered-model meta keys, in v3's spelling.
const META_CLAUDE_MAIN: &str = "claude_main_model";
const META_CLAUDE_HAIKU: &str = "claude_haiku_model";
const META_CLAUDE_SONNET: &str = "claude_sonnet_model";
const META_CLAUDE_OPUS: &str = "claude_opus_model";
const META_MODEL_CATALOG: &str = "model_catalog";
/// v1 leftover. Its only reader retired with the v1 store.
const META_LEGACY_BASE_URL: &str = "baseURL";

/// Catalog rows lifted out of one provider row, for the caller to persist.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedCatalog {
    pub provider_id: String,
    /// The raw v3 `meta.model_catalog` value, untouched. Converting it to
    /// [`crate::providers::catalog::ModelEntry`] needs the source chain that
    /// WP-2 owns; carrying the original through means that conversion can
    /// happen later without a second migration.
    pub raw: serde_json::Value,
    pub entry_count: u64,
}

/// The full result of migrating one v3 store.
#[derive(Debug, Clone, PartialEq)]
pub struct MigrationOutcome {
    pub store: ProvidersStoreV4,
    pub report: MigrationReport,
    /// Catalogs to write to the cache directory. Failing to write these is a
    /// warning, never an abort.
    pub catalogs: Vec<ExtractedCatalog>,
}

/// Migrate a v3 flat store to v4.
///
/// `presets` is passed in rather than read from the registry so the function
/// stays pure and so tests can drive the backfill rule with a fixture.
pub fn migrate_v3_to_v4(v3: FlatProvidersStore, presets: &[ProviderPresetFlat]) -> MigrationOutcome {
    let mut report = MigrationReport::default();
    let mut catalogs = Vec::new();
    let mut providers = Vec::with_capacity(v3.providers.len());
    // Per-provider values that step 1 finds but only step 3 can place.
    let mut claude_tiers: BTreeMap<String, ClaudeTierModels> = BTreeMap::new();
    let mut codex_auth_modes: BTreeMap<String, String> = BTreeMap::new();

    for entry in v3.providers {
        let (provider, carried) = migrate_provider(entry, presets, &mut report);
        if let Some(catalog) = carried.catalog {
            report.externalized_catalogs.push(ExternalizedCatalog {
                provider_id: provider.id.clone(),
                entry_count: catalog.entry_count,
            });
            catalogs.push(catalog);
        }
        if !carried.claude_tiers.is_empty() {
            claude_tiers.insert(provider.id.clone(), carried.claude_tiers);
        }
        if let Some(mode) = carried.codex_auth_mode {
            codex_auth_modes.insert(provider.id.clone(), mode);
        }
        providers.push(provider);
    }

    let bindings = migrate_bindings(
        v3.tool_activations,
        &providers,
        &claude_tiers,
        &codex_auth_modes,
        &mut report,
    );

    report.preserved_ext_keys.sort();
    report.preserved_ext_keys.dedup();

    MigrationOutcome {
        store: ProvidersStoreV4 {
            version: STORE_VERSION_V4,
            providers,
            bindings,
        },
        report,
        catalogs,
    }
}

/// Claude's tiered models as they sat on the v3 provider row.
#[derive(Debug, Clone, Default, PartialEq)]
struct ClaudeTierModels {
    main: Option<String>,
    haiku: Option<String>,
    sonnet: Option<String>,
    opus: Option<String>,
}

impl ClaudeTierModels {
    fn is_empty(&self) -> bool {
        self.main.is_none() && self.haiku.is_none() && self.sonnet.is_none() && self.opus.is_none()
    }
}

/// Values pulled off a v3 row that belong to a later step.
#[derive(Debug, Default)]
struct CarriedForward {
    catalog: Option<ExtractedCatalog>,
    claude_tiers: ClaudeTierModels,
    codex_auth_mode: Option<String>,
}

fn migrate_provider(
    entry: ProviderEntryFlat,
    presets: &[ProviderPresetFlat],
    report: &mut MigrationReport,
) -> (Provider, CarriedForward) {
    // Destructured exhaustively on purpose: adding a field to the v3 type must
    // become a compile error here, not a field that quietly stops migrating.
    let ProviderEntryFlat {
        id,
        name,
        base_url_openai,
        base_url_anthropic,
        models_url,
        api_key,
        models,
        default_model,
        sort_index,
        preset_id,
        icon_color,
        notes,
        created_at,
        meta,
        codex_wire_api: _discarded_wire,
        codex_auth_mode,
    } = entry;

    let preset = preset_id
        .as_deref()
        .and_then(|pid| presets.iter().find(|p| p.id == pid));

    // Step 5, done inline: the endpoint backfill. Reading the three conditions
    // here — where both the row and its preset are in hand — keeps the rule in
    // one place instead of requiring a second pass over the store.
    let mut anthropic = non_empty(&base_url_anthropic);
    if let Some(preset) = preset
        && should_backfill(&base_url_anthropic, &preset.base_url_anthropic, &base_url_openai, &preset.base_url_openai)
    {
        anthropic = Some(preset.base_url_anthropic.clone());
        report.backfilled_anthropic.push(BackfilledEndpoint {
            provider_id: id.clone(),
            provider_name: name.clone(),
            preset_id: preset.id.clone(),
            url: preset.base_url_anthropic.clone(),
        });
    }

    let mut models_list = non_empty(&models_url);
    if let Some(preset) = preset
        && should_backfill(&models_url, &preset.models_url, &base_url_openai, &preset.base_url_openai)
    {
        models_list = Some(preset.models_url.clone());
        report.backfilled_models_list.push(BackfilledEndpoint {
            provider_id: id.clone(),
            provider_name: name.clone(),
            preset_id: preset.id.clone(),
            url: preset.models_url.clone(),
        });
    }

    let (openai_responses, responses_cap) = derive_responses(&base_url_openai);

    let mut carried = CarriedForward::default();
    let ext = split_meta(meta, &id, &mut carried, report);

    let credential = derive_credential(&id, &api_key, &base_url_openai, &base_url_anthropic);
    carried.codex_auth_mode = Some(codex_auth_mode);

    let provider = Provider {
        id,
        name,
        preset_id,
        endpoints: Endpoints {
            openai_chat: non_empty(&base_url_openai),
            openai_responses,
            anthropic_messages: anthropic,
            models_list,
        },
        credential,
        headers: BTreeMap::new(),
        caps: ProviderCaps {
            responses_api: responses_cap,
            // Never `No`. A v3 store carries no probe results, and recording
            // "never probed" as "unsupported" would disable working bindings
            // on upgrade with no obvious route back.
            anthropic_messages: Tri::Unknown,
            models_list: Tri::Unknown,
            ..ProviderCaps::unknown()
        },
        models,
        default_model: non_empty(&default_model),
        sort_index,
        icon_color,
        notes,
        // v3 already stored this one in milliseconds; only the name changes.
        created_at_ms: created_at,
        ext,
    };

    (provider, carried)
}

/// The three-condition backfill rule.
///
/// Condition ③ — that the OpenAI URL still matches the preset verbatim — is the
/// one that carries the argument. It separates "empty because a frontend bug
/// never wrote it" (the row is otherwise untouched, so filling it in restores
/// what the user asked for) from "empty because the user cleared it" (they
/// edited this row, so it is theirs and filling it in overrides a decision).
/// Without ③ the migration would overwrite deliberate configuration; without
/// the backfill at all, those rows can never bind to Claude.
fn should_backfill(current: &str, preset_value: &str, current_openai: &str, preset_openai: &str) -> bool {
    current.trim().is_empty()
        && !preset_value.trim().is_empty()
        && current_openai == preset_openai
}

/// `openai_responses` and the matching capability bit.
///
/// Only `api.openai.com` is known to speak the Responses API without probing.
/// Everything else is `Unknown` — see the note on [`ProviderCaps`].
fn derive_responses(base_url_openai: &str) -> (Option<String>, Tri) {
    if base_url_openai.contains("api.openai.com") {
        (Some(base_url_openai.to_string()), Tri::Yes)
    } else {
        (None, Tri::Unknown)
    }
}

fn derive_credential(
    id: &str,
    api_key: &str,
    base_url_openai: &str,
    base_url_anthropic: &str,
) -> Credential {
    if id == CLAUDE_OFFICIAL_ID {
        return Credential::ExternalCli {
            surface: "claude".to_string(),
        };
    }
    if id == CODEX_OFFICIAL_ID {
        return Credential::ExternalCli {
            surface: "codex".to_string(),
        };
    }
    if api_key.trim().is_empty() {
        let no_endpoints = base_url_openai.trim().is_empty() && base_url_anthropic.trim().is_empty();
        return Credential::None {
            reason: if no_endpoints {
                NoCredentialReason::NativeLogin
            } else {
                NoCredentialReason::LocalService
            },
        };
    }
    Credential::single_key(uuid::Uuid::new_v4().to_string(), api_key)
}

/// Split v3's `meta` into the fields that got promoted and the remainder.
///
/// Unrecognised keys go to `ext` verbatim rather than being dropped: `meta` was
/// schema-free for three versions, so what is in there cannot be enumerated
/// from the type, only observed.
fn split_meta(
    meta: Option<serde_json::Value>,
    provider_id: &str,
    carried: &mut CarriedForward,
    report: &mut MigrationReport,
) -> Option<serde_json::Value> {
    let Some(serde_json::Value::Object(map)) = meta else {
        return None;
    };

    let mut rest = serde_json::Map::new();
    for (key, value) in map {
        match key.as_str() {
            META_CLAUDE_MAIN => carried.claude_tiers.main = value_as_model(&value),
            META_CLAUDE_HAIKU => carried.claude_tiers.haiku = value_as_model(&value),
            META_CLAUDE_SONNET => carried.claude_tiers.sonnet = value_as_model(&value),
            META_CLAUDE_OPUS => carried.claude_tiers.opus = value_as_model(&value),
            META_MODEL_CATALOG => {
                let entry_count = value.as_array().map(|a| a.len() as u64).unwrap_or(0);
                carried.catalog = Some(ExtractedCatalog {
                    provider_id: provider_id.to_string(),
                    raw: value,
                    entry_count,
                });
            }
            META_LEGACY_BASE_URL => {}
            other => {
                report.preserved_ext_keys.push(other.to_string());
                rest.insert(key, value);
            }
        }
    }

    if rest.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(rest))
    }
}

fn value_as_model(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn non_empty(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ---------------------------------------------------------------------------
// Step 2 + 3: bindings and role convergence
// ---------------------------------------------------------------------------

fn migrate_bindings(
    v3_bindings: std::collections::HashMap<String, crate::providers::types::ToolBinding>,
    providers: &[Provider],
    claude_tiers: &BTreeMap<String, ClaudeTierModels>,
    codex_auth_modes: &BTreeMap<String, String>,
    report: &mut MigrationReport,
) -> std::collections::HashMap<String, AgentBinding> {
    let mut out = std::collections::HashMap::new();
    // Sorted so the report reads the same way every run — a report whose order
    // depends on hash iteration is one nobody can diff.
    let mut agent_ids: Vec<String> = v3_bindings.keys().cloned().collect();
    agent_ids.sort();

    for agent_id in agent_ids {
        let binding = v3_bindings.get(&agent_id).cloned().unwrap_or_default();

        if agent_id == PLANNED_AGENT_CLAUDE_DESKTOP {
            for entry in &binding.entries {
                report.dropped_bindings.push(DroppedBinding {
                    agent_id: agent_id.clone(),
                    provider_id: entry.provider_id.clone(),
                    provider_name: provider_name(providers, &entry.provider_id),
                    model: entry.model.clone(),
                    reason: DropReason::AgentPlannedNotImplemented,
                });
            }
            continue;
        }

        let mut roles = BTreeMap::new();
        let mut settings = binding.settings.clone();
        if let Some(value) = settings.as_mut() {
            roles = take_roles_from_settings(value);
        }
        // An object whose only key was `roles` becomes an empty bag; drop it
        // rather than writing `{}`.
        if settings
            .as_ref()
            .and_then(|v| v.as_object())
            .is_some_and(|m| m.is_empty())
        {
            settings = None;
        }

        let mut entries: Vec<BindingEntry> = binding
            .entries
            .iter()
            .map(|e| BindingEntry {
                provider_id: e.provider_id.clone(),
                model: e.model.clone(),
                settings: e.settings.clone(),
                // v3 stored seconds here while storing `created_at` in
                // milliseconds; this is where that inconsistency is paid off.
                last_sync_at_ms: e.last_sync_at.map(|s| s.saturating_mul(1000)),
            })
            .collect();

        if agent_id == "codex" {
            for entry in entries.iter_mut() {
                backfill_codex_auth_mode(entry, codex_auth_modes);
            }
        }

        if agent_id == "claude-code" {
            apply_claude_tiers(&mut roles, &entries, binding.active_index, claude_tiers);
        }

        out.insert(
            agent_id,
            AgentBinding {
                entries,
                active_index: binding.active_index,
                roles,
                settings,
            },
        );
    }

    out
}

fn provider_name(providers: &[Provider], provider_id: &str) -> String {
    providers
        .iter()
        .find(|p| p.id == provider_id)
        .map(|p| p.name.clone())
        .unwrap_or_default()
}

/// Lift `settings.roles` (v3's `OmpSettings`) into the typed role map.
///
/// The key mapping is written out rather than derived: OMP's names and the
/// canonical names overlap only partially, and a rule that guessed would map
/// `task` to something other than `subagent`.
fn take_roles_from_settings(settings: &mut serde_json::Value) -> BTreeMap<String, ModelRef> {
    let Some(object) = settings.as_object_mut() else {
        return BTreeMap::new();
    };
    let Some(raw_roles) = object.remove("roles") else {
        return BTreeMap::new();
    };
    let Some(role_map) = raw_roles.as_object() else {
        return BTreeMap::new();
    };

    let mut roles = BTreeMap::new();
    for (v3_key, target) in role_map {
        let canonical = canonical_role_key(v3_key);
        let provider_id = target
            .get("provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let model = target
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let effort = target
            .get("thinking")
            .and_then(|v| v.as_str())
            .and_then(crate::providers::binding::Effort::from_omp_thinking);
        roles.insert(
            canonical,
            ModelRef {
                provider_id,
                model,
                effort,
                ext: None,
            },
        );
    }
    roles
}

/// Map a v3 OMP role name to its canonical id.
///
/// Names with no canonical counterpart — `slow`, `designer`, `commit`, and any
/// user-invented key — are kept verbatim. `slow` in particular is not a role at
/// all in the v4 model (it is a reasoning tier), but discarding it during
/// migration would delete configuration the user typed, so it survives as an
/// extra role and the UI simply stops promoting it.
pub fn canonical_role_key(v3_key: &str) -> String {
    match v3_key {
        "default" => ROLE_DEFAULT.to_string(),
        "smol" => ROLE_FAST.to_string(),
        "plan" => ROLE_PLAN.to_string(),
        "vision" => ROLE_VISION.to_string(),
        "task" => ROLE_SUBAGENT.to_string(),
        other => other.to_string(),
    }
}

/// Fill a Codex entry's `auth_mode` from the provider row it used to sit on.
///
/// An entry that already carries its own `auth_mode` wins: per-entry settings
/// existed in v3 and were the more specific of the two.
fn backfill_codex_auth_mode(entry: &mut BindingEntry, modes: &BTreeMap<String, String>) {
    let Some(mode) = modes.get(&entry.provider_id) else {
        return;
    };
    let already_set = entry
        .settings
        .as_ref()
        .and_then(|s| s.get("auth_mode"))
        .is_some();
    if already_set {
        return;
    }
    let settings = entry
        .settings
        .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let Some(object) = settings.as_object_mut() {
        object.insert(
            "auth_mode".to_string(),
            serde_json::Value::String(mode.clone()),
        );
    }
}

/// Move Claude's tiered models off the provider row and onto the binding.
///
/// `sonnet` and `opus` are kept under their own names as extra roles rather
/// than being forced into the canonical five: they are Claude-specific tiers
/// with no cross-agent counterpart, and inventing one would make the role map
/// claim a generality it does not have.
fn apply_claude_tiers(
    roles: &mut BTreeMap<String, ModelRef>,
    entries: &[BindingEntry],
    active_index: usize,
    claude_tiers: &BTreeMap<String, ClaudeTierModels>,
) {
    if entries.is_empty() {
        return;
    }
    let idx = active_index.min(entries.len() - 1);
    let Some(active) = entries.get(idx) else {
        return;
    };
    let Some(tiers) = claude_tiers.get(&active.provider_id) else {
        return;
    };

    let mut put = |role: &str, model: &Option<String>| {
        if let Some(model) = model
            && !roles.contains_key(role)
        {
            roles.insert(role.to_string(), ModelRef::new(&active.provider_id, model));
        }
    };
    put(ROLE_FAST, &tiers.haiku);
    put("sonnet", &tiers.sonnet);
    put("opus", &tiers.opus);

    // `claude_main_model` only becomes the default role when the entry itself
    // does not name a model — otherwise the entry is the more direct statement
    // of intent and the meta field was a fallback.
    if active.model.trim().is_empty() {
        put(ROLE_DEFAULT, &tiers.main);
    }
}

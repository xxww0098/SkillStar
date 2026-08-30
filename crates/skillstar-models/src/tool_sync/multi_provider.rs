//! Multi-provider tool sync (Codex, OpenCode, Pi).
//!
//! The single-provider agent (Claude Code) writes one global env block, so its
//! writer lives in `sync.rs` and takes a single provider+model. The agents
//! handled here — Codex, OpenCode and Pi — natively support several providers
//! coexisting in one config file (Codex `[model_providers.*]`, OpenCode
//! `provider.*`, Pi `providers.*` in `models.json`), with a pointer selecting
//! the active one (Codex `model_provider`, OpenCode top-level `model`, Pi
//! `defaultProvider`/`defaultModel` in `settings.json`).
//!
//! These writers project an entire [`AgentBinding`] onto disk: one managed entry
//! per bound provider, keyed `skillstar_<id8>`, plus the active pointer. Every
//! managed key shares the `skillstar` prefix so unsync and conflict detection
//! can find them all regardless of how many providers are bound.

use super::*;

/// Prefix shared by every SkillStar-managed provider entry across Codex and
/// OpenCode. Unsync and conflict detection match on this prefix so they catch
/// both the legacy single `skillstar` key and the per-provider `skillstar_<id>`
/// keys written for multi-provider bindings.
pub const SKILLSTAR_MANAGED_PREFIX: &str = "skillstar";

/// Derive the managed config key for a provider entry: `skillstar_<id8>`, where
/// `<id8>` is the first 8 chars of the provider id, lowercased and reduced to
/// `[a-z0-9_]`. Mirrors [`codex_env_key_for`]'s prefix rule so a provider's
/// table key and env-var name stay correlated and collision-resistant.
pub fn skillstar_managed_key(provider_id: &str) -> String {
    let safe: String = provider_id
        .chars()
        .take(8)
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let safe = if safe.is_empty() {
        "provider".to_string()
    } else {
        safe
    };
    format!("{SKILLSTAR_MANAGED_PREFIX}_{safe}")
}

/// True if a config key is one SkillStar manages (legacy `skillstar` or any
/// `skillstar_*` per-provider key).
pub fn is_skillstar_managed_key(key: &str) -> bool {
    key == SKILLSTAR_MANAGED_PREFIX
        || key
            .strip_prefix(SKILLSTAR_MANAGED_PREFIX)
            .is_some_and(|rest| rest.starts_with('_'))
}

/// Resolve a binding's entries to `(provider, entry)` pairs in list order,
/// skipping entries whose provider id no longer exists in the store, and report
/// the active provider's id.
///
/// Returns `None` when no usable entry remains (the tool should be unsynced).
pub(crate) fn resolve_entries<'a>(
    binding: &'a AgentBinding,
    providers: &'a [Provider],
) -> Option<(Vec<(&'a Provider, &'a BindingEntry)>, String)> {
    let resolved: Vec<_> = binding
        .entries
        .iter()
        .filter_map(|entry| {
            providers
                .iter()
                .find(|p| p.id == entry.provider_id)
                .map(|p| (p, entry))
        })
        .collect();

    if resolved.is_empty() {
        return None;
    }

    // The active provider id, clamped through ToolBinding::active.
    let active_id = binding.active()?.provider_id.clone();
    // If the active entry's provider was filtered out, fall back to the first.
    let active_id = if resolved.iter().any(|(p, _)| p.id == active_id) {
        active_id
    } else {
        resolved[0].0.id.clone()
    };
    Some((resolved, active_id))
}

// ---------------------------------------------------------------------------
// Shared JSON managed-block skeleton (OpenCode, Pi)
// ---------------------------------------------------------------------------

/// Shared write skeleton for JSON multi-provider configs (OpenCode, Pi):
/// rolling backup → read-or-init root → drop stale `skillstar_*` blocks →
/// one managed block per bound provider (skipping entries without an
/// OpenAI-compatible URL, computing the active `(key, model)` pair) → let the
/// caller finalize the root (active selector, `$schema`, …) → persist.
///
/// Returns the backup path plus the active pointer when it resolved to a
/// non-empty model. Codex deliberately does NOT go through this skeleton: its
/// TOML document, `auth.json` side-channel and per-entry wire settings would
/// push the adapter surface past the writer it replaces (see
/// docs/decisions.md).
/// Active pointer for a managed binding: `(skillstar_<id8> key, model_id)`.
pub(crate) type ActivePointer = (String, String);

pub(crate) fn sync_json_blocks_inner(
    entries: &[(&Provider, &BindingEntry)],
    active_id: &str,
    config_path: &Path,
    blocks_key: &str,
    init_root: impl Fn() -> Value,
    build_block: impl Fn(&Provider, &str) -> Value,
    finish_root: impl FnOnce(&mut serde_json::Map<String, Value>, Option<&ActivePointer>),
) -> Result<(Option<PathBuf>, Option<ActivePointer>)> {
    let backup_path = if config_path.exists() {
        Some(create_rolling_backup(config_path)?)
    } else {
        None
    };
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let mut root: Value = match read_existing_config(config_path)? {
        Some(content) => serde_json::from_str(&content).with_context(|| {
            format!(
                "Failed to parse {} — fix or remove it before syncing",
                config_path.display()
            )
        })?,
        None => init_root(),
    };

    let file_label = config_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| config_path.display().to_string());
    let root_obj = root
        .as_object_mut()
        .with_context(|| format!("{file_label} root must be an object"))?;

    let provider_map = root_obj
        .entry(blocks_key)
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let provider_map = provider_map
        .as_object_mut()
        .with_context(|| format!("{file_label} `{blocks_key}` must be an object"))?;

    // Drop stale skillstar* blocks, then write one per current entry.
    provider_map.retain(|k, _| !is_skillstar_managed_key(k));
    let mut active_pointer: Option<(String, String)> = None;
    for (provider, entry) in entries {
        if openai_base(provider).trim().is_empty() {
            continue;
        }
        let key = skillstar_managed_key(&provider.id);
        let block = build_block(provider, &entry.model);
        if provider.id == active_id {
            let model_id = if entry.model.trim().is_empty() {
                default_model(provider).to_string()
            } else {
                entry.model.clone()
            };
            if !model_id.trim().is_empty() {
                active_pointer = Some((key.clone(), model_id));
            }
        }
        provider_map.insert(key, block);
    }

    finish_root(root_obj, active_pointer.as_ref());

    let output = serde_json::to_string_pretty(&root)
        .with_context(|| format!("Failed to serialize {file_label}"))?;
    skillstar_core::infra::fs_ops::atomic_write(config_path, output.as_bytes())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    Ok((backup_path, active_pointer))
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// Whether a provider can be written into Codex's config at all.
///
/// Two conditions, and they are not the same question:
///
/// - the host must expose a `/v1/responses` endpoint, because Codex ≥0.95
///   removed every other `WireApi` variant from its enum; and
/// - a probe must not have established that it does *not* speak it.
///
/// `Tri::Unknown` deliberately passes. Migration writes `Unknown` for every
/// row, so treating "never probed" as "unsupported" would unbind everyone on
/// upgrade — the endpoint's presence is what carries the decision, and the
/// capability bit only ever *removes* a host a probe has disproved.
pub fn codex_can_serve(provider: &Provider) -> bool {
    !provider.caps.responses_api.is_denied() && serves(provider, RequiredWire::OpenaiResponses)
}

/// The Codex settings for one entry, defaulting from the credential.
///
/// v3 fell back to two columns on the provider row (`codex_wire_api` /
/// `codex_auth_mode`) that applied to agents with no such concept. v4 keeps
/// `auth_mode` per entry, where it belongs, and derives the default from what
/// kind of credential the provider actually has: a key that lives in another
/// CLI's store must never be written to `auth.json`, and a literal third-party
/// key travels via `env_key` so a concurrent ChatGPT login survives.
pub(crate) fn codex_settings_for(provider: &Provider, entry: &BindingEntry) -> CodexSettings {
    let mut settings = entry
        .settings
        .as_ref()
        .map(CodexSettings::from_value)
        .unwrap_or_else(|| CodexSettings {
            auth_mode: default_codex_auth_mode(provider).to_string(),
        });
    if settings.auth_mode.trim().is_empty() {
        settings.auth_mode = default_codex_auth_mode(provider).to_string();
    }
    settings
}

/// The auth mode a provider implies when its entry does not name one.
fn default_codex_auth_mode(provider: &Provider) -> &'static str {
    if provider.is_external_cli() {
        // Credentials belong to the Codex CLI's own login; never touch them.
        CODEX_AUTH_MODE_OAUTH
    } else if serves(provider, RequiredWire::OpenaiResponses)
        && responses_base(provider).contains("api.openai.com")
    {
        CODEX_AUTH_MODE_API_KEY
    } else {
        CODEX_AUTH_MODE_THIRD_PARTY
    }
}

/// Write a whole Codex binding to `~/.codex/config.toml` (+ `auth.json`).
///
/// Each bound provider gets a `[model_providers.skillstar_<id>]` table; the
/// active entry drives top-level `model_provider` + `model`. `auth.json` is
/// written from the active entry only (Codex has a single `OPENAI_API_KEY`
/// slot); third-party entries carry their key via per-table `env_key`, so they
/// never depend on `auth.json`.
pub fn sync_codex_binding(
    binding: &AgentBinding,
    providers: &[Provider],
) -> Result<ToolSyncResultFlat> {
    let config_path = resolve_codex_config_path()?;
    Ok(ToolSyncResultFlat::from_write_outcome(
        "codex",
        &config_path,
        sync_codex_binding_inner(binding, providers, &config_path),
    ))
}

/// Path-taking core of [`sync_codex_binding`] — public so property tests can
/// drive the TOML merge against an isolated temp path instead of the shared
/// sandbox HOME. (`auth.json` still resolves through the sandboxable home; use
/// an OAuth/third-party auth mode to keep a test fully path-hermetic.)
pub fn sync_codex_binding_inner(
    binding: &AgentBinding,
    providers: &[Provider],
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    let (entries, active_id) = resolve_entries(binding, providers)
        .context("Codex binding has no resolvable provider entries")?;

    // Resolve the active entry + its settings for the auth.json decision and
    // the top-level pointer.
    let (active_provider, active_entry) = entries
        .iter()
        .find(|(p, _)| p.id == active_id)
        .copied()
        .context("active Codex entry not found after resolution")?;
    let active_settings = codex_settings_for(active_provider, active_entry);

    let official_active = active_provider.is_external_cli();

    // Official (ChatGPT OAuth): never require a Base URL and never touch auth.json.
    if !official_active && !codex_can_serve(active_provider) {
        bail!(
            "Provider '{}' has no /v1/responses endpoint; Codex >=0.95 speaks nothing else",
            active_provider.name
        );
    }

    let auth_path = resolve_codex_auth_path()?;
    let mut first_backup: Option<PathBuf> = None;

    // --- auth.json (active entry only; Official / oauth / third_party skip) ---
    if !official_active && !active_settings.preserves_oauth_token() {
        if auth_path.exists() {
            let backup = create_rolling_backup(&auth_path)?;
            first_backup.get_or_insert(backup);
        }
        if let Some(parent) = auth_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }
        let auth_fields: Vec<(&str, Value)> = vec![(
            "OPENAI_API_KEY",
            Value::String(api_key(active_provider).to_string()),
        )];
        merge_json_write(&auth_path, &auth_fields)?;
    }

    // --- config.toml ---
    if config_path.exists() {
        let backup = create_rolling_backup(config_path)?;
        first_backup.get_or_insert(backup);
    }
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let mut table: toml::Table = match read_existing_config(config_path)? {
        Some(content) => toml::from_str(&content).with_context(|| {
            format!(
                "Failed to parse {} — fix or remove it before syncing",
                config_path.display()
            )
        })?,
        None => toml::Table::new(),
    };

    // Official active → clear SkillStar top-level pointers so Codex uses native
    // ChatGPT login. Other bound (non-Official) providers keep their tables for
    // later switching.
    if official_active {
        if table
            .get("model_provider")
            .and_then(|v| v.as_str())
            .is_some_and(is_skillstar_managed_key)
        {
            table.remove("model_provider");
            table.remove("model");
        }
    } else {
        let active_key = skillstar_managed_key(&active_id);
        table.insert(
            "model_provider".to_string(),
            toml::Value::String(active_key.clone()),
        );
        table.insert(
            "model".to_string(),
            toml::Value::String(active_entry.model.clone()),
        );
    }

    // Rebuild the managed provider tables: drop every stale skillstar* table,
    // then write one per current non-Official entry (empty Official URLs skip).
    let mp = table
        .entry("model_providers")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !mp.is_table() {
        *mp = toml::Value::Table(toml::Table::new());
    }
    let mp_table = mp.as_table_mut().expect("model_providers is a table");
    mp_table.retain(|k, _| !is_skillstar_managed_key(k));

    for (provider, entry) in &entries {
        // The Codex fix, in one condition. A host with no `/v1/responses`
        // endpoint is *skipped*, not written with `wire_api = "chat"`: that
        // value no longer exists in Codex's enum, and a config.toml containing
        // it fails to deserialize — taking the whole file, and every other
        // provider in it, down with it.
        if provider.is_external_cli() || !codex_can_serve(provider) {
            continue;
        }
        let settings = codex_settings_for(provider, entry);
        let section = CodexModelProvider::from_binding(provider, &settings).to_toml_table();
        mp_table.insert(
            skillstar_managed_key(&provider.id),
            toml::Value::Table(section),
        );
    }

    if mp_table.is_empty() {
        table.remove("model_providers");
    }

    let output = toml::to_string_pretty(&table).context("Failed to serialize Codex config.toml")?;
    skillstar_core::infra::fs_ops::atomic_write(config_path, output.as_bytes())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    Ok(first_backup)
}

// ---------------------------------------------------------------------------
// OpenCode
// ---------------------------------------------------------------------------

/// Write a whole OpenCode binding to `opencode.json`.
///
/// Each bound provider becomes a `provider.skillstar_<id>` block; the active
/// entry sets the top-level `model = "skillstar_<id>/<model>"` selector.
pub fn sync_opencode_binding(
    binding: &AgentBinding,
    providers: &[Provider],
) -> Result<ToolSyncResultFlat> {
    let config_path = resolve_opencode_config_path()?;
    Ok(ToolSyncResultFlat::from_write_outcome(
        "opencode",
        &config_path,
        sync_opencode_binding_inner(binding, providers, &config_path),
    ))
}

pub(crate) fn sync_opencode_binding_inner(
    binding: &AgentBinding,
    providers: &[Provider],
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    let (entries, active_id) = resolve_entries(binding, providers)
        .context("OpenCode binding has no resolvable provider entries")?;

    let (backup_path, _) = sync_json_blocks_inner(
        &entries,
        &active_id,
        config_path,
        "provider",
        || serde_json::json!({ "$schema": "https://opencode.ai/config.json", "provider": {} }),
        build_opencode_provider_block,
        |root, active| {
            root.entry("$schema")
                .or_insert_with(|| Value::String("https://opencode.ai/config.json".to_string()));
            // The active entry sets the top-level `model` selector; when no
            // model resolved, any pre-existing selector is left untouched.
            if let Some((key, model_id)) = active {
                root.insert(
                    "model".to_string(),
                    Value::String(format!("{key}/{model_id}")),
                );
            }
        },
    )?;

    Ok(backup_path)
}

// ---------------------------------------------------------------------------
// Pi
// ---------------------------------------------------------------------------

/// Write a whole Pi binding to `~/.pi/agent/models.json` (+ `settings.json`).
///
/// Each bound provider becomes a `providers.skillstar_<id>` block
/// (`api: "openai-completions"`, plaintext `apiKey`, minimal `{ id }` model
/// entries so Pi's own defaults apply); the active entry drives
/// `defaultProvider` / `defaultModel` in `settings.json`.
pub fn sync_pi_binding(
    binding: &AgentBinding,
    providers: &[Provider],
) -> Result<ToolSyncResultFlat> {
    let config_path = resolve_pi_models_path()?;
    let settings_path = resolve_pi_settings_path()?;
    Ok(ToolSyncResultFlat::from_write_outcome(
        "pi",
        &config_path,
        sync_pi_binding_inner(binding, providers, &config_path, &settings_path),
    ))
}

/// Build one Pi provider block. Model entries carry only `id` — Pi supplies
/// its own `contextWindow` / `maxTokens` defaults, and we have no reliable
/// per-model metadata to override them with.
pub(crate) fn build_pi_provider_block(provider: &Provider, model: &str) -> Value {
    let base_url = openai_base(provider).trim().trim_end_matches('/');

    let mut seen = std::collections::HashSet::new();
    let mut model_ids: Vec<String> = Vec::new();
    for candidate in std::iter::once(model)
        .chain(std::iter::once(default_model(provider)))
        .chain(provider.models.iter().map(String::as_str))
    {
        let id = candidate.trim();
        if !id.is_empty() && seen.insert(id.to_string()) {
            model_ids.push(id.to_string());
        }
    }

    let models: Vec<Value> = model_ids
        .into_iter()
        .map(|id| serde_json::json!({ "id": id }))
        .collect();

    serde_json::json!({
        "baseUrl": base_url,
        "api": "openai-completions",
        "apiKey": api_key(provider),
        "models": models
    })
}

pub(crate) fn sync_pi_binding_inner(
    binding: &AgentBinding,
    providers: &[Provider],
    config_path: &Path,
    settings_path: &Path,
) -> Result<Option<PathBuf>> {
    let (entries, active_id) = resolve_entries(binding, providers)
        .context("Pi binding has no resolvable provider entries")?;

    let (backup_path, active_pointer) = sync_json_blocks_inner(
        &entries,
        &active_id,
        config_path,
        "providers",
        || serde_json::json!({ "providers": {} }),
        build_pi_provider_block,
        // Pi's active pointer lives in settings.json, not models.json.
        |_root, _active| {},
    )?;

    // settings.json: point Pi's default model at the active entry, preserving
    // every other setting the user keeps there.
    if let Some((provider_key, model_id)) = active_pointer {
        if settings_path.exists() {
            create_rolling_backup(settings_path)?;
        }
        merge_json_write(
            settings_path,
            &[
                ("defaultProvider", Value::String(provider_key)),
                ("defaultModel", Value::String(model_id)),
            ],
        )?;
    }

    Ok(backup_path)
}
// ---------------------------------------------------------------------------
// Unified dispatch
// ---------------------------------------------------------------------------

/// Write a tool's current binding to disk, routing through the agent registry.
///
/// The single sync entry point for the command layer: each agent's
/// [`AgentSpec::sync_binding`] column projects the binding (single-provider
/// agents write their active entry's env block, multi-provider agents project
/// the whole binding). An empty binding unsyncs the tool via
/// [`AgentSpec::unsync`]. Unknown tools return a failed result.
pub fn sync_tool_binding(store: &ProvidersStoreV4, tool_id: &str) -> ToolSyncResultFlat {
    let Some(spec) = agent_spec(tool_id) else {
        return ToolSyncResultFlat {
            tool_id: tool_id.to_string(),
            success: false,
            config_path: None,
            error: Some(format!("Unknown tool_id '{tool_id}'")),
            backup_path: None,
            dropped_roles: Vec::new(),
        };
    };

    sync_binding_with_spec(spec, store)
}

/// The dispatch body, taking the spec instead of looking it up.
///
/// Split out so the claim "a new agent needs a registry row and a writer, and
/// nothing else" can be *tested*: a synthetic [`AgentSpec`] built in a test —
/// an id this function has never heard of — flows through unchanged. If this
/// body ever grows a `match tool_id`, that test stops passing.
pub(crate) fn sync_binding_with_spec(
    spec: &AgentSpec,
    store: &ProvidersStoreV4,
) -> ToolSyncResultFlat {
    let empty = AgentBinding::default();
    let binding = store.bindings.get(spec.id).unwrap_or(&empty);

    // Empty binding → ensure the tool is clean.
    if binding.is_empty() {
        let unsync_result = (spec.unsync)();
        return ToolSyncResultFlat {
            tool_id: spec.id.to_string(),
            success: unsync_result.is_ok(),
            config_path: None,
            error: unsync_result.err().map(|e| e.to_string()),
            backup_path: None,
            dropped_roles: Vec::new(),
        };
    }

    (spec.sync_binding)(binding, &store.providers).unwrap_or_else(err_result(spec.id))
}

/// Build a closure that turns a sync error into a failed `ToolSyncResultFlat`
/// for the given tool — keeps the dispatch arms terse.
fn err_result(tool_id: &str) -> impl Fn(anyhow::Error) -> ToolSyncResultFlat + '_ {
    move |e| ToolSyncResultFlat::failed_without_path(tool_id, e)
}

// ---------------------------------------------------------------------------
// Unsync (prefix-aware)
// ---------------------------------------------------------------------------

/// Remove every SkillStar-managed Codex provider table (`skillstar` +
/// `skillstar_*`) plus the top-level pointer and `OPENAI_API_KEY`.
pub fn unsync_codex_all() -> Result<()> {
    let auth_path = resolve_codex_auth_path()?;
    let config_path = resolve_codex_config_path()?;
    unsync_codex_all_at(&auth_path, &config_path)
}

/// Path-taking core of [`unsync_codex_all`] — exposed `pub(crate)` so unit
/// tests can drive it against isolated temp paths instead of the shared
/// sandbox HOME (avoids cross-test races on `~/.codex/config.toml`).
pub(crate) fn unsync_codex_all_at(auth_path: &Path, config_path: &Path) -> Result<()> {
    if auth_path.exists() {
        create_rolling_backup(auth_path)?;
        let content = std::fs::read_to_string(auth_path)?;
        let mut json: Value = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", auth_path.display()))?;
        if let Some(obj) = json.as_object_mut() {
            obj.remove("OPENAI_API_KEY");
        }
        skillstar_core::infra::fs_ops::atomic_write(
            auth_path,
            serde_json::to_string_pretty(&json)?.as_bytes(),
        )?;
    }

    if config_path.exists() {
        create_rolling_backup(config_path)?;
        let content = std::fs::read_to_string(config_path)?;
        let mut table: toml::Table = toml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", config_path.display()))?;
        table.remove("model_provider");
        table.remove("model");
        if let Some(mp) = table
            .get_mut("model_providers")
            .and_then(|v| v.as_table_mut())
        {
            mp.retain(|k, _| !is_skillstar_managed_key(k));
            if mp.is_empty() {
                table.remove("model_providers");
            }
        }
        skillstar_core::infra::fs_ops::atomic_write(
            config_path,
            toml::to_string_pretty(&table)?.as_bytes(),
        )?;
    }
    Ok(())
}

/// Remove **one** provider's managed table from Codex's `config.toml`.
///
/// The migration needs this and full unsync will not do. A user with three
/// providers bound to Codex, one of which is chat-only, must lose exactly that
/// one: leaving its `wire_api = "chat"` table behind keeps Codex unbootable,
/// and clearing all three throws away two working bindings to fix a third.
pub fn unsync_codex_entry(provider_id: &str) -> Result<()> {
    unsync_codex_entry_at(provider_id, &resolve_codex_config_path()?)
}

/// Path-taking core of [`unsync_codex_entry`].
pub(crate) fn unsync_codex_entry_at(provider_id: &str, config_path: &Path) -> Result<()> {
    if !config_path.exists() {
        return Ok(());
    }
    let key = skillstar_managed_key(provider_id);
    let content = std::fs::read_to_string(config_path)?;
    let mut table: toml::Table = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let removed = table
        .get_mut("model_providers")
        .and_then(|v| v.as_table_mut())
        .map(|mp| mp.remove(&key).is_some())
        .unwrap_or(false);
    if !removed {
        return Ok(());
    }
    create_rolling_backup(config_path)?;

    // The top-level pointer must not outlive the table it names — Codex fails
    // to start on a `model_provider` that resolves to nothing, which is the
    // same class of breakage this whole repair exists to undo.
    if table
        .get("model_provider")
        .and_then(|v| v.as_str())
        .is_some_and(|current| current == key)
    {
        table.remove("model_provider");
        table.remove("model");
    }
    if table
        .get("model_providers")
        .and_then(|v| v.as_table())
        .is_some_and(|mp| mp.is_empty())
    {
        table.remove("model_providers");
    }

    skillstar_core::infra::fs_ops::atomic_write(
        config_path,
        toml::to_string_pretty(&table)?.as_bytes(),
    )?;
    Ok(())
}

/// Remove every SkillStar-managed OpenCode provider block (`skillstar` +
/// `skillstar_*`) plus the top-level `model` selector when it points at one.
pub fn unsync_opencode_all() -> Result<()> {
    let config_path = resolve_opencode_config_path()?;
    if !config_path.exists() {
        return Ok(());
    }
    create_rolling_backup(&config_path)?;
    let content = std::fs::read_to_string(&config_path)?;
    let mut json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let model_points_at_managed = json
        .get("model")
        .and_then(|v| v.as_str())
        .and_then(|m| m.split('/').next())
        .is_some_and(is_skillstar_managed_key);

    if let Some(root) = json.as_object_mut() {
        if let Some(providers) = root.get_mut("provider").and_then(|v| v.as_object_mut()) {
            providers.retain(|k, _| !is_skillstar_managed_key(k));
            if providers.is_empty() {
                root.remove("provider");
            }
        }
        if model_points_at_managed {
            root.remove("model");
        }
    }

    skillstar_core::infra::fs_ops::atomic_write(
        &config_path,
        serde_json::to_string_pretty(&json)?.as_bytes(),
    )?;
    Ok(())
}

/// Remove every SkillStar-managed Pi provider block (`skillstar` +
/// `skillstar_*`) from `models.json`, plus the `defaultProvider` /
/// `defaultModel` pointer in `settings.json` when it points at one.
pub fn unsync_pi_all() -> Result<()> {
    let models_path = resolve_pi_models_path()?;
    let settings_path = resolve_pi_settings_path()?;
    unsync_pi_all_at(&models_path, &settings_path)
}

/// Path-taking core of [`unsync_pi_all`] — exposed `pub(crate)` so unit tests
/// can drive it against isolated temp paths instead of the shared sandbox HOME.
pub(crate) fn unsync_pi_all_at(models_path: &Path, settings_path: &Path) -> Result<()> {
    if models_path.exists() {
        create_rolling_backup(models_path)?;
        let content = std::fs::read_to_string(models_path)?;
        let mut json: Value = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", models_path.display()))?;
        if let Some(root) = json.as_object_mut()
            && let Some(providers) = root.get_mut("providers").and_then(|v| v.as_object_mut())
        {
            providers.retain(|k, _| !is_skillstar_managed_key(k));
        }
        skillstar_core::infra::fs_ops::atomic_write(
            models_path,
            serde_json::to_string_pretty(&json)?.as_bytes(),
        )?;
    }

    if settings_path.exists() {
        let content = std::fs::read_to_string(settings_path)?;
        let mut json: Value = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", settings_path.display()))?;
        let points_at_managed = json
            .get("defaultProvider")
            .and_then(|v| v.as_str())
            .is_some_and(is_skillstar_managed_key);
        if points_at_managed && let Some(root) = json.as_object_mut() {
            create_rolling_backup(settings_path)?;
            root.remove("defaultProvider");
            root.remove("defaultModel");
            skillstar_core::infra::fs_ops::atomic_write(
                settings_path,
                serde_json::to_string_pretty(&json)?.as_bytes(),
            )?;
        }
    }
    Ok(())
}

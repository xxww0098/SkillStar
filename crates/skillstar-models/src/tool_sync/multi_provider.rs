//! Multi-provider sync (Codex, OpenCode, Pi) no longer writes Agent configs.
//!
//! Unsync still removes managed `skillstar` / `skillstar_*` keys. Codex's
//! loopback config is written by `skillstar-gateway`, not from this store.

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

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// Whether a Codex binding may keep this provider.
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
    let has_responses = !provider
        .endpoint_for(RequiredWire::OpenaiResponses)
        .unwrap_or("")
        .trim()
        .is_empty();
    !provider.caps.responses_api.is_denied() && has_responses
}

/// Codex config is no longer written from the provider store.
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

/// Codex config is no longer written from the provider store.
pub fn sync_codex_binding_inner(
    binding: &AgentBinding,
    providers: &[Provider],
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    let _ = (binding, providers, config_path);
    Ok(None)
}

// ---------------------------------------------------------------------------
// OpenCode
// ---------------------------------------------------------------------------

/// OpenCode config is no longer written from the provider store.
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
    let _ = (binding, providers, config_path);
    Ok(None)
}

// ---------------------------------------------------------------------------
// Pi
// ---------------------------------------------------------------------------

/// Pi config is no longer written from the provider store.
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

pub(crate) fn sync_pi_binding_inner(
    binding: &AgentBinding,
    providers: &[Provider],
    config_path: &Path,
    settings_path: &Path,
) -> Result<Option<PathBuf>> {
    let _ = (binding, providers, config_path, settings_path);
    Ok(None)
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

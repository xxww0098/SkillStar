//! Single-provider sync writer (Claude Code) and unsync/deactivation.
//!
//! Multi-provider binding writers (Codex, OpenCode, Pi) live in
//! `multi_provider.rs`; this module keeps the single-env-block writers plus
//! the registry-driven unsync dispatch.

use super::*;

/// Resolve the path to Codex's auth.json file.
pub fn resolve_codex_auth_path() -> Result<PathBuf> {
    Ok(codex_home()?.join("auth.json"))
}

/// Resolve the path to Codex's config.toml file.
pub fn resolve_codex_config_path() -> Result<PathBuf> {
    Ok(codex_home()?.join("config.toml"))
}

/// Codex's own home, honouring the upstream `CODEX_HOME` override.
///
/// The CLI namespaces both `auth.json` **and** its keychain entry by this
/// directory (`cli|<sha256(canonical CODEX_HOME)[..16]>`), so resolving the
/// hardcoded `~/.codex` for a user who moved it would write credentials the
/// CLI never reads. The sandbox still wins: tests must never escape into a
/// developer's real Codex home even when `CODEX_HOME` is exported.
fn codex_home() -> Result<PathBuf> {
    if let Some(home) = sandbox_home() {
        return Ok(home.join(".codex"));
    }
    if let Some(dir) = upstream_home_override("CODEX_HOME") {
        return Ok(dir);
    }
    Ok(sync_home_dir()?.join(".codex"))
}

/// Sync a provider's credentials to Claude Code's config file.
///
/// Writes to `~/.claude/settings.json` env block, preserving existing non-managed fields.
/// Creates a rolling backup before writing (keeps last 5).
///
/// The env block will contain:
/// - `ANTHROPIC_BASE_URL`: the provider's Anthropic-compatible base URL
/// - `ANTHROPIC_AUTH_TOKEN`: the provider's API key
/// - one env key per declared role in the agent registry, taken from the
///   binding's `roles` map (the key is removed when blank)
///
/// The tier overrides used to live in `provider.meta`, where only Claude could
/// reach them; v4 stores them as roles on the binding. Which role lands in which
/// env key is no longer spelled out here either — the registry row owns that
/// mapping, so adding a Claude role is a registry edit rather than an edit here
/// plus an edit to the managed-key list plus an edit to the unsync path.
pub fn sync_to_claude_code(
    provider: &Provider,
    model: &str,
    roles: &std::collections::BTreeMap<String, ModelRef>,
) -> Result<ToolSyncResultFlat> {
    let config_path = resolve_tool_config_path("claude-code")?;
    Ok(ToolSyncResultFlat::from_write_outcome_with_drops(
        "claude-code",
        &config_path,
        sync_to_claude_code_inner(provider, model, roles, &config_path)
            .map(|backup| (backup, claude_dropped_roles(provider, roles))),
    ))
}

/// Roles the Claude writer will not put on disk, and why.
///
/// Claude Code is a single-provider agent: its env block names exactly one
/// `ANTHROPIC_BASE_URL`, so a role pointing at a *different* provider cannot be
/// honoured — the model id would be sent to the bound provider's endpoint and
/// fail. v3 wrote the model id anyway (it only ever read the string), producing
/// a config that looks configured and 404s. The role is skipped, and the user is
/// told which one and why instead of being left to discover it at runtime.
fn claude_dropped_roles(
    provider: &Provider,
    roles: &std::collections::BTreeMap<String, ModelRef>,
) -> Vec<DroppedRole> {
    let defs = agent_spec("claude-code").map(|s| s.roles).unwrap_or(&[]);
    let mut dropped = Vec::new();
    for (role, target) in roles {
        if !defs.iter().any(|def| def.id == role.as_str()) {
            dropped.push(DroppedRole::new(role, RoleDropReason::RoleNotSupported));
        } else if target.model.trim().is_empty() {
            // Nothing to write is not a failure — it is how a role is cleared —
            // so an empty *and* unset role is silent. A role with a provider but
            // no model is a half-filled row worth flagging.
            if !target.provider_id.trim().is_empty() {
                dropped.push(DroppedRole::new(role, RoleDropReason::NoModel));
            }
        } else if !target.provider_id.trim().is_empty() && target.provider_id != provider.id {
            dropped.push(DroppedRole::for_provider(
                role,
                RoleDropReason::ProviderNotBound,
                &target.provider_id,
            ));
        }
    }
    dropped
}

/// Inner implementation for Claude Code sync.
pub(crate) fn sync_to_claude_code_inner(
    provider: &Provider,
    model: &str,
    roles: &std::collections::BTreeMap<String, ModelRef>,
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    // Native login: clear SkillStar-managed env so Claude uses its own login.
    if provider.is_external_cli() {
        return clear_claude_managed_env_at(config_path);
    }

    let anthropic_base = anthropic_base(provider);
    if anthropic_base.is_empty() {
        bail!(
            "Provider '{}' does not have an Anthropic-compatible endpoint",
            provider.name
        );
    }

    // Create rolling backup if file exists
    let backup_path = if config_path.exists() {
        Some(create_rolling_backup(config_path)?)
    } else {
        None
    };

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Build managed fields for the env block. Every role the registry declares
    // contributes its env key, written when set, or Null (→ key removed) when the
    // user left it blank. `ANTHROPIC_MODEL` is the `default` role's key and takes
    // the active entry's model when no `default` role is assigned — the entry is
    // the direct statement of intent that predates roles, and a binding with no
    // role config at all has to keep behaving exactly as it did.
    //
    // An empty/whitespace model (a provider with no `default_model` bound without
    // an explicit model) is Null rather than `""`: Claude Code cannot resolve an
    // empty model id, and the missing key is the state that lets it fall back.
    let mut managed_fields: Vec<(&str, Value)> = vec![
        (
            "ANTHROPIC_BASE_URL",
            Value::String(anthropic_base.to_string()),
        ),
        (
            "ANTHROPIC_AUTH_TOKEN",
            Value::String(api_key(provider).to_string()),
        ),
    ];
    for def in claude_role_defs() {
        let from_role = role_model_field(provider, roles, def.id);
        let value = if def.id == crate::providers::ROLE_DEFAULT && from_role.is_null() {
            trim_or_null(model)
        } else {
            from_role
        };
        managed_fields.push((def.agent_key, value));
    }

    // Merge write into the env block
    merge_json_env_write(config_path, &managed_fields)?;

    Ok(backup_path)
}

/// Claude Code's declared roles, or an empty slice if the registry ever loses
/// the row (in which case the writer degrades to base URL + token rather than
/// panicking on a lookup that should never fail).
fn claude_role_defs() -> &'static [crate::providers::RoleDef] {
    agent_spec("claude-code")
        .map(|spec| spec.roles)
        .unwrap_or(&[])
}

/// Remove SkillStar-managed Claude env keys (Official / unsync shared path).
fn clear_claude_managed_env_at(config_path: &Path) -> Result<Option<PathBuf>> {
    if !config_path.exists() {
        return Ok(None);
    }

    let backup_path = Some(create_rolling_backup(config_path)?);

    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON in {}", config_path.display()))?;

    if let Some(env_obj) = json.get_mut("env").and_then(|v| v.as_object_mut()) {
        for key in claude_managed_env_keys() {
            env_obj.remove(key);
        }
        if env_obj.is_empty()
            && let Some(root_obj) = json.as_object_mut()
        {
            root_obj.remove("env");
        }
    }

    let output =
        serde_json::to_string_pretty(&json).context("Failed to serialize Claude Code config")?;
    std::fs::write(config_path, output)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    Ok(backup_path)
}

/// Read one role's model out of the binding. Returns a `Value::String` for a
/// usable assignment, otherwise `Value::Null` (which `merge_json_env_write`
/// treats as "remove the key").
///
/// A role pointing at a provider other than the bound one yields Null: Claude's
/// env block carries a single base URL, so that model id would be sent to the
/// wrong host. [`claude_dropped_roles`] reports the same condition to the caller
/// so the skip is visible rather than silent.
///
/// Fallbacks are deliberately **not** resolved here. Writing the inherited value
/// into the tier key would make "explicitly set to the same model" and "left to
/// Claude's own default" identical on disk, and clearing the field would no
/// longer restore Claude's behaviour.
fn role_model_field(
    provider: &Provider,
    roles: &std::collections::BTreeMap<String, ModelRef>,
    role: &str,
) -> Value {
    roles
        .get(role)
        .filter(|target| target.provider_id.trim().is_empty() || target.provider_id == provider.id)
        .map(|target| target.model.trim())
        .filter(|model| !model.is_empty())
        .map(|model| Value::String(model.to_string()))
        .unwrap_or(Value::Null)
}

/// Non-empty trimmed string → `Value::String`; empty/whitespace → `Value::Null`
/// (which `merge_json_env_write` treats as "remove the key"). Used for
/// `ANTHROPIC_MODEL` so an empty model selection is dropped instead of written
/// as an invalid `""` value.
fn trim_or_null(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() {
        Value::Null
    } else {
        Value::String(t.to_string())
    }
}

pub(crate) fn build_opencode_provider_block(provider: &Provider, model: &str) -> Value {
    let default_model = default_model(provider);
    let selected_model_id = if model.trim().is_empty() {
        if default_model.trim().is_empty() {
            "default".to_string()
        } else {
            default_model.to_string()
        }
    } else {
        model.to_string()
    };

    let base_url = openai_base(provider).trim().trim_end_matches('/');
    // The catalog left the provider row in v4; it now lives in the cache
    // directory. Reading it here is what keeps each model's `name` / `limit` /
    // `cost` block identical to what v3 wrote.
    let catalog = catalog_cache::read_catalog(&provider.id);
    let model_ids = build_opencode_model_ids(provider, &selected_model_id, &catalog);
    let models = model_ids
        .iter()
        .map(|model_id| {
            let entry = catalog.iter().find(|entry| entry.id == *model_id);
            (
                model_id.clone(),
                build_opencode_model_entry(model_id, entry),
            )
        })
        .collect::<serde_json::Map<String, Value>>();

    serde_json::json!({
        "npm": "@ai-sdk/openai-compatible",
        "name": provider.name,
        "options": {
            "baseURL": base_url,
            "apiKey": api_key(provider),
        },
        "models": models
    })
}

fn build_opencode_model_ids(
    provider: &Provider,
    selected_model_id: &str,
    catalog: &[ModelCatalogEntry],
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();

    for candidate in std::iter::once(selected_model_id)
        .chain(std::iter::once(default_model(provider)))
        .chain(provider.models.iter().map(String::as_str))
        .chain(catalog.iter().map(|entry| entry.id.as_str()))
    {
        let id = candidate.trim();
        if !id.is_empty() && seen.insert(id.to_string()) {
            ids.push(id.to_string());
        }
    }

    ids
}

fn build_opencode_model_entry(model_id: &str, catalog_entry: Option<&ModelCatalogEntry>) -> Value {
    let mut model = serde_json::Map::new();
    let display_name = catalog_entry
        .and_then(|entry| entry.display_name.as_deref())
        .unwrap_or(model_id);
    model.insert("name".to_string(), Value::String(display_name.to_string()));

    if let Some(entry) = catalog_entry {
        if let Some(source_name) = entry.source_name.as_deref()
            && source_name != model_id
        {
            model.insert("id".to_string(), Value::String(source_name.to_string()));
        }

        let mut limit = serde_json::Map::new();
        if let Some(context) = entry.context_length {
            limit.insert("context".to_string(), Value::Number(context.into()));
        }
        if let Some(output) = entry.max_completion_tokens {
            limit.insert("output".to_string(), Value::Number(output.into()));
        }
        if !limit.is_empty() {
            model.insert("limit".to_string(), Value::Object(limit));
        }
        if let Some(cost) = entry.cost.clone() {
            model.insert("cost".to_string(), cost);
        }
    }

    Value::Object(model)
}

/// Registry adapter: write a Claude Code binding by resolving its active
/// entry (single-provider agents only ever project the active entry).
pub(crate) fn sync_claude_code_binding(
    binding: &AgentBinding,
    providers: &[Provider],
) -> Result<ToolSyncResultFlat> {
    let (provider, model) = resolve_single_active(binding, providers)?;
    sync_to_claude_code(provider, model, &binding.roles)
}

/// Persist the Claude Desktop store binding to a local marker file.
///
/// Does not write Claude Desktop's native profile/proxy config yet. The marker
/// (`resolve_claude_desktop_binding_path`) keeps CLI / Desktop store bindings
/// independently inspectable under `SKILLSTAR_TOOL_SYNC_HOME`.
pub(crate) fn sync_claude_desktop_binding(
    binding: &AgentBinding,
    providers: &[Provider],
) -> Result<ToolSyncResultFlat> {
    let path = resolve_claude_desktop_binding_path()?;
    Ok(ToolSyncResultFlat::from_write_outcome(
        "claude-desktop",
        &path,
        sync_claude_desktop_binding_inner(binding, providers, &path),
    ))
}

fn sync_claude_desktop_binding_inner(
    binding: &AgentBinding,
    providers: &[Provider],
    path: &Path,
) -> Result<Option<PathBuf>> {
    let (provider, model) = resolve_single_active(binding, providers)?;
    let mut first_backup: Option<PathBuf> = None;
    if path.exists() {
        first_backup = Some(create_rolling_backup(path)?);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let body = serde_json::json!({
        "provider_id": provider.id,
        "provider_name": provider.name,
        "model": model,
        "note": "SkillStar binding marker; Claude Desktop native write-path TBD",
    });
    std::fs::write(path, serde_json::to_string_pretty(&body)?)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(first_backup)
}

/// Remove the Claude Desktop SkillStar binding marker (deactivation).
pub fn unsync_claude_desktop() -> Result<()> {
    let path = resolve_claude_desktop_binding_path()?;
    if path.exists() {
        let _ = create_rolling_backup(&path)?;
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    Ok(())
}

/// Marker-file detect: report bound provider name when the marker exists.
pub(crate) fn detect_claude_desktop_provider(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
    Ok(value
        .get("provider_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

/// Resolve the active entry of a single-provider binding to `(provider, model)`.
fn resolve_single_active<'a>(
    binding: &'a AgentBinding,
    providers: &'a [Provider],
) -> Result<(&'a Provider, &'a str)> {
    let active = binding.active().context("no active entry")?;
    let provider = providers
        .iter()
        .find(|p| p.id == active.provider_id)
        .with_context(|| format!("Provider '{}' not found", active.provider_id))?;
    Ok((provider, active.model.as_str()))
}

/// Remove every SkillStar-managed field/entry from a tool's config files.
///
/// Registry-driven deactivation dispatch: known agents route to their
/// [`AgentSpec::unsync`] column; ids missing from the registry are a no-op
/// (nothing was ever written for them).
pub fn unsync_tool(tool_id: &str) -> Result<()> {
    match agent_spec(tool_id) {
        Some(spec) => (spec.unsync)(),
        None => Ok(()),
    }
}

/// Remove managed fields from Claude Code's config (deactivation).
///
/// Removes `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, and `ANTHROPIC_MODEL`
/// from the `env` block in `~/.claude/settings.json`.
/// Preserves all other user-added fields in the env block and top-level.
pub fn unsync_claude_code() -> Result<()> {
    let config_path = resolve_tool_config_path("claude-code")?;
    let _ = clear_claude_managed_env_at(&config_path)?;
    Ok(())
}

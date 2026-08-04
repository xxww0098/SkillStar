//! Single-provider sync writer (Claude Code) and unsync/deactivation.
//!
//! Multi-provider binding writers (Codex, OpenCode, Pi) live in
//! `multi_provider.rs`; this module keeps the single-env-block writers plus
//! the registry-driven unsync dispatch.

use super::*;

/// Resolve the path to Codex's auth.json file.
pub fn resolve_codex_auth_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".codex").join("auth.json"))
}

/// Resolve the path to Codex's config.toml file.
pub fn resolve_codex_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".codex").join("config.toml"))
}

/// Sync a provider's credentials to Claude Code's config file.
///
/// Writes to `~/.claude/settings.json` env block, preserving existing non-managed fields.
/// Creates a rolling backup before writing (keeps last 5).
///
/// The env block will contain:
/// - `ANTHROPIC_BASE_URL`: the provider's Anthropic-compatible base URL
/// - `ANTHROPIC_AUTH_TOKEN`: the provider's API key
/// - `ANTHROPIC_MODEL`: the selected model
/// - `ANTHROPIC_DEFAULT_HAIKU_MODEL` / `_SONNET_MODEL` / `_OPUS_MODEL`: optional
///   tier overrides read from `provider.meta` (the key is removed when blank)
pub fn sync_to_claude_code(
    provider: &ProviderEntryFlat,
    model: &str,
) -> Result<ToolSyncResultFlat> {
    let config_path = resolve_tool_config_path("claude-code")?;
    Ok(ToolSyncResultFlat::from_write_outcome(
        "claude-code",
        &config_path,
        sync_to_claude_code_inner(provider, model, &config_path),
    ))
}

/// Inner implementation for Claude Code sync.
pub(crate) fn sync_to_claude_code_inner(
    provider: &ProviderEntryFlat,
    model: &str,
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    // Native Official: clear SkillStar-managed env so Claude uses browser/client login.
    if crate::providers::is_native_official_provider(provider) {
        return clear_claude_managed_env_at(config_path);
    }

    // Validate that base_url_anthropic is non-empty
    if provider.base_url_anthropic.is_empty() {
        bail!(
            "Provider '{}' does not have an Anthropic-compatible endpoint (base_url_anthropic is empty)",
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

    // Build managed fields for the env block. The tier-model overrides
    // (Haiku/Sonnet/Opus) come from `provider.meta`; each is written when set,
    // or passed as Null (→ key removed) when the user left it blank.
    // `ANTHROPIC_MODEL` follows the same rule: an empty/whitespace model
    // (e.g. a provider with no `default_model` activated without an explicit
    // model) is treated as Null so Claude Code doesn't receive an invalid
    // `"ANTHROPIC_MODEL": ""` that breaks model resolution.
    let managed_fields: Vec<(&str, Value)> = vec![
        (
            "ANTHROPIC_BASE_URL",
            Value::String(provider.base_url_anthropic.clone()),
        ),
        (
            "ANTHROPIC_AUTH_TOKEN",
            Value::String(provider.api_key.clone()),
        ),
        ("ANTHROPIC_MODEL", trim_or_null(model)),
        (
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            meta_model_field(provider, "claude_haiku_model"),
        ),
        (
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            meta_model_field(provider, "claude_sonnet_model"),
        ),
        (
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            meta_model_field(provider, "claude_opus_model"),
        ),
    ];

    // Merge write into the env block
    merge_json_env_write(config_path, &managed_fields)?;

    Ok(backup_path)
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
        for key in CLAUDE_MANAGED_ENV_KEYS {
            env_obj.remove(*key);
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

/// Read a Claude tier-model override from `provider.meta`. Returns a
/// `Value::String` when the field is a non-empty string, otherwise
/// `Value::Null` (which `merge_json_env_write` treats as "remove the key").
fn meta_model_field(provider: &ProviderEntryFlat, key: &str) -> Value {
    provider
        .meta
        .as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| Value::String(s.to_string()))
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

pub(crate) fn build_opencode_provider_block(provider: &ProviderEntryFlat, model: &str) -> Value {
    let selected_model_id = if model.trim().is_empty() {
        if provider.default_model.trim().is_empty() {
            "default".to_string()
        } else {
            provider.default_model.clone()
        }
    } else {
        model.to_string()
    };

    let base_url = provider.base_url_openai.trim().trim_end_matches('/');
    let catalog = catalog_from_meta(provider.meta.as_ref());
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
            "apiKey": provider.api_key,
        },
        "models": models
    })
}

fn build_opencode_model_ids(
    provider: &ProviderEntryFlat,
    selected_model_id: &str,
    catalog: &[ModelCatalogEntry],
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();

    for candidate in std::iter::once(selected_model_id)
        .chain(std::iter::once(provider.default_model.as_str()))
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
    binding: &crate::providers::ToolBinding,
    providers: &[ProviderEntryFlat],
) -> Result<ToolSyncResultFlat> {
    let (provider, model) = resolve_single_active(binding, providers)?;
    sync_to_claude_code(provider, model)
}

/// Persist the Claude Desktop store binding to a local marker file.
///
/// Does not write Claude Desktop's native profile/proxy config yet. The marker
/// (`resolve_claude_desktop_binding_path`) keeps CLI / Desktop store bindings
/// independently inspectable under `SKILLSTAR_TOOL_SYNC_HOME`.
pub(crate) fn sync_claude_desktop_binding(
    binding: &crate::providers::ToolBinding,
    providers: &[ProviderEntryFlat],
) -> Result<ToolSyncResultFlat> {
    let path = resolve_claude_desktop_binding_path()?;
    Ok(ToolSyncResultFlat::from_write_outcome(
        "claude-desktop",
        &path,
        sync_claude_desktop_binding_inner(binding, providers, &path),
    ))
}

fn sync_claude_desktop_binding_inner(
    binding: &crate::providers::ToolBinding,
    providers: &[ProviderEntryFlat],
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
    binding: &'a crate::providers::ToolBinding,
    providers: &'a [ProviderEntryFlat],
) -> Result<(&'a ProviderEntryFlat, &'a str)> {
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

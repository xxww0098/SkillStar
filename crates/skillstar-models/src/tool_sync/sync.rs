//! Single-provider sync writers (Claude Code, Gemini) and unsync/deactivation.
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
    let config_path_str = config_path.to_string_lossy().to_string();

    match sync_to_claude_code_inner(provider, model, &config_path) {
        Ok(backup_path) => Ok(ToolSyncResultFlat {
            tool_id: "claude-code".to_string(),
            success: true,
            config_path: Some(config_path_str),
            error: None,
            backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
        }),
        Err(e) => Ok(ToolSyncResultFlat {
            tool_id: "claude-code".to_string(),
            success: false,
            config_path: Some(config_path_str),
            error: Some(e.to_string()),
            backup_path: None,
        }),
    }
}

/// Inner implementation for Claude Code sync.
pub(crate) fn sync_to_claude_code_inner(
    provider: &ProviderEntryFlat,
    model: &str,
    config_path: &Path,
) -> Result<Option<PathBuf>> {
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

/// Sync a provider's credentials to Gemini CLI's `~/.gemini/.env`.
///
/// Writes `GOOGLE_GEMINI_BASE_URL`, `GEMINI_API_KEY`, and `GEMINI_MODEL`,
/// preserving any other user-defined env entries. Creates a rolling backup
/// before writing (keeps last 5).
pub fn sync_to_gemini(provider: &ProviderEntryFlat, model: &str) -> Result<ToolSyncResultFlat> {
    let config_path = match resolve_gemini_env_path() {
        Ok(p) => p,
        Err(e) => {
            return Ok(ToolSyncResultFlat {
                tool_id: "gemini".to_string(),
                success: false,
                config_path: None,
                error: Some(e.to_string()),
                backup_path: None,
            });
        }
    };
    let config_path_str = config_path.to_string_lossy().to_string();

    match sync_to_gemini_inner(provider, model, &config_path) {
        Ok(backup_path) => Ok(ToolSyncResultFlat {
            tool_id: "gemini".to_string(),
            success: true,
            config_path: Some(config_path_str),
            error: None,
            backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
        }),
        Err(e) => Ok(ToolSyncResultFlat {
            tool_id: "gemini".to_string(),
            success: false,
            config_path: Some(config_path_str),
            error: Some(e.to_string()),
            backup_path: None,
        }),
    }
}

pub(crate) fn sync_to_gemini_inner(
    provider: &ProviderEntryFlat,
    model: &str,
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    let base_url = provider.base_url_openai.trim().trim_end_matches('/');
    if base_url.is_empty() {
        bail!(
            "Provider '{}' has no OpenAI-compatible endpoint (base_url_openai is empty); Gemini CLI needs a base URL",
            provider.name
        );
    }

    let model_id = if model.trim().is_empty() {
        provider.default_model.trim().to_string()
    } else {
        model.trim().to_string()
    };

    let managed: Vec<(&str, Option<String>)> = vec![
        ("GOOGLE_GEMINI_BASE_URL", Some(base_url.to_string())),
        ("GEMINI_API_KEY", Some(provider.api_key.clone())),
        (
            "GEMINI_MODEL",
            if model_id.is_empty() {
                None
            } else {
                Some(model_id)
            },
        ),
    ];

    merge_env_write(config_path, &managed)
}

/// Remove every SkillStar-managed field/entry from a tool's config files.
///
/// Registry-driven deactivation dispatch: known agents route to their unsync
/// implementation; ids missing from the registry are a no-op (nothing was ever
/// written for them).
pub fn unsync_tool(tool_id: &str) -> Result<()> {
    match agent_spec(tool_id).map(|spec| spec.id) {
        Some("claude-code") => unsync_claude_code(),
        Some("codex") => unsync_codex_all(),
        Some("opencode") => unsync_opencode_all(),
        Some("gemini") => unsync_gemini(),
        Some("pi") => unsync_pi_all(),
        _ => Ok(()),
    }
}

/// Remove managed Gemini env keys from `~/.gemini/.env` (deactivation).
pub fn unsync_gemini() -> Result<()> {
    let config_path = resolve_gemini_env_path()?;
    if !config_path.exists() {
        return Ok(());
    }
    let managed: Vec<(&str, Option<String>)> =
        GEMINI_MANAGED_ENV_KEYS.iter().map(|k| (*k, None)).collect();
    merge_env_write(&config_path, &managed)?;
    Ok(())
}

/// Remove managed fields from Claude Code's config (deactivation).
///
/// Removes `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, and `ANTHROPIC_MODEL`
/// from the `env` block in `~/.claude/settings.json`.
/// Preserves all other user-added fields in the env block and top-level.
pub fn unsync_claude_code() -> Result<()> {
    let config_path = resolve_tool_config_path("claude-code")?;

    if !config_path.exists() {
        // Nothing to unsync
        return Ok(());
    }

    // Create rolling backup before modifying
    create_rolling_backup(&config_path)?;

    // Read existing JSON
    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON in {}", config_path.display()))?;

    // Remove managed keys from the env block
    if let Some(env_obj) = json.get_mut("env").and_then(|v| v.as_object_mut()) {
        for key in CLAUDE_MANAGED_ENV_KEYS {
            env_obj.remove(*key);
        }
        // If env block is now empty, remove it entirely
        if env_obj.is_empty()
            && let Some(root_obj) = json.as_object_mut()
        {
            root_obj.remove("env");
        }
    }

    // Write back
    let output =
        serde_json::to_string_pretty(&json).context("Failed to serialize Claude Code config")?;
    std::fs::write(&config_path, output)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    Ok(())
}

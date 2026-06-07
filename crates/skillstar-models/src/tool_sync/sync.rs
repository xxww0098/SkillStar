//! Provider-to-tool sync operations (v1 + flat v2) and unsync/deactivation.

use super::*;

// ---------------------------------------------------------------------------
// Sync operations
// ---------------------------------------------------------------------------

/// Sync a provider's configuration to a single external tool.
///
/// Steps:
/// 1. Backup existing config file (if it exists) to `{path}.bak.{timestamp_ms}`
/// 2. Generate tool-specific config content
/// 3. Write the config file (create parent dirs if needed)
///
/// Returns a `ToolSyncResult` with success/failure status.
pub fn sync_provider_to_tool(provider: &ProviderEntry, tool_id: &str) -> ToolSyncResult {
    match sync_provider_to_tool_inner(provider, tool_id) {
        Ok((config_path, backup_path)) => ToolSyncResult {
            tool_id: tool_id.to_string(),
            success: true,
            error: None,
            config_path,
            backup_path,
        },
        Err(e) => {
            let config_path = resolve_tool_config_path(tool_id)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| format!("<unknown path for {}>", tool_id));
            ToolSyncResult {
                tool_id: tool_id.to_string(),
                success: false,
                error: Some(e.to_string()),
                config_path,
                backup_path: None,
            }
        }
    }
}

/// Inner implementation that returns Result for easier error handling.
fn sync_provider_to_tool_inner(
    provider: &ProviderEntry,
    tool_id: &str,
) -> Result<(String, Option<String>)> {
    let config_path = resolve_tool_config_path(tool_id)?;
    let config_path_str = config_path.to_string_lossy().to_string();

    // Parse provider settings
    let settings: ProviderSettings = serde_json::from_value(provider.settings_config.clone())
        .context("Failed to parse provider settings_config")?;

    // Step 1: Backup existing config file
    let backup_path = if config_path.exists() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let backup = format!("{}.bak.{}", config_path_str, timestamp);
        std::fs::copy(&config_path, &backup)
            .with_context(|| format!("Failed to create backup at {}", backup))?;
        Some(backup)
    } else {
        None
    };

    // Step 2: Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Step 3: Generate and write tool-specific config
    match tool_id {
        "claude-code" => write_claude_code_config(&config_path, &settings)?,
        "codex" => write_codex_config(&config_path, &settings)?,
        _ => bail!("Unknown tool_id: '{}'", tool_id),
    }

    Ok((config_path_str, backup_path))
}

/// Sync a provider to multiple tools with per-tool error isolation.
///
/// If one tool fails, others still succeed. Returns a result for each tool.
pub fn sync_provider_to_all_tools(
    provider: &ProviderEntry,
    tool_ids: &[String],
) -> Vec<ToolSyncResult> {
    tool_ids
        .iter()
        .map(|tool_id| sync_provider_to_tool(provider, tool_id))
        .collect()
}

// ---------------------------------------------------------------------------
// Flat store sync operations (v2 architecture)
// ---------------------------------------------------------------------------

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
        bail!("Provider '{}' does not have an Anthropic-compatible endpoint (base_url_anthropic is empty)", provider.name);
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
    let managed_fields: Vec<(&str, Value)> = vec![
        ("ANTHROPIC_BASE_URL", Value::String(provider.base_url_anthropic.clone())),
        ("ANTHROPIC_AUTH_TOKEN", Value::String(provider.api_key.clone())),
        ("ANTHROPIC_MODEL", Value::String(model.to_string())),
        ("ANTHROPIC_DEFAULT_HAIKU_MODEL", meta_model_field(provider, "claude_haiku_model")),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL", meta_model_field(provider, "claude_sonnet_model")),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL", meta_model_field(provider, "claude_opus_model")),
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

/// Sync a provider's credentials to Codex's config files.
///
/// Writes to:
/// - `~/.codex/auth.json`: `{ "OPENAI_API_KEY": "<api_key>" }` (only when auth_mode is "api_key")
/// - `~/.codex/config.toml`: `model_provider = "skillstar"`, `model = "<model>"`,
///   and `[model_providers.skillstar]` table
///
/// `activation.settings` controls Codex-specific options:
/// - `wire_api`: `"responses"` (default) or `"chat"`
/// - `auth_mode`: `"api_key"` (default) or `"oauth"`
///
/// Creates rolling backups before writing (keeps last 5 per file).
/// Preserves existing non-managed fields in both files.
pub fn sync_to_codex(
    provider: &ProviderEntryFlat,
    activation: &ToolActivation,
) -> Result<ToolSyncResultFlat> {
    let config_path = resolve_codex_config_path()?;
    let config_path_str = config_path.to_string_lossy().to_string();

    match sync_to_codex_inner(provider, activation) {
        Ok(backup_path) => Ok(ToolSyncResultFlat {
            tool_id: "codex".to_string(),
            success: true,
            config_path: Some(config_path_str),
            error: None,
            backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
        }),
        Err(e) => Ok(ToolSyncResultFlat {
            tool_id: "codex".to_string(),
            success: false,
            config_path: Some(config_path_str),
            error: Some(e.to_string()),
            backup_path: None,
        }),
    }
}

/// Inner implementation for Codex sync.
fn sync_to_codex_inner(
    provider: &ProviderEntryFlat,
    activation: &ToolActivation,
) -> Result<Option<PathBuf>> {
    // Validate that base_url_openai is non-empty
    if provider.base_url_openai.is_empty() {
        bail!("Provider '{}' does not have an OpenAI-compatible endpoint (base_url_openai is empty)", provider.name);
    }

    // Resolve settings: activation overrides > provider-level defaults > hardcoded defaults
    let settings = activation
        .settings
        .as_ref()
        .map(CodexSettings::from_value)
        .unwrap_or_else(|| CodexSettings {
            wire_api: provider.codex_wire_api.clone(),
            auth_mode: provider.codex_auth_mode.clone(),
        });

    let auth_path = resolve_codex_auth_path()?;
    let config_path = resolve_codex_config_path()?;

    // Track the first backup path to return
    let mut first_backup: Option<PathBuf> = None;

    // --- Write auth.json ---
    if settings.auth_mode == "api_key" {
        // Create rolling backup if file exists
        if auth_path.exists() {
            let backup = create_rolling_backup(&auth_path)?;
            if first_backup.is_none() {
                first_backup = Some(backup);
            }
        }

        // Ensure parent directory exists
        if let Some(parent) = auth_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        // Merge write auth.json — set OPENAI_API_KEY
        let auth_fields: Vec<(&str, Value)> = vec![
            ("OPENAI_API_KEY", Value::String(provider.api_key.clone())),
        ];
        merge_json_write(&auth_path, &auth_fields)?;
    } else {
        // OAuth mode: clear or skip OPENAI_API_KEY so Codex CLI handles auth itself
        if auth_path.exists() {
            let backup = create_rolling_backup(&auth_path)?;
            if first_backup.is_none() {
                first_backup = Some(backup);
            }
            let auth_fields: Vec<(&str, Value)> = vec![
                ("OPENAI_API_KEY", Value::String("".to_string())),
            ];
            merge_json_write(&auth_path, &auth_fields)?;
        }
    }

    // --- Write config.toml ---
    // Create rolling backup if file exists
    if config_path.exists() {
        let backup = create_rolling_backup(&config_path)?;
        if first_backup.is_none() {
            first_backup = Some(backup);
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Write config.toml with merge semantics
    write_codex_config_flat(&config_path, provider, activation, &settings)?;

    Ok(first_backup)
}

/// Sync a provider to OpenCode's `opencode.json` under `provider.skillstar`.
pub fn sync_to_opencode(
    provider: &ProviderEntryFlat,
    model: &str,
) -> Result<ToolSyncResultFlat> {
    let config_path = resolve_opencode_config_path()?;
    let config_path_str = config_path.to_string_lossy().to_string();

    match sync_to_opencode_inner(provider, model, &config_path) {
        Ok(backup_path) => Ok(ToolSyncResultFlat {
            tool_id: "opencode".to_string(),
            success: true,
            config_path: Some(config_path_str),
            error: None,
            backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
        }),
        Err(e) => Ok(ToolSyncResultFlat {
            tool_id: "opencode".to_string(),
            success: false,
            config_path: Some(config_path_str),
            error: Some(e.to_string()),
            backup_path: None,
        }),
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

fn sync_to_opencode_inner(
    provider: &ProviderEntryFlat,
    model: &str,
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    if provider.base_url_openai.trim().is_empty() {
        bail!(
            "Provider '{}' has no OpenAI-compatible endpoint (base_url_openai is empty)",
            provider.name
        );
    }

    let backup_path = if config_path.exists() {
        Some(create_rolling_backup(config_path)?)
    } else {
        None
    };

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let mut root: Value = if config_path.exists() {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| {
            serde_json::json!({
                "$schema": "https://opencode.ai/config.json",
                "provider": {}
            })
        })
    } else {
        serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "provider": {}
        })
    };

    if root.get("$schema").is_none()
        && let Some(obj) = root.as_object_mut() {
            obj.insert(
                "$schema".to_string(),
                Value::String("https://opencode.ai/config.json".to_string()),
            );
        }

    let provider_block = build_opencode_provider_block(provider, model);
    let root_obj = root.as_object_mut().context("opencode.json root must be an object")?;
    let providers = root_obj
        .entry("provider")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(map) = providers.as_object_mut() {
        map.insert(
            OPENCODE_MANAGED_PROVIDER_KEY.to_string(),
            provider_block,
        );
    }

    let output = serde_json::to_string_pretty(&root).context("Failed to serialize opencode.json")?;
    std::fs::write(config_path, output)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    Ok(backup_path)
}

/// Remove managed OpenCode provider block from `opencode.json`.
pub fn unsync_opencode() -> Result<()> {
    let config_path = resolve_opencode_config_path()?;
    if !config_path.exists() {
        return Ok(());
    }

    create_rolling_backup(&config_path)?;

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse JSON in {}", config_path.display()))?;

    if let Some(providers) = json.get_mut("provider").and_then(|v| v.as_object_mut()) {
        providers.remove(OPENCODE_MANAGED_PROVIDER_KEY);
        if providers.is_empty()
            && let Some(root) = json.as_object_mut() {
                root.remove("provider");
            }
    }

    let output = serde_json::to_string_pretty(&json).context("Failed to serialize opencode.json")?;
    std::fs::write(&config_path, output)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    Ok(())
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
            if model_id.is_empty() { None } else { Some(model_id) },
        ),
    ];

    merge_env_write(config_path, &managed)
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
            && let Some(root_obj) = json.as_object_mut() {
                root_obj.remove("env");
            }
    }

    // Write back
    let output = serde_json::to_string_pretty(&json)
        .context("Failed to serialize Claude Code config")?;
    std::fs::write(&config_path, output)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;

    Ok(())
}

/// Remove managed fields from Codex's config (deactivation).
///
/// Removes:
/// - `OPENAI_API_KEY` from `~/.codex/auth.json`
/// - `model_provider`, `model`, and `[model_providers.skillstar]` from `~/.codex/config.toml`
///
/// Preserves all other user-added fields/sections.
pub fn unsync_codex() -> Result<()> {
    let auth_path = resolve_codex_auth_path()?;
    let config_path = resolve_codex_config_path()?;

    // --- Unsync auth.json ---
    if auth_path.exists() {
        create_rolling_backup(&auth_path)?;

        let content = std::fs::read_to_string(&auth_path)
            .with_context(|| format!("Failed to read {}", auth_path.display()))?;
        let mut json: Value = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON in {}", auth_path.display()))?;

        if let Some(obj) = json.as_object_mut() {
            obj.remove("OPENAI_API_KEY");
        }

        let output = serde_json::to_string_pretty(&json)
            .context("Failed to serialize auth.json")?;
        std::fs::write(&auth_path, output)
            .with_context(|| format!("Failed to write {}", auth_path.display()))?;
    }

    // --- Unsync config.toml ---
    if config_path.exists() {
        create_rolling_backup(&config_path)?;

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;
        let mut table: toml::Table = toml::from_str(&content)
            .with_context(|| format!("Failed to parse TOML in {}", config_path.display()))?;

        // Remove top-level managed keys
        table.remove("model_provider");
        table.remove("model");

        // Remove [model_providers.skillstar] section
        if let Some(model_providers) = table.get_mut("model_providers")
            && let Some(mp_table) = model_providers.as_table_mut() {
                mp_table.remove(CODEX_MANAGED_PROVIDER_KEY);
                // If model_providers is now empty, remove it entirely
                if mp_table.is_empty() {
                    table.remove("model_providers");
                }
            }

        let output = toml::to_string_pretty(&table)
            .context("Failed to serialize Codex config.toml")?;
        std::fs::write(&config_path, output)
            .with_context(|| format!("Failed to write {}", config_path.display()))?;
    }

    Ok(())
}

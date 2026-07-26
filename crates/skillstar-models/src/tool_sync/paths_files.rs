//! Tool-config path resolution, file read/write/format, and dotenv helpers.

use super::*;

// ---------------------------------------------------------------------------
// Path resolution (security: only hardcoded known paths)
// ---------------------------------------------------------------------------

/// Resolve the primary config file path for a given tool_id.
///
/// Registry-driven: only tool ids registered in [`agent_specs`] resolve.
/// Returns an error for any other tool_id to prevent arbitrary file writes.
pub fn resolve_tool_config_path(tool_id: &str) -> Result<PathBuf> {
    match agent_spec(tool_id) {
        Some(spec) => (spec.files[0].resolve)(),
        None => bail!(
            "Unknown tool_id: '{}'. Supported: claude-code, codex, opencode, gemini, pi.",
            tool_id
        ),
    }
}

/// `~/.config/opencode/opencode.json`
pub fn resolve_opencode_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".config").join("opencode").join("opencode.json"))
}

/// `~/.zcode/v2/config.json` — ZCode desktop config (OpenCode schema).
///
/// Retained as a shared path resolver: the MCP subsystem uses it to clean up
/// stale OpenCode-style `mcp` entries from this file
/// (`mcp::zcode_v2_opencode_mcp_remove`), and the Usage subsystem resolves it
/// for `switch_zcode`. ZCode is **no longer** a model-workbench provider tool
/// (no `sync_to_zcode`), so this path is intentionally not wired into
/// `resolve_tool_config_path` anymore.
pub fn resolve_zcode_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".zcode").join("v2").join("config.json"))
}

/// `~/.gemini/.env` — Gemini CLI reads provider credentials from this dotenv file.
pub fn resolve_gemini_env_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".gemini").join(".env"))
}

/// `~/.pi/agent/models.json` — Pi's custom provider/model registry
/// (`providers.<key>` blocks with `baseUrl` / `api` / `apiKey` / `models`).
pub fn resolve_pi_models_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".pi").join("agent").join("models.json"))
}

/// `~/.pi/agent/settings.json` — Pi's global settings; carries the
/// `defaultProvider` / `defaultModel` pointer selecting the active model.
pub fn resolve_pi_settings_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".pi").join("agent").join("settings.json"))
}

/// `~/.grok/auth.json` — the xAI Grok Build CLI's OAuth credential store.
///
/// The file is a JSON object keyed by OIDC scope URLs
/// (`https://auth.x.ai::<client-id>`); each entry carries `key` (the bearer
/// access token), `refresh_token`, `expires_at`, and identity metadata. This
/// is the file account-switching rewrites to flip which Grok account the CLI
/// authenticates as. We deliberately mirror [`resolve_codex_auth_path`] and
/// ignore the `GROK_HOME` override so resolution always funnels through the
/// sandboxable [`sync_home_dir`] (keeps tests off the developer's real
/// `~/.grok`).
pub fn resolve_grok_auth_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".grok").join("auth.json"))
}

/// Resolve a config file path for `(tool_id, file_id)` from the registry.
pub fn resolve_tool_config_file_path(tool_id: &str, file_id: &str) -> Result<PathBuf> {
    agent_spec(tool_id)
        .and_then(|spec| spec.files.iter().find(|f| f.file_id == file_id))
        .map(|file| (file.resolve)())
        .unwrap_or_else(|| bail!("Unknown tool config file: {tool_id}/{file_id}"))
}

/// List editable config files for a tool (used by the JSON/TOML editor UI).
pub fn list_tool_config_files(tool_id: &str) -> Result<Vec<ToolConfigFileInfo>> {
    let spec = agent_spec(tool_id).with_context(|| format!("Unknown tool_id: '{tool_id}'"))?;
    spec.files
        .iter()
        .map(|file| {
            let path = (file.resolve)()?;
            Ok(ToolConfigFileInfo {
                file_id: file.file_id.to_string(),
                label: file.label.to_string(),
                path: path.to_string_lossy().to_string(),
                format: file.format.to_string(),
                exists: path.exists(),
                managed_by_skillstar: true,
            })
        })
        .collect()
}

/// Read raw config file contents (empty string if missing).
pub fn read_tool_config_file(tool_id: &str, file_id: &str) -> Result<String> {
    let path = resolve_tool_config_file_path(tool_id, file_id)?;
    if !path.exists() {
        return Ok(default_empty_config_content(tool_id, file_id));
    }
    std::fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))
}

/// Skeleton content served by the editor when a config file does not exist.
/// Registry-driven; unknown ids fall back to an empty JSON object.
pub(crate) fn default_empty_config_content(tool_id: &str, file_id: &str) -> String {
    agent_spec(tool_id)
        .and_then(|spec| spec.files.iter().find(|f| f.file_id == file_id))
        .map(|file| file.default_content.to_string())
        .unwrap_or_else(|| "{}".to_string())
}

/// Validate and write config file contents (creates rolling backup when file exists).
pub fn write_tool_config_file(
    tool_id: &str,
    file_id: &str,
    content: &str,
) -> WriteToolConfigFileResult {
    match write_tool_config_file_inner(tool_id, file_id, content) {
        Ok(backup) => WriteToolConfigFileResult {
            success: true,
            backup_path: backup.map(|p| p.to_string_lossy().to_string()),
            error: None,
        },
        Err(e) => WriteToolConfigFileResult {
            success: false,
            backup_path: None,
            error: Some(e.to_string()),
        },
    }
}

fn write_tool_config_file_inner(
    tool_id: &str,
    file_id: &str,
    content: &str,
) -> Result<Option<PathBuf>> {
    let path = resolve_tool_config_file_path(tool_id, file_id)?;
    let info = list_tool_config_files(tool_id)?
        .into_iter()
        .find(|f| f.file_id == file_id)
        .context("Config file descriptor not found")?;

    if info.format == "json" {
        let _: Value =
            serde_json::from_str(content).context("Invalid JSON — fix syntax before saving")?;
    } else if info.format == "toml" {
        let _: toml::Table =
            toml::from_str(content).context("Invalid TOML — fix syntax before saving")?;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let backup = if path.exists() {
        Some(create_rolling_backup(&path)?)
    } else {
        None
    };

    let normalized = match info.format.as_str() {
        "json" => {
            let value: Value = serde_json::from_str(content)?;
            serde_json::to_string_pretty(&value).context("Failed to format JSON")?
        }
        "toml" => {
            let table: toml::Table = toml::from_str(content)?;
            toml::to_string_pretty(&table).context("Failed to format TOML")?
        }
        // dotenv files (Gemini): preserve as-is so user comments/ordering survive.
        _ => content.to_string(),
    };

    std::fs::write(&path, normalized)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(backup)
}

/// Pretty-format existing file contents without changing semantics.
pub fn format_tool_config_file(tool_id: &str, file_id: &str) -> Result<String> {
    let content = read_tool_config_file(tool_id, file_id)?;
    let info = list_tool_config_files(tool_id)?
        .into_iter()
        .find(|f| f.file_id == file_id)
        .context("Config file descriptor not found")?;
    match info.format.as_str() {
        "json" => {
            let value: Value = serde_json::from_str(&content).context("Invalid JSON")?;
            Ok(serde_json::to_string_pretty(&value)?)
        }
        "toml" => {
            let table: toml::Table = toml::from_str(&content).context("Invalid TOML")?;
            Ok(toml::to_string_pretty(&table)?)
        }
        // dotenv: normalize by re-serializing parsed key/value pairs (sorted, comments dropped).
        _ => Ok(serialize_env_file(&parse_env_file(&content))),
    }
}

// ---------------------------------------------------------------------------
// dotenv (.env) helpers — used by the Gemini CLI integration
// ---------------------------------------------------------------------------

/// Parse a `.env` file into an ordered list of `(key, value)` pairs, skipping
/// blank lines and comments. Order is preserved so a merge write keeps the
/// user's existing layout stable.
pub(crate) fn parse_env_file(content: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                pairs.push((key.to_string(), value.trim().to_string()));
            }
        }
    }
    pairs
}

/// Serialize ordered `(key, value)` pairs back into `.env` text.
fn serialize_env_file(pairs: &[(String, String)]) -> String {
    let mut out: String = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Merge managed key/value pairs into a `.env` file, preserving unmanaged keys
/// and creating a rolling backup when the file already exists.
///
/// A `None` value removes the key (used on deactivation). Existing keys keep
/// their position; new keys are appended in the supplied order.
pub(crate) fn merge_env_write(
    path: &Path,
    managed: &[(&str, Option<String>)],
) -> Result<Option<PathBuf>> {
    let backup = if path.exists() {
        Some(create_rolling_backup(path)?)
    } else {
        None
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let existing = if path.exists() {
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?
    } else {
        String::new()
    };

    let mut pairs = parse_env_file(&existing);

    for (key, value) in managed {
        match value {
            Some(v) => {
                if let Some(slot) = pairs.iter_mut().find(|(k, _)| k == key) {
                    slot.1 = v.clone();
                } else {
                    pairs.push(((*key).to_string(), v.clone()));
                }
            }
            None => pairs.retain(|(k, _)| k != key),
        }
    }

    std::fs::write(path, serialize_env_file(&pairs))
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(backup)
}

// ---------------------------------------------------------------------------
// Tool config targets
// ---------------------------------------------------------------------------

/// Returns the list of supported tool config targets with their paths and existence status.
pub fn get_tool_config_targets() -> Result<Vec<ToolConfigTarget>> {
    let mut targets = Vec::new();

    for spec in agent_specs() {
        let config_path = (spec.files[0].resolve)()?;
        let exists = config_path.exists();
        let current_provider = if exists {
            detect_current_provider(spec.id, &config_path)
                .ok()
                .flatten()
        } else {
            None
        };

        targets.push(ToolConfigTarget {
            tool_id: spec.id.to_string(),
            display_name: spec.display_name.to_string(),
            config_path: config_path.to_string_lossy().to_string(),
            exists,
            current_provider,
        });
    }

    Ok(targets)
}

/// Attempt to detect the current provider name from an existing config file.
fn detect_current_provider(tool_id: &str, path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }

    match tool_id {
        "claude-code" => {
            let content = std::fs::read_to_string(path)?;
            let json: Value = serde_json::from_str(&content)?;
            // Try to read apiUrl as a hint for the provider
            if let Some(api_url) = json.get("apiUrl").and_then(|v| v.as_str()) {
                return Ok(Some(api_url.to_string()));
            }
            Ok(None)
        }
        "codex" => {
            let content = std::fs::read_to_string(path)?;
            let table: toml::Table = toml::from_str(&content)?;
            // Follow the active pointer (`model_provider`) to its managed table,
            // falling back to any skillstar* table or a legacy `[provider]`.
            if let Some(mp) = table.get("model_providers").and_then(|v| v.as_table()) {
                let active_key = table.get("model_provider").and_then(|v| v.as_str());
                let active = active_key.and_then(|k| mp.get(k)).or_else(|| {
                    mp.iter()
                        .find(|(k, _)| is_skillstar_managed_key(k))
                        .map(|(_, v)| v)
                });
                if let Some(url) = active
                    .and_then(|v| v.as_table())
                    .and_then(|ss| ss.get("base_url"))
                    .and_then(|v| v.as_str())
                {
                    return Ok(Some(url.to_string()));
                }
            }
            if let Some(provider) = table.get("provider").and_then(|v| v.as_table())
                && let Some(base_url) = provider.get("base_url").and_then(|v| v.as_str())
            {
                return Ok(Some(base_url.to_string()));
            }
            Ok(None)
        }
        "opencode" => {
            let content = std::fs::read_to_string(path)?;
            let json: Value = serde_json::from_str(&content)?;
            if let Some(providers) = json.get("provider").and_then(|p| p.as_object()) {
                // Prefer the block named by the active `model` selector; else
                // any skillstar* block.
                let active_key = json
                    .get("model")
                    .and_then(|v| v.as_str())
                    .and_then(|m| m.split('/').next());
                let block = active_key.and_then(|k| providers.get(k)).or_else(|| {
                    providers
                        .iter()
                        .find(|(k, _)| is_skillstar_managed_key(k))
                        .map(|(_, v)| v)
                });
                if let Some(name) = block.and_then(|c| c.get("name")).and_then(|v| v.as_str()) {
                    return Ok(Some(name.to_string()));
                }
            }
            Ok(None)
        }
        "gemini" => {
            let content = std::fs::read_to_string(path)?;
            let pairs = parse_env_file(&content);
            Ok(pairs
                .into_iter()
                .find(|(k, _)| k == "GOOGLE_GEMINI_BASE_URL")
                .map(|(_, v)| v))
        }
        "pi" => {
            let content = std::fs::read_to_string(path)?;
            let json: Value = serde_json::from_str(&content)?;
            if let Some(providers) = json.get("providers").and_then(|p| p.as_object()) {
                // Report the base URL of any skillstar* managed block.
                if let Some(url) = providers
                    .iter()
                    .find(|(k, _)| is_skillstar_managed_key(k))
                    .and_then(|(_, v)| v.get("baseUrl"))
                    .and_then(|v| v.as_str())
                {
                    return Ok(Some(url.to_string()));
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

//! Tool-config path resolution, file read/write/format, and dotenv helpers.

use super::*;

// ---------------------------------------------------------------------------
// Path resolution (security: only hardcoded known paths)
// ---------------------------------------------------------------------------

/// Resolve the config file path for a given tool_id.
///
/// Only accepts "claude-code", "codex", "opencode", and "claude-desktop" as valid tool IDs.
/// Returns an error for any other tool_id to prevent arbitrary file writes.
pub fn resolve_tool_config_path(tool_id: &str) -> Result<PathBuf> {
    let home = sync_home_dir()?;
    match tool_id {
        "claude-code" => Ok(home.join(".claude").join("settings.json")),
        "codex" => Ok(home.join(".codex").join("config.toml")),
        "opencode" => Ok(resolve_opencode_config_path()?),
        "claude-desktop" => Ok(resolve_claude_desktop_config_path()?),
        "gemini" => Ok(resolve_gemini_env_path()?),
        _ => bail!(
            "Unknown tool_id: '{}'. Supported: claude-code, codex, opencode, claude-desktop, gemini.",
            tool_id
        ),
    }
}

/// `~/.config/opencode/opencode.json`
pub fn resolve_opencode_config_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".config").join("opencode").join("opencode.json"))
}

/// `~/.gemini/.env` — Gemini CLI reads provider credentials from this dotenv file.
pub fn resolve_gemini_env_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".gemini").join(".env"))
}

/// Resolve the Claude Desktop config file path.
///
/// - macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
/// - Windows: `%APPDATA%\Claude\claude_desktop_config.json`
/// - Linux: `~/.config/Claude/claude_desktop_config.json` (no official Linux Claude Desktop yet,
///   but we mirror the macOS/Windows layout so power users running it via Wine/Flatpak still work).
///
/// Claude Desktop only honours the `mcpServers` section of this file — it does NOT accept
/// custom `base_url` or API keys, since it authenticates via the user's Claude.ai account.
pub fn resolve_claude_desktop_config_path() -> Result<PathBuf> {
    let base = sync_config_dir()?;
    Ok(base.join("Claude").join("claude_desktop_config.json"))
}

/// Resolve a config file path for `(tool_id, file_id)`.
pub fn resolve_tool_config_file_path(tool_id: &str, file_id: &str) -> Result<PathBuf> {
    match (tool_id, file_id) {
        ("claude-code", "settings") => resolve_tool_config_path("claude-code"),
        ("codex", "config") => resolve_codex_config_path(),
        ("codex", "auth") => resolve_codex_auth_path(),
        ("opencode", "opencode") => resolve_opencode_config_path(),
        ("claude-desktop", "config") => resolve_claude_desktop_config_path(),
        ("gemini", "env") => resolve_gemini_env_path(),
        _ => bail!("Unknown tool config file: {tool_id}/{file_id}"),
    }
}

/// List editable config files for a tool (used by the JSON/TOML editor UI).
pub fn list_tool_config_files(tool_id: &str) -> Result<Vec<ToolConfigFileInfo>> {
    match tool_id {
        "claude-code" => {
            let path = resolve_tool_config_path("claude-code")?;
            Ok(vec![ToolConfigFileInfo {
                file_id: "settings".to_string(),
                label: "settings.json".to_string(),
                path: path.to_string_lossy().to_string(),
                format: "json".to_string(),
                exists: path.exists(),
                managed_by_skillstar: true,
            }])
        }
        "codex" => {
            let config = resolve_codex_config_path()?;
            let auth = resolve_codex_auth_path()?;
            Ok(vec![
                ToolConfigFileInfo {
                    file_id: "config".to_string(),
                    label: "config.toml".to_string(),
                    path: config.to_string_lossy().to_string(),
                    format: "toml".to_string(),
                    exists: config.exists(),
                    managed_by_skillstar: true,
                },
                ToolConfigFileInfo {
                    file_id: "auth".to_string(),
                    label: "auth.json".to_string(),
                    path: auth.to_string_lossy().to_string(),
                    format: "json".to_string(),
                    exists: auth.exists(),
                    managed_by_skillstar: true,
                },
            ])
        }
        "opencode" => {
            let path = resolve_opencode_config_path()?;
            Ok(vec![ToolConfigFileInfo {
                file_id: "opencode".to_string(),
                label: "opencode.json".to_string(),
                path: path.to_string_lossy().to_string(),
                format: "json".to_string(),
                exists: path.exists(),
                managed_by_skillstar: true,
            }])
        }
        "claude-desktop" => {
            let path = resolve_claude_desktop_config_path()?;
            Ok(vec![ToolConfigFileInfo {
                file_id: "config".to_string(),
                label: "claude_desktop_config.json".to_string(),
                path: path.to_string_lossy().to_string(),
                format: "json".to_string(),
                exists: path.exists(),
                // Claude Desktop config is NOT "managed by SkillStar" in the usual sense —
                // SkillStar only edits the `mcpServers` node, leaving everything else untouched.
                managed_by_skillstar: false,
            }])
        }
        "gemini" => {
            let path = resolve_gemini_env_path()?;
            Ok(vec![ToolConfigFileInfo {
                file_id: "env".to_string(),
                label: ".env".to_string(),
                path: path.to_string_lossy().to_string(),
                format: "env".to_string(),
                exists: path.exists(),
                managed_by_skillstar: true,
            }])
        }
        _ => bail!("Unknown tool_id: '{tool_id}'"),
    }
}

/// Read raw config file contents (empty string if missing).
pub fn read_tool_config_file(tool_id: &str, file_id: &str) -> Result<String> {
    let path = resolve_tool_config_file_path(tool_id, file_id)?;
    if !path.exists() {
        return Ok(default_empty_config_content(tool_id, file_id));
    }
    std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))
}

fn default_empty_config_content(tool_id: &str, file_id: &str) -> String {
    match (tool_id, file_id) {
        ("claude-code", "settings") => "{\n  \"env\": {}\n}\n".to_string(),
        ("codex", "auth") => "{\n  \"OPENAI_API_KEY\": \"\"\n}\n".to_string(),
        ("codex", "config") => {
            "model_provider = \"skillstar\"\nmodel = \"\"\n\n[model_providers.skillstar]\nname = \"SkillStar\"\nbase_url = \"\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n".to_string()
        }
        ("opencode", "opencode") => {
            "{\n  \"$schema\": \"https://opencode.ai/config.json\",\n  \"provider\": {}\n}\n".to_string()
        }
        // Claude Desktop only honours `mcpServers` — start with an empty list so the user
        // can drop entries in without first reading the schema.
        ("claude-desktop", "config") => "{\n  \"mcpServers\": {}\n}\n".to_string(),
        ("gemini", "env") => {
            "GOOGLE_GEMINI_BASE_URL=\nGEMINI_API_KEY=\nGEMINI_MODEL=\n".to_string()
        }
        _ => "{}".to_string(),
    }
}

/// Validate and write config file contents (creates rolling backup when file exists).
pub fn write_tool_config_file(tool_id: &str, file_id: &str, content: &str) -> WriteToolConfigFileResult {
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

fn write_tool_config_file_inner(tool_id: &str, file_id: &str, content: &str) -> Result<Option<PathBuf>> {
    let path = resolve_tool_config_file_path(tool_id, file_id)?;
    let info = list_tool_config_files(tool_id)?
        .into_iter()
        .find(|f| f.file_id == file_id)
        .context("Config file descriptor not found")?;

    if info.format == "json" {
        let _: Value = serde_json::from_str(content)
            .context("Invalid JSON — fix syntax before saving")?;
    } else if info.format == "toml" {
        let _: toml::Table = toml::from_str(content).context("Invalid TOML — fix syntax before saving")?;
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

    std::fs::write(&path, normalized).with_context(|| format!("Failed to write {}", path.display()))?;
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
pub(crate) fn merge_env_write(path: &Path, managed: &[(&str, Option<String>)]) -> Result<Option<PathBuf>> {
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
    let tool_ids = [
        ("claude-code", "Claude Code"),
        ("codex", "Codex"),
        ("opencode", "OpenCode"),
        ("gemini", "Gemini CLI"),
    ];
    let mut targets = Vec::new();

    for (tool_id, display_name) in &tool_ids {
        let config_path = resolve_tool_config_path(tool_id)?;
        let exists = config_path.exists();
        let current_provider = if exists {
            detect_current_provider(tool_id, &config_path).ok().flatten()
        } else {
            None
        };

        targets.push(ToolConfigTarget {
            tool_id: tool_id.to_string(),
            display_name: display_name.to_string(),
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
            if let Some(mp) = table.get("model_providers").and_then(|v| v.as_table())
                && let Some(ss) = mp.get(CODEX_MANAGED_PROVIDER_KEY).and_then(|v| v.as_table())
                    && let Some(url) = ss.get("base_url").and_then(|v| v.as_str()) {
                        return Ok(Some(url.to_string()));
                    }
            if let Some(provider) = table.get("provider").and_then(|v| v.as_table())
                && let Some(base_url) = provider.get("base_url").and_then(|v| v.as_str()) {
                    return Ok(Some(base_url.to_string()));
                }
            Ok(None)
        }
        "opencode" => {
            let content = std::fs::read_to_string(path)?;
            let json: Value = serde_json::from_str(&content)?;
            if let Some(name) = json
                .get("provider")
                .and_then(|p| p.get(OPENCODE_MANAGED_PROVIDER_KEY))
                .and_then(|c| c.get("name"))
                .and_then(|v| v.as_str())
            {
                return Ok(Some(name.to_string()));
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
        _ => Ok(None),
    }
}


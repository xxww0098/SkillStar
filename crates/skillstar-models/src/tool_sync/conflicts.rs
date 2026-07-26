//! Config-conflict detection (external edits, legacy files, env overrides).

use super::*;

// ---------------------------------------------------------------------------
// Config conflict detection
// ---------------------------------------------------------------------------

/// Environment variables that may override Claude Code config file settings.
const CLAUDE_ENV_VARS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
];

/// Environment variables that may override Codex config file settings.
const CODEX_ENV_VARS: &[&str] = &["OPENAI_API_KEY", "OPENAI_BASE_URL"];

/// Detect all config conflicts for a given tool.
///
/// Checks for:
/// - External modification of the tool's config file (mtime > last sync timestamp)
/// - Legacy `~/.claude.json` with conflicting env fields (for claude-code only)
/// - Shell environment variable overrides
///
/// Returns a list of detected conflicts for the frontend to display.
pub fn detect_conflicts(tool_id: &str, last_sync_timestamp: Option<u64>) -> Vec<ConfigConflict> {
    let mut conflicts = Vec::new();

    // Check external modification of the tool's config file
    if let Ok(config_path) = resolve_tool_config_path(tool_id)
        && let Some(mut conflict) = check_external_modification(&config_path, last_sync_timestamp)
    {
        conflict.tool_id = Some(tool_id.to_string());
        conflicts.push(conflict);
    }

    // Check legacy ~/.claude.json for claude-code tool
    if tool_id == "claude-code"
        && let Some(conflict) = check_legacy_claude_config()
    {
        conflicts.push(conflict);
    }

    // Check environment variable overrides
    conflicts.extend(detect_env_conflicts());

    conflicts
}

/// Detect environment variable overrides that affect Claude Code and Codex.
///
/// Checks for `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`,
/// `OPENAI_API_KEY`, and `OPENAI_BASE_URL` in the current process environment.
///
/// Returns a `ConfigConflict` for each detected override.
pub fn detect_env_conflicts() -> Vec<ConfigConflict> {
    let mut conflicts = Vec::new();

    // Check Anthropic/Claude-related env vars
    for &var_name in CLAUDE_ENV_VARS {
        if let Ok(value) = std::env::var(var_name)
            && !value.is_empty()
        {
            conflicts.push(ConfigConflict {
                conflict_type: ConflictType::EnvVarOverride,
                description: format!(
                    "环境变量 {} 已设置，将覆盖 Claude Code 配置文件中的对应设置",
                    var_name
                ),
                file_path: None,
                details: Some(format!("{}={}***", var_name, &value[..value.len().min(4)])),
                tool_id: None,
            });
        }
    }

    // Check OpenAI/Codex-related env vars
    for &var_name in CODEX_ENV_VARS {
        if let Ok(value) = std::env::var(var_name)
            && !value.is_empty()
        {
            conflicts.push(ConfigConflict {
                conflict_type: ConflictType::EnvVarOverride,
                description: format!(
                    "环境变量 {} 已设置，将覆盖 Codex 配置文件中的对应设置",
                    var_name
                ),
                file_path: None,
                details: Some(format!("{}={}***", var_name, &value[..value.len().min(4)])),
                tool_id: None,
            });
        }
    }

    conflicts
}

/// Check if a config file was modified externally since our last write.
///
/// Compares the file's modification time (mtime) against the provided
/// `last_sync_ts` (Unix timestamp in seconds). If mtime > last_sync_ts,
/// the file was modified externally after our last sync.
///
/// Returns `None` if:
/// - `last_sync_ts` is `None` (no previous sync recorded)
/// - The file does not exist
/// - The file's mtime cannot be read
/// - The file was not modified since last sync
pub(crate) fn check_external_modification(
    path: &Path,
    last_sync_ts: Option<u64>,
) -> Option<ConfigConflict> {
    let last_sync_ts = last_sync_ts?;

    if !path.exists() {
        return None;
    }

    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let mtime_secs = modified.duration_since(UNIX_EPOCH).ok()?.as_secs();

    if mtime_secs > last_sync_ts {
        Some(ConfigConflict {
            conflict_type: ConflictType::ExternalModification,
            description: format!(
                "配置文件在上次同步后被外部修改（文件修改时间: {}, 上次同步: {}）",
                mtime_secs, last_sync_ts
            ),
            file_path: Some(path.to_string_lossy().to_string()),
            details: Some(format!(
                "file_mtime={}, last_sync_ts={}, diff={}s",
                mtime_secs,
                last_sync_ts,
                mtime_secs - last_sync_ts
            )),
            tool_id: None,
        })
    } else {
        None
    }
}

/// Check for legacy `~/.claude.json` with conflicting env fields.
///
/// If `~/.claude.json` exists and contains an `env` block with any
/// ANTHROPIC_* fields, it may conflict with the primary config location
/// at `~/.claude/settings.json`.
fn check_legacy_claude_config() -> Option<ConfigConflict> {
    let home = sync_home_dir_opt()?;
    let legacy_path = home.join(".claude.json");

    if !legacy_path.exists() {
        return None;
    }

    // Read and parse the legacy file
    let content = std::fs::read_to_string(&legacy_path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;

    // Check for an "env" block with ANTHROPIC_* fields
    if let Some(env_obj) = json.get("env").and_then(|v| v.as_object()) {
        let conflicting_keys: Vec<&String> = env_obj
            .keys()
            .filter(|k| k.starts_with("ANTHROPIC_"))
            .collect();

        if !conflicting_keys.is_empty() {
            let keys_str = conflicting_keys
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", ");

            return Some(ConfigConflict {
                conflict_type: ConflictType::LegacyConfig,
                description: format!(
                    "检测到旧版配置文件 ~/.claude.json 中包含冲突的环境变量字段: {}",
                    keys_str
                ),
                file_path: Some(legacy_path.to_string_lossy().to_string()),
                details: Some(format!("conflicting_keys=[{}]", keys_str)),
                tool_id: None,
            });
        }
    }

    None
}


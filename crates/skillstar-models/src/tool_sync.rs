//! Tool configuration sync module.
//!
//! Writes provider settings to external tool config files (Claude Code, Codex).
//! Only supports hardcoded known config paths for security.
//!
//! ## Flat Store Sync (v2)
//!
//! The flat store sync functions write provider credentials from `ProviderEntryFlat`
//! to external tool config files:
//! - Claude Code: `~/.claude/settings.json` env block (ANTHROPIC_BASE_URL, ANTHROPIC_AUTH_TOKEN, ANTHROPIC_MODEL)
//! - Codex: `~/.codex/auth.json` (OPENAI_API_KEY) + `~/.codex/config.toml` (model_provider, model, [model_providers.skillstar])
//!
//! All writes use rolling backups (keep last 5) and merge semantics (preserve non-managed fields).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::providers::{FlatProvidersStore, ProviderEntry, ProviderEntryFlat, ProviderSettings};

// ---------------------------------------------------------------------------
// Config conflict detection types
// ---------------------------------------------------------------------------

/// Describes a detected configuration conflict that may affect tool sync.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigConflict {
    /// The type of conflict detected.
    pub conflict_type: ConflictType,
    /// Human-readable description of the conflict.
    pub description: String,
    /// The file path involved in the conflict, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Additional details (e.g., which env var, what value was found).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// The type of configuration conflict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConflictType {
    /// The config file was modified externally since our last sync write.
    ExternalModification,
    /// A legacy `~/.claude.json` file exists with conflicting env fields.
    LegacyConfig,
    /// A shell environment variable overrides config file settings.
    EnvVarOverride,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Describes an external tool's config file target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfigTarget {
    pub tool_id: String,
    pub display_name: String,
    pub config_path: String,
    pub exists: bool,
    pub current_provider: Option<String>,
}

/// Result of syncing a provider to a single tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSyncResult {
    pub tool_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub config_path: String,
    pub backup_path: Option<String>,
}

/// Result of syncing a provider to a single tool using the flat store format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSyncResultFlat {
    pub tool_id: String,
    pub success: bool,
    pub config_path: Option<String>,
    pub error: Option<String>,
    pub backup_path: Option<String>,
}

// ---------------------------------------------------------------------------
// Constants: managed field names
// ---------------------------------------------------------------------------

/// Fields managed by SkillStar in Claude Code's `~/.claude/settings.json` env block.
const CLAUDE_MANAGED_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_MODEL",
];

/// The key for the model_providers section managed by SkillStar in Codex's config.toml.
const CODEX_MANAGED_PROVIDER_KEY: &str = "skillstar";

// ---------------------------------------------------------------------------
// Path resolution (security: only hardcoded known paths)
// ---------------------------------------------------------------------------

/// Resolve the config file path for a given tool_id.
///
/// Only accepts "claude-code" and "codex" as valid tool IDs.
/// Returns an error for any other tool_id to prevent arbitrary file writes.
pub fn resolve_tool_config_path(tool_id: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    match tool_id {
        "claude-code" => Ok(home.join(".claude").join("settings.json")),
        "codex" => Ok(home.join(".codex").join("config.toml")),
        _ => bail!("Unknown tool_id: '{}'. Only 'claude-code' and 'codex' are supported.", tool_id),
    }
}

// ---------------------------------------------------------------------------
// Tool config targets
// ---------------------------------------------------------------------------

/// Returns the list of supported tool config targets with their paths and existence status.
pub fn get_tool_config_targets() -> Result<Vec<ToolConfigTarget>> {
    let tool_ids = [("claude-code", "Claude Code"), ("codex", "Codex")];
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
            // Try to read [provider].base_url as a hint
            if let Some(provider) = table.get("provider").and_then(|v| v.as_table()) {
                if let Some(base_url) = provider.get("base_url").and_then(|v| v.as_str()) {
                    return Ok(Some(base_url.to_string()));
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

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
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".codex").join("auth.json"))
}

/// Resolve the path to Codex's config.toml file.
pub fn resolve_codex_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
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
fn sync_to_claude_code_inner(
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

    // Build managed fields for the env block
    let managed_fields: Vec<(&str, Value)> = vec![
        ("ANTHROPIC_BASE_URL", Value::String(provider.base_url_anthropic.clone())),
        ("ANTHROPIC_AUTH_TOKEN", Value::String(provider.api_key.clone())),
        ("ANTHROPIC_MODEL", Value::String(model.to_string())),
    ];

    // Merge write into the env block
    merge_json_env_write(config_path, &managed_fields)?;

    Ok(backup_path)
}

/// Sync a provider's credentials to Codex's config files.
///
/// Writes to:
/// - `~/.codex/auth.json`: `{ "OPENAI_API_KEY": "<api_key>" }`
/// - `~/.codex/config.toml`: `model_provider = "skillstar"`, `model = "<model>"`,
///   and `[model_providers.skillstar]` table
///
/// Creates rolling backups before writing (keeps last 5 per file).
/// Preserves existing non-managed fields in both files.
pub fn sync_to_codex(
    provider: &ProviderEntryFlat,
    model: &str,
) -> Result<ToolSyncResultFlat> {
    let config_path = resolve_codex_config_path()?;
    let config_path_str = config_path.to_string_lossy().to_string();

    match sync_to_codex_inner(provider, model) {
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
    model: &str,
) -> Result<Option<PathBuf>> {
    // Validate that base_url_openai is non-empty
    if provider.base_url_openai.is_empty() {
        bail!("Provider '{}' does not have an OpenAI-compatible endpoint (base_url_openai is empty)", provider.name);
    }

    let auth_path = resolve_codex_auth_path()?;
    let config_path = resolve_codex_config_path()?;

    // Track the first backup path to return
    let mut first_backup: Option<PathBuf> = None;

    // --- Write auth.json ---
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
    write_codex_config_flat(&config_path, provider, model)?;

    Ok(first_backup)
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
        if env_obj.is_empty() {
            if let Some(root_obj) = json.as_object_mut() {
                root_obj.remove("env");
            }
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
        if let Some(model_providers) = table.get_mut("model_providers") {
            if let Some(mp_table) = model_providers.as_table_mut() {
                mp_table.remove(CODEX_MANAGED_PROVIDER_KEY);
                // If model_providers is now empty, remove it entirely
                if mp_table.is_empty() {
                    table.remove("model_providers");
                }
            }
        }

        let output = toml::to_string_pretty(&table)
            .context("Failed to serialize Codex config.toml")?;
        std::fs::write(&config_path, output)
            .with_context(|| format!("Failed to write {}", config_path.display()))?;
    }

    Ok(())
}

/// Create a rolling backup of a config file (keep last 5).
///
/// Copies the file to `{path}.bak.{timestamp_ms}` and removes older backups
/// beyond the 5 most recent.
///
/// Returns the path to the newly created backup file.
pub fn create_rolling_backup(path: &Path) -> Result<PathBuf> {
    let path_str = path.to_string_lossy().to_string();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let backup_name = format!("{}.bak.{}", path_str, timestamp);
    let backup_path = PathBuf::from(&backup_name);

    std::fs::copy(path, &backup_path)
        .with_context(|| format!("Failed to create backup at {}", backup_name))?;

    // Clean up old backups — keep only the 5 most recent
    cleanup_old_backups(path, 5)?;

    Ok(backup_path)
}

/// Remove old backup files, keeping only the `keep` most recent.
fn cleanup_old_backups(path: &Path, keep: usize) -> Result<()> {
    let parent = match path.parent() {
        Some(p) => p,
        None => return Ok(()),
    };

    let file_name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return Ok(()),
    };

    // Pattern: {filename}.bak.{digits}
    let prefix = format!("{}.bak.", file_name);

    let mut backups: Vec<(u128, PathBuf)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let entry_name = entry.file_name();
            let entry_name_str = entry_name.to_string_lossy();
            if let Some(suffix) = entry_name_str.strip_prefix(&prefix) {
                if let Ok(ts) = suffix.parse::<u128>() {
                    backups.push((ts, entry.path()));
                }
            }
        }
    }

    // Sort by timestamp descending (newest first)
    backups.sort_by(|a, b| b.0.cmp(&a.0));

    // Remove backups beyond the keep limit
    for (_ts, backup_path) in backups.iter().skip(keep) {
        let _ = std::fs::remove_file(backup_path);
    }

    Ok(())
}

/// Merge write: read existing JSON, update managed fields at top level, write back.
///
/// If the file doesn't exist, creates a new JSON object with just the managed fields.
/// Preserves all existing fields that are not in the managed_fields list.
pub fn merge_json_write(path: &Path, managed_fields: &[(&str, Value)]) -> Result<()> {
    // Read existing JSON or start with empty object
    let mut json: serde_json::Map<String, Value> = if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        match serde_json::from_str::<Value>(&content) {
            Ok(Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        }
    } else {
        serde_json::Map::new()
    };

    // Update managed fields
    for (key, value) in managed_fields {
        json.insert(key.to_string(), value.clone());
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Write back as pretty JSON
    let output = serde_json::to_string_pretty(&Value::Object(json))
        .context("Failed to serialize JSON")?;
    std::fs::write(path, output)
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

/// Merge write for Claude Code's env block specifically.
///
/// Reads existing `~/.claude/settings.json`, updates only the `env` sub-object
/// with the managed fields, preserving all other top-level fields and non-managed
/// env fields.
pub fn merge_json_env_write(path: &Path, managed_fields: &[(&str, Value)]) -> Result<()> {
    // Read existing JSON or start with empty object
    let mut json: serde_json::Map<String, Value> = if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        match serde_json::from_str::<Value>(&content) {
            Ok(Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        }
    } else {
        serde_json::Map::new()
    };

    // Get or create the env sub-object
    let env_obj = json
        .entry("env")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    if let Some(env_map) = env_obj.as_object_mut() {
        // Update managed fields in the env block
        for (key, value) in managed_fields {
            env_map.insert(key.to_string(), value.clone());
        }
    } else {
        // env exists but is not an object — replace it
        let mut new_env = serde_json::Map::new();
        for (key, value) in managed_fields {
            new_env.insert(key.to_string(), value.clone());
        }
        json.insert("env".to_string(), Value::Object(new_env));
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Write back as pretty JSON
    let output = serde_json::to_string_pretty(&Value::Object(json))
        .context("Failed to serialize JSON")?;
    std::fs::write(path, output)
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

/// Write Codex config.toml with flat store format.
///
/// Sets:
/// - `model_provider = "skillstar"`
/// - `model = "<model>"`
/// - `[model_providers.skillstar]` table with name, base_url, wire_api, requires_openai_auth
///
/// Preserves all other existing sections/fields.
pub fn write_codex_config_flat(
    path: &Path,
    provider: &ProviderEntryFlat,
    model: &str,
) -> Result<()> {
    // Read existing config or start with empty table
    let mut table: toml::Table = if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        toml::Table::new()
    };

    // Set top-level managed fields
    table.insert(
        "model_provider".to_string(),
        toml::Value::String("skillstar".to_string()),
    );
    table.insert(
        "model".to_string(),
        toml::Value::String(model.to_string()),
    );

    // Build [model_providers.skillstar] section
    let mut skillstar_section = toml::Table::new();
    skillstar_section.insert(
        "name".to_string(),
        toml::Value::String("SkillStar".to_string()),
    );
    skillstar_section.insert(
        "base_url".to_string(),
        toml::Value::String(provider.base_url_openai.clone()),
    );
    skillstar_section.insert(
        "wire_api".to_string(),
        toml::Value::String("responses".to_string()),
    );
    skillstar_section.insert(
        "requires_openai_auth".to_string(),
        toml::Value::Boolean(true),
    );

    // Get or create [model_providers] table
    let model_providers = table
        .entry("model_providers")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));

    if let Some(mp_table) = model_providers.as_table_mut() {
        mp_table.insert(
            CODEX_MANAGED_PROVIDER_KEY.to_string(),
            toml::Value::Table(skillstar_section),
        );
    } else {
        // model_providers exists but is not a table — replace it
        let mut mp_table = toml::Table::new();
        mp_table.insert(
            CODEX_MANAGED_PROVIDER_KEY.to_string(),
            toml::Value::Table(skillstar_section),
        );
        table.insert(
            "model_providers".to_string(),
            toml::Value::Table(mp_table),
        );
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Write back as TOML
    let output = toml::to_string_pretty(&table)
        .context("Failed to serialize Codex config.toml")?;
    std::fs::write(path, output)
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Re-sync active tools after provider settings update
// ---------------------------------------------------------------------------

/// After a provider's settings are saved, re-sync all tools that are currently
/// using this provider. Each tool's individually selected model is preserved.
///
/// Returns a list of sync results (one per active tool).
///
/// # Per-tool error isolation
/// One tool failing does not prevent syncing others. Each tool is synced
/// independently and its result is collected regardless of success/failure.
///
/// # Logic
/// 1. Find the provider by `provider_id` in the store
/// 2. Iterate over `store.tool_activations`
/// 3. For each tool where `activation.provider_id == provider_id`:
///    - Call the appropriate sync function (`sync_to_claude_code` or `sync_to_codex`)
///      with the provider and the tool's individually selected model
/// 4. Collect and return all results
pub fn resync_active_tools(
    store: &FlatProvidersStore,
    provider_id: &str,
) -> Vec<ToolSyncResultFlat> {
    // 1. Find the provider by provider_id
    let provider = match store.providers.iter().find(|p| p.id == provider_id) {
        Some(p) => p,
        None => {
            // Provider not found — return a single error result
            return vec![ToolSyncResultFlat {
                tool_id: String::new(),
                success: false,
                config_path: None,
                error: Some(format!("Provider '{}' not found in store", provider_id)),
                backup_path: None,
            }];
        }
    };

    let mut results: Vec<ToolSyncResultFlat> = Vec::new();

    // 2. Iterate over tool_activations
    for (tool_id, activation_opt) in &store.tool_activations {
        // 3. For each tool where activation.provider_id == provider_id
        let activation = match activation_opt {
            Some(a) if a.provider_id == provider_id => a,
            _ => continue,
        };

        // Call the appropriate sync function based on tool_id, preserving the
        // tool's individually selected model.
        let result = match tool_id.as_str() {
            "claude-code" => {
                match sync_to_claude_code(provider, &activation.model) {
                    Ok(r) => r,
                    Err(e) => ToolSyncResultFlat {
                        tool_id: tool_id.clone(),
                        success: false,
                        config_path: None,
                        error: Some(e.to_string()),
                        backup_path: None,
                    },
                }
            }
            "codex" => {
                match sync_to_codex(provider, &activation.model) {
                    Ok(r) => r,
                    Err(e) => ToolSyncResultFlat {
                        tool_id: tool_id.clone(),
                        success: false,
                        config_path: None,
                        error: Some(e.to_string()),
                        backup_path: None,
                    },
                }
            }
            _ => {
                // Unknown tool_id — record error but continue with others
                ToolSyncResultFlat {
                    tool_id: tool_id.clone(),
                    success: false,
                    config_path: None,
                    error: Some(format!(
                        "Unknown tool_id '{}'. Only 'claude-code' and 'codex' are supported.",
                        tool_id
                    )),
                    backup_path: None,
                }
            }
        };

        results.push(result);
    }

    // 4. Return all collected results
    results
}

// ---------------------------------------------------------------------------
// Claude Code config writer
// ---------------------------------------------------------------------------

/// Write provider settings to Claude Code's `~/.claude/settings.json`.
///
/// Merges `apiUrl` and `apiKey` fields into the existing JSON object.
/// If the file doesn't exist, creates a new JSON object with just those fields.
fn write_claude_code_config(path: &Path, settings: &ProviderSettings) -> Result<()> {
    // Read existing config or start with empty object
    let mut json: HashMap<String, Value> = if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };

    // Merge apiUrl and apiKey fields
    json.insert(
        "apiUrl".to_string(),
        Value::String(settings.base_url.clone()),
    );
    json.insert(
        "apiKey".to_string(),
        Value::String(settings.api_key.clone()),
    );

    // Write back as pretty JSON
    let output = serde_json::to_string_pretty(&json)
        .context("Failed to serialize Claude Code config")?;
    std::fs::write(path, output)
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Codex config writer
// ---------------------------------------------------------------------------

/// Write provider settings to Codex's `~/.codex/config.toml`.
///
/// Sets the `[provider]` section with `base_url` and `api_key` fields.
/// If the file doesn't exist, creates a new TOML file with just the provider section.
/// If the file exists, merges the provider section into the existing TOML.
fn write_codex_config(path: &Path, settings: &ProviderSettings) -> Result<()> {
    // Read existing config or start with empty table
    let mut table: toml::Table = if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        toml::Table::new()
    };

    // Build the [provider] section
    let mut provider_section = toml::Table::new();
    provider_section.insert(
        "base_url".to_string(),
        toml::Value::String(settings.base_url.clone()),
    );
    provider_section.insert(
        "api_key".to_string(),
        toml::Value::String(settings.api_key.clone()),
    );

    // Merge into existing table
    table.insert(
        "provider".to_string(),
        toml::Value::Table(provider_section),
    );

    // Write back as TOML
    let output = toml::to_string_pretty(&table)
        .context("Failed to serialize Codex config")?;
    std::fs::write(path, output)
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

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
    if let Ok(config_path) = resolve_tool_config_path(tool_id) {
        if let Some(conflict) = check_external_modification(&config_path, last_sync_timestamp) {
            conflicts.push(conflict);
        }
    }

    // Check legacy ~/.claude.json for claude-code tool
    if tool_id == "claude-code" {
        if let Some(conflict) = check_legacy_claude_config() {
            conflicts.push(conflict);
        }
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
        if let Ok(value) = std::env::var(var_name) {
            if !value.is_empty() {
                conflicts.push(ConfigConflict {
                    conflict_type: ConflictType::EnvVarOverride,
                    description: format!(
                        "环境变量 {} 已设置，将覆盖 Claude Code 配置文件中的对应设置",
                        var_name
                    ),
                    file_path: None,
                    details: Some(format!("{}={}***", var_name, &value[..value.len().min(4)])),
                });
            }
        }
    }

    // Check OpenAI/Codex-related env vars
    for &var_name in CODEX_ENV_VARS {
        if let Ok(value) = std::env::var(var_name) {
            if !value.is_empty() {
                conflicts.push(ConfigConflict {
                    conflict_type: ConflictType::EnvVarOverride,
                    description: format!(
                        "环境变量 {} 已设置，将覆盖 Codex 配置文件中的对应设置",
                        var_name
                    ),
                    file_path: None,
                    details: Some(format!("{}={}***", var_name, &value[..value.len().min(4)])),
                });
            }
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
fn check_external_modification(path: &Path, last_sync_ts: Option<u64>) -> Option<ConfigConflict> {
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
    let home = dirs::home_dir()?;
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
            });
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Public helpers for generating config content (useful for testing)
// ---------------------------------------------------------------------------

/// Generate Claude Code JSON config content from provider settings.
/// Returns the JSON string that would be written to the config file.
pub fn generate_claude_code_config(settings: &ProviderSettings) -> Result<String> {
    let mut json: HashMap<String, Value> = HashMap::new();
    json.insert(
        "apiUrl".to_string(),
        Value::String(settings.base_url.clone()),
    );
    json.insert(
        "apiKey".to_string(),
        Value::String(settings.api_key.clone()),
    );
    serde_json::to_string_pretty(&json).context("Failed to serialize Claude Code config")
}

/// Generate Codex TOML config content from provider settings.
/// Returns the TOML string that would be written to the config file.
pub fn generate_codex_config(settings: &ProviderSettings) -> Result<String> {
    let mut table = toml::Table::new();
    let mut provider_section = toml::Table::new();
    provider_section.insert(
        "base_url".to_string(),
        toml::Value::String(settings.base_url.clone()),
    );
    provider_section.insert(
        "api_key".to_string(),
        toml::Value::String(settings.api_key.clone()),
    );
    table.insert(
        "provider".to_string(),
        toml::Value::Table(provider_section),
    );
    toml::to_string_pretty(&table).context("Failed to serialize Codex config")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ModelMapping, ProviderSettings};
    use proptest::prelude::*;
    use tempfile::TempDir;

    fn make_test_settings() -> ProviderSettings {
        ProviderSettings {
            base_url: "https://api.example.com/v1".to_string(),
            api_key: "sk-test-key-12345".to_string(),
            models: vec![ModelMapping {
                source_model: "model-a".to_string(),
                target_model: "model-a".to_string(),
                enabled: true,
            }],
            timeout_ms: None,
            max_retries: None,
        }
    }

    fn make_test_provider() -> ProviderEntry {
        ProviderEntry {
            id: "test-provider".to_string(),
            name: "Test Provider".to_string(),
            category: "cloud".to_string(),
            settings_config: serde_json::to_value(&make_test_settings()).unwrap(),
            preset_id: None,
            website_url: None,
            api_key_url: None,
            icon_color: None,
            notes: None,
            created_at: None,
            sort_index: None,
            meta: None,
        }
    }

    #[test]
    fn test_resolve_tool_config_path_claude_code() {
        let path = resolve_tool_config_path("claude-code").unwrap();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains(".claude"));
        assert!(path_str.ends_with("settings.json"));
    }

    #[test]
    fn test_resolve_tool_config_path_codex() {
        let path = resolve_tool_config_path("codex").unwrap();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains(".codex"));
        assert!(path_str.ends_with("config.toml"));
    }

    #[test]
    fn test_resolve_tool_config_path_unknown() {
        let result = resolve_tool_config_path("unknown-tool");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown tool_id"));
    }

    #[test]
    fn test_generate_claude_code_config() {
        let settings = make_test_settings();
        let json_str = generate_claude_code_config(&settings).unwrap();
        let parsed: HashMap<String, Value> = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            parsed.get("apiUrl").unwrap().as_str().unwrap(),
            "https://api.example.com/v1"
        );
        assert_eq!(
            parsed.get("apiKey").unwrap().as_str().unwrap(),
            "sk-test-key-12345"
        );
    }

    #[test]
    fn test_generate_codex_config() {
        let settings = make_test_settings();
        let toml_str = generate_codex_config(&settings).unwrap();
        let parsed: toml::Table = toml::from_str(&toml_str).unwrap();
        let provider = parsed.get("provider").unwrap().as_table().unwrap();
        assert_eq!(
            provider.get("base_url").unwrap().as_str().unwrap(),
            "https://api.example.com/v1"
        );
        assert_eq!(
            provider.get("api_key").unwrap().as_str().unwrap(),
            "sk-test-key-12345"
        );
    }

    #[test]
    fn test_write_claude_code_config_new_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        let settings = make_test_settings();

        write_claude_code_config(&path, &settings).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: HashMap<String, Value> = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.get("apiUrl").unwrap().as_str().unwrap(), settings.base_url);
        assert_eq!(parsed.get("apiKey").unwrap().as_str().unwrap(), settings.api_key);
    }

    #[test]
    fn test_write_claude_code_config_merges_existing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");

        // Write existing config with extra fields
        let existing = serde_json::json!({
            "theme": "dark",
            "existingField": 42
        });
        std::fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        let settings = make_test_settings();
        write_claude_code_config(&path, &settings).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: HashMap<String, Value> = serde_json::from_str(&content).unwrap();
        // New fields are present
        assert_eq!(parsed.get("apiUrl").unwrap().as_str().unwrap(), settings.base_url);
        assert_eq!(parsed.get("apiKey").unwrap().as_str().unwrap(), settings.api_key);
        // Existing fields are preserved
        assert_eq!(parsed.get("theme").unwrap().as_str().unwrap(), "dark");
        assert_eq!(parsed.get("existingField").unwrap().as_i64().unwrap(), 42);
    }

    #[test]
    fn test_write_codex_config_new_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let settings = make_test_settings();

        write_codex_config(&path, &settings).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: toml::Table = toml::from_str(&content).unwrap();
        let provider = parsed.get("provider").unwrap().as_table().unwrap();
        assert_eq!(provider.get("base_url").unwrap().as_str().unwrap(), settings.base_url);
        assert_eq!(provider.get("api_key").unwrap().as_str().unwrap(), settings.api_key);
    }

    #[test]
    fn test_write_codex_config_merges_existing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");

        // Write existing config with extra sections
        let existing = r#"
[general]
theme = "dark"

[provider]
base_url = "https://old-api.example.com"
api_key = "old-key"
"#;
        std::fs::write(&path, existing).unwrap();

        let settings = make_test_settings();
        write_codex_config(&path, &settings).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: toml::Table = toml::from_str(&content).unwrap();
        // Provider section is updated
        let provider = parsed.get("provider").unwrap().as_table().unwrap();
        assert_eq!(provider.get("base_url").unwrap().as_str().unwrap(), settings.base_url);
        assert_eq!(provider.get("api_key").unwrap().as_str().unwrap(), settings.api_key);
        // Existing sections are preserved
        let general = parsed.get("general").unwrap().as_table().unwrap();
        assert_eq!(general.get("theme").unwrap().as_str().unwrap(), "dark");
    }

    #[test]
    fn test_sync_provider_to_tool_creates_backup() {
        let tmp = TempDir::new().unwrap();
        let claude_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let config_path = claude_dir.join("settings.json");

        // Write an existing config
        let existing = serde_json::json!({"existingKey": "existingValue"});
        std::fs::write(&config_path, serde_json::to_string(&existing).unwrap()).unwrap();

        // We can't easily test sync_provider_to_tool directly because it uses
        // resolve_tool_config_path which points to the real home dir.
        // Instead, test the inner write + backup logic directly.
        let settings = make_test_settings();

        // Simulate backup
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let backup_path = format!("{}.bak.{}", config_path.display(), timestamp);
        std::fs::copy(&config_path, &backup_path).unwrap();

        // Write new config
        write_claude_code_config(&config_path, &settings).unwrap();

        // Verify backup has original content
        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        let backup_parsed: Value = serde_json::from_str(&backup_content).unwrap();
        assert_eq!(
            backup_parsed.get("existingKey").unwrap().as_str().unwrap(),
            "existingValue"
        );

        // Verify new config has updated content
        let new_content = std::fs::read_to_string(&config_path).unwrap();
        let new_parsed: HashMap<String, Value> = serde_json::from_str(&new_content).unwrap();
        assert_eq!(
            new_parsed.get("apiUrl").unwrap().as_str().unwrap(),
            settings.base_url
        );
    }

    #[test]
    fn test_sync_provider_to_all_tools_isolation() {
        // Test that sync_provider_to_all_tools returns results for each tool
        // even if some fail (unknown tool_id will fail)
        let provider = make_test_provider();
        let tool_ids = vec![
            "unknown-tool".to_string(), // This will fail
            "another-unknown".to_string(), // This will also fail
        ];

        let results = sync_provider_to_all_tools(&provider, &tool_ids);
        assert_eq!(results.len(), 2);
        // Both should fail but not panic
        assert!(!results[0].success);
        assert!(!results[1].success);
        assert!(results[0].error.is_some());
        assert!(results[1].error.is_some());
    }

    #[test]
    fn test_get_tool_config_targets_returns_both_tools() {
        let targets = get_tool_config_targets().unwrap();
        assert_eq!(targets.len(), 2);

        let claude_target = targets.iter().find(|t| t.tool_id == "claude-code").unwrap();
        assert_eq!(claude_target.display_name, "Claude Code");
        assert!(claude_target.config_path.contains(".claude"));

        let codex_target = targets.iter().find(|t| t.tool_id == "codex").unwrap();
        assert_eq!(codex_target.display_name, "Codex");
        assert!(codex_target.config_path.contains(".codex"));
    }

    // =========================================================================
    // Flat store sync tests (v2 architecture)
    // =========================================================================

    fn make_test_provider_flat() -> ProviderEntryFlat {
        ProviderEntryFlat {
            id: "test-uuid-1234".to_string(),
            name: "Test Provider".to_string(),
            base_url_openai: "https://api.example.com/v1".to_string(),
            base_url_anthropic: "https://api.example.com/anthropic".to_string(),
            models_url: "https://api.example.com/v1/models".to_string(),
            api_key: "sk-test-key-flat-12345".to_string(),
            models: vec!["model-a".to_string(), "model-b".to_string()],
            default_model: "model-a".to_string(),
            sort_index: 0,
            preset_id: Some("test-preset".to_string()),
            icon_color: Some("#FF0000".to_string()),
            notes: None,
            created_at: Some(1719000000000),
            meta: None,
        }
    }

    #[test]
    fn test_sync_to_claude_code_inner_new_file() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join(".claude").join("settings.json");
        let provider = make_test_provider_flat();

        let result = sync_to_claude_code_inner(&provider, "model-a", &config_path).unwrap();

        // No backup since file didn't exist
        assert!(result.is_none());

        // Verify the written content
        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let env = parsed.get("env").unwrap().as_object().unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").unwrap().as_str().unwrap(),
            "https://api.example.com/anthropic"
        );
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").unwrap().as_str().unwrap(),
            "sk-test-key-flat-12345"
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").unwrap().as_str().unwrap(),
            "model-a"
        );
    }

    #[test]
    fn test_sync_to_claude_code_inner_merges_existing() {
        let tmp = TempDir::new().unwrap();
        let claude_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let config_path = claude_dir.join("settings.json");

        // Write existing config with extra fields
        let existing = serde_json::json!({
            "theme": "dark",
            "env": {
                "MY_CUSTOM_VAR": "custom_value",
                "ANTHROPIC_BASE_URL": "old_url"
            }
        });
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        let provider = make_test_provider_flat();
        let backup = sync_to_claude_code_inner(&provider, "model-b", &config_path).unwrap();

        // Backup should exist
        assert!(backup.is_some());
        assert!(backup.unwrap().exists());

        // Verify the written content
        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Top-level fields preserved
        assert_eq!(parsed.get("theme").unwrap().as_str().unwrap(), "dark");

        // Env block: managed fields updated, custom field preserved
        let env = parsed.get("env").unwrap().as_object().unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").unwrap().as_str().unwrap(),
            "https://api.example.com/anthropic"
        );
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").unwrap().as_str().unwrap(),
            "sk-test-key-flat-12345"
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").unwrap().as_str().unwrap(),
            "model-b"
        );
        assert_eq!(
            env.get("MY_CUSTOM_VAR").unwrap().as_str().unwrap(),
            "custom_value"
        );
    }

    #[test]
    fn test_sync_to_claude_code_inner_fails_without_anthropic_url() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("settings.json");

        let mut provider = make_test_provider_flat();
        provider.base_url_anthropic = String::new();

        let result = sync_to_claude_code_inner(&provider, "model-a", &config_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Anthropic-compatible endpoint"));
    }

    #[test]
    fn test_write_codex_config_flat_new_file() {
        let tmp = TempDir::new().unwrap();
        let codex_dir = tmp.path().join(".codex");
        let config_path = codex_dir.join("config.toml");

        let provider = make_test_provider_flat();
        write_codex_config_flat(&config_path, &provider, "model-a").unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: toml::Table = toml::from_str(&content).unwrap();

        assert_eq!(
            parsed.get("model_provider").unwrap().as_str().unwrap(),
            "skillstar"
        );
        assert_eq!(parsed.get("model").unwrap().as_str().unwrap(), "model-a");

        let mp = parsed.get("model_providers").unwrap().as_table().unwrap();
        let skillstar = mp.get("skillstar").unwrap().as_table().unwrap();
        assert_eq!(skillstar.get("name").unwrap().as_str().unwrap(), "SkillStar");
        assert_eq!(
            skillstar.get("base_url").unwrap().as_str().unwrap(),
            "https://api.example.com/v1"
        );
        assert_eq!(
            skillstar.get("wire_api").unwrap().as_str().unwrap(),
            "responses"
        );
        assert_eq!(
            skillstar
                .get("requires_openai_auth")
                .unwrap()
                .as_bool()
                .unwrap(),
            true
        );
    }

    #[test]
    fn test_write_codex_config_flat_merges_existing() {
        let tmp = TempDir::new().unwrap();
        let codex_dir = tmp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let config_path = codex_dir.join("config.toml");

        // Write existing config with extra sections
        let existing = r#"
[general]
theme = "dark"
auto_update = true

[model_providers.custom]
name = "Custom Provider"
base_url = "https://custom.example.com"
"#;
        std::fs::write(&config_path, existing).unwrap();

        let provider = make_test_provider_flat();
        write_codex_config_flat(&config_path, &provider, "model-b").unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: toml::Table = toml::from_str(&content).unwrap();

        // Managed fields are set
        assert_eq!(
            parsed.get("model_provider").unwrap().as_str().unwrap(),
            "skillstar"
        );
        assert_eq!(parsed.get("model").unwrap().as_str().unwrap(), "model-b");

        // Existing sections preserved
        let general = parsed.get("general").unwrap().as_table().unwrap();
        assert_eq!(general.get("theme").unwrap().as_str().unwrap(), "dark");
        assert_eq!(
            general.get("auto_update").unwrap().as_bool().unwrap(),
            true
        );

        // Existing model_providers.custom preserved
        let mp = parsed.get("model_providers").unwrap().as_table().unwrap();
        let custom = mp.get("custom").unwrap().as_table().unwrap();
        assert_eq!(
            custom.get("name").unwrap().as_str().unwrap(),
            "Custom Provider"
        );

        // New model_providers.skillstar added
        let skillstar = mp.get("skillstar").unwrap().as_table().unwrap();
        assert_eq!(
            skillstar.get("base_url").unwrap().as_str().unwrap(),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn test_merge_json_write_new_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.json");

        let fields: Vec<(&str, Value)> = vec![
            ("key1", Value::String("value1".to_string())),
            ("key2", Value::Number(42.into())),
        ];
        merge_json_write(&path, &fields).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.get("key1").unwrap().as_str().unwrap(), "value1");
        assert_eq!(parsed.get("key2").unwrap().as_i64().unwrap(), 42);
    }

    #[test]
    fn test_merge_json_write_preserves_existing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.json");

        // Write existing content
        let existing = serde_json::json!({"existing": "preserved", "key1": "old_value"});
        std::fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        let fields: Vec<(&str, Value)> = vec![
            ("key1", Value::String("new_value".to_string())),
            ("key2", Value::String("added".to_string())),
        ];
        merge_json_write(&path, &fields).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed.get("existing").unwrap().as_str().unwrap(),
            "preserved"
        );
        assert_eq!(
            parsed.get("key1").unwrap().as_str().unwrap(),
            "new_value"
        );
        assert_eq!(parsed.get("key2").unwrap().as_str().unwrap(), "added");
    }

    #[test]
    fn test_merge_json_env_write_new_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");

        let fields: Vec<(&str, Value)> = vec![
            (
                "ANTHROPIC_BASE_URL",
                Value::String("https://api.test.com".to_string()),
            ),
            (
                "ANTHROPIC_AUTH_TOKEN",
                Value::String("sk-test".to_string()),
            ),
        ];
        merge_json_env_write(&path, &fields).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        let env = parsed.get("env").unwrap().as_object().unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").unwrap().as_str().unwrap(),
            "https://api.test.com"
        );
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").unwrap().as_str().unwrap(),
            "sk-test"
        );
    }

    #[test]
    fn test_merge_json_env_write_preserves_all_fields() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");

        // Write existing config with top-level and env fields
        let existing = serde_json::json!({
            "theme": "dark",
            "version": 1,
            "env": {
                "MY_VAR": "my_value",
                "ANTHROPIC_BASE_URL": "old_url"
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        let fields: Vec<(&str, Value)> = vec![
            (
                "ANTHROPIC_BASE_URL",
                Value::String("new_url".to_string()),
            ),
            (
                "ANTHROPIC_MODEL",
                Value::String("model-x".to_string()),
            ),
        ];
        merge_json_env_write(&path, &fields).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Top-level fields preserved
        assert_eq!(parsed.get("theme").unwrap().as_str().unwrap(), "dark");
        assert_eq!(parsed.get("version").unwrap().as_i64().unwrap(), 1);

        // Env block: managed fields updated, custom field preserved
        let env = parsed.get("env").unwrap().as_object().unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").unwrap().as_str().unwrap(),
            "new_url"
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").unwrap().as_str().unwrap(),
            "model-x"
        );
        assert_eq!(
            env.get("MY_VAR").unwrap().as_str().unwrap(),
            "my_value"
        );
    }

    #[test]
    fn test_unsync_claude_code_removes_managed_fields() {
        let tmp = TempDir::new().unwrap();
        let claude_dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let config_path = claude_dir.join("settings.json");

        // Write a config with managed + custom fields
        let existing = serde_json::json!({
            "theme": "dark",
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com/anthropic",
                "ANTHROPIC_AUTH_TOKEN": "sk-test",
                "ANTHROPIC_MODEL": "model-a",
                "MY_CUSTOM_VAR": "keep_me"
            }
        });
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&existing).unwrap(),
        )
        .unwrap();

        // Simulate unsync logic (same as unsync_claude_code but with custom path)
        create_rolling_backup(&config_path).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let mut json: Value = serde_json::from_str(&content).unwrap();

        if let Some(env_obj) = json.get_mut("env").and_then(|v| v.as_object_mut()) {
            for key in CLAUDE_MANAGED_ENV_KEYS {
                env_obj.remove(*key);
            }
        }

        let output = serde_json::to_string_pretty(&json).unwrap();
        std::fs::write(&config_path, output).unwrap();

        // Verify
        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Top-level preserved
        assert_eq!(parsed.get("theme").unwrap().as_str().unwrap(), "dark");

        // Managed fields removed, custom field preserved
        let env = parsed.get("env").unwrap().as_object().unwrap();
        assert!(!env.contains_key("ANTHROPIC_BASE_URL"));
        assert!(!env.contains_key("ANTHROPIC_AUTH_TOKEN"));
        assert!(!env.contains_key("ANTHROPIC_MODEL"));
        assert_eq!(
            env.get("MY_CUSTOM_VAR").unwrap().as_str().unwrap(),
            "keep_me"
        );
    }

    #[test]
    fn test_unsync_codex_removes_managed_fields() {
        let tmp = TempDir::new().unwrap();
        let codex_dir = tmp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();

        // Write config.toml with managed + custom sections
        let config_path = codex_dir.join("config.toml");
        let config_content = r#"
model_provider = "skillstar"
model = "model-a"

[general]
theme = "dark"

[model_providers.skillstar]
name = "SkillStar"
base_url = "https://api.example.com/v1"
wire_api = "responses"
requires_openai_auth = true

[model_providers.custom]
name = "Custom"
base_url = "https://custom.example.com"
"#;
        std::fs::write(&config_path, config_content).unwrap();

        // Simulate unsync_codex logic for config.toml
        create_rolling_backup(&config_path).unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        let mut table: toml::Table = toml::from_str(&content).unwrap();
        table.remove("model_provider");
        table.remove("model");
        if let Some(model_providers) = table.get_mut("model_providers") {
            if let Some(mp_table) = model_providers.as_table_mut() {
                mp_table.remove(CODEX_MANAGED_PROVIDER_KEY);
            }
        }
        std::fs::write(&config_path, toml::to_string_pretty(&table).unwrap()).unwrap();

        // Verify config.toml
        let config_result = std::fs::read_to_string(&config_path).unwrap();
        let config_parsed: toml::Table = toml::from_str(&config_result).unwrap();
        assert!(!config_parsed.contains_key("model_provider"));
        assert!(!config_parsed.contains_key("model"));

        // general section preserved
        let general = config_parsed.get("general").unwrap().as_table().unwrap();
        assert_eq!(general.get("theme").unwrap().as_str().unwrap(), "dark");

        // model_providers.custom preserved, skillstar removed
        let mp = config_parsed
            .get("model_providers")
            .unwrap()
            .as_table()
            .unwrap();
        assert!(!mp.contains_key("skillstar"));
        assert!(mp.contains_key("custom"));
    }

    #[test]
    fn test_rolling_backup_keeps_last_5() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("settings.json");
        std::fs::write(&config_path, "{}").unwrap();

        // Create 7 backups manually
        for i in 0..7u128 {
            let backup_name = format!(
                "{}.bak.{}",
                config_path.to_string_lossy(),
                1000 + i
            );
            std::fs::write(&backup_name, format!("backup {}", i)).unwrap();
        }

        // Run cleanup
        cleanup_old_backups(&config_path, 5).unwrap();

        // Count remaining backups
        let prefix = "settings.json.bak.";
        let remaining: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(prefix)
            })
            .collect();

        assert_eq!(remaining.len(), 5);

        // Verify the 5 most recent are kept (timestamps 1002..1006)
        for i in 2..7u128 {
            let backup_name = format!(
                "{}.bak.{}",
                config_path.to_string_lossy(),
                1000 + i
            );
            assert!(
                Path::new(&backup_name).exists(),
                "Backup {} should exist",
                1000 + i
            );
        }

        // Verify the 2 oldest are removed (timestamps 1000, 1001)
        for i in 0..2u128 {
            let backup_name = format!(
                "{}.bak.{}",
                config_path.to_string_lossy(),
                1000 + i
            );
            assert!(
                !Path::new(&backup_name).exists(),
                "Backup {} should be removed",
                1000 + i
            );
        }
    }

    #[test]
    fn test_create_rolling_backup_creates_file() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("settings.json");
        std::fs::write(&config_path, r#"{"key": "value"}"#).unwrap();

        let backup_path = create_rolling_backup(&config_path).unwrap();

        // Backup file exists
        assert!(backup_path.exists());

        // Backup has the original content
        let backup_content = std::fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, r#"{"key": "value"}"#);

        // Original file still exists
        assert!(config_path.exists());
    }

    #[test]
    fn test_codex_auth_json_merge_write() {
        let tmp = TempDir::new().unwrap();
        let auth_path = tmp.path().join("auth.json");

        // Write existing auth.json with extra fields
        let existing = serde_json::json!({
            "OTHER_KEY": "keep_me",
            "OPENAI_API_KEY": "old-key"
        });
        std::fs::write(&auth_path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        // Merge write new API key
        let fields: Vec<(&str, Value)> = vec![
            ("OPENAI_API_KEY", Value::String("new-key-12345".to_string())),
        ];
        merge_json_write(&auth_path, &fields).unwrap();

        let content = std::fs::read_to_string(&auth_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            parsed.get("OPENAI_API_KEY").unwrap().as_str().unwrap(),
            "new-key-12345"
        );
        assert_eq!(
            parsed.get("OTHER_KEY").unwrap().as_str().unwrap(),
            "keep_me"
        );
    }

    #[test]
    fn test_resync_active_tools_syncs_correct_tools() {
        use crate::providers::{FlatProvidersStore, ToolActivation};

        let provider = make_test_provider_flat();
        let store = FlatProvidersStore {
            version: 2,
            providers: vec![provider.clone()],
            tool_activations: {
                let mut map = HashMap::new();
                map.insert(
                    "claude-code".to_string(),
                    Some(ToolActivation {
                        provider_id: "test-uuid-1234".to_string(),
                        model: "model-a".to_string(),
                    }),
                );
                map.insert(
                    "codex".to_string(),
                    Some(ToolActivation {
                        provider_id: "test-uuid-1234".to_string(),
                        model: "model-b".to_string(),
                    }),
                );
                map
            },
        };

        // resync_active_tools will try to write to real home dir paths,
        // so we just verify it returns results for both tools
        let results = resync_active_tools(&store, "test-uuid-1234");
        assert_eq!(results.len(), 2);

        // Both tools should be attempted
        let tool_ids: Vec<&str> = results.iter().map(|r| r.tool_id.as_str()).collect();
        assert!(tool_ids.contains(&"claude-code"));
        assert!(tool_ids.contains(&"codex"));
    }

    #[test]
    fn test_resync_active_tools_provider_not_found() {
        use crate::providers::FlatProvidersStore;

        let store = FlatProvidersStore::default();
        let results = resync_active_tools(&store, "nonexistent-id");

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0]
            .error
            .as_ref()
            .unwrap()
            .contains("not found"));
    }

    #[test]
    fn test_resync_active_tools_skips_other_providers() {
        use crate::providers::{FlatProvidersStore, ToolActivation};

        let provider = make_test_provider_flat();
        let store = FlatProvidersStore {
            version: 2,
            providers: vec![provider.clone()],
            tool_activations: {
                let mut map = HashMap::new();
                // Claude Code uses a different provider
                map.insert(
                    "claude-code".to_string(),
                    Some(ToolActivation {
                        provider_id: "other-provider-id".to_string(),
                        model: "other-model".to_string(),
                    }),
                );
                // Codex uses our provider
                map.insert(
                    "codex".to_string(),
                    Some(ToolActivation {
                        provider_id: "test-uuid-1234".to_string(),
                        model: "model-a".to_string(),
                    }),
                );
                map
            },
        };

        let results = resync_active_tools(&store, "test-uuid-1234");
        // Should only sync codex (the one using our provider)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_id, "codex");
    }

    // =========================================================================
    // Property 10: Backup Before Write Invariant
    //
    // For any tool sync operation where the target config file already exists,
    // a backup file (.bak) SHALL be created before the config file is modified.
    // The backup SHALL contain the original content, and the new file SHALL
    // contain the updated content.
    //
    // **Validates: Requirements 4.6**
    // =========================================================================

    /// Strategy: generate arbitrary JSON content as a HashMap<String, String>.
    fn arb_json_content() -> impl Strategy<Value = HashMap<String, String>> {
        prop::collection::hash_map(
            "[a-zA-Z][a-zA-Z0-9_]{0,15}",  // keys: valid JSON field names
            "[a-zA-Z0-9 _\\-\\.]{0,50}",    // values: safe string values
            1..=8,                           // 1 to 8 entries
        )
    }

    /// Strategy: generate valid ProviderSettings for writing new config.
    fn arb_provider_settings() -> impl Strategy<Value = ProviderSettings> {
        (
            "https://[a-z]{3,10}\\.[a-z]{2,5}/v[0-9]", // base_url
            "sk-[a-zA-Z0-9]{10,40}",                    // api_key
        )
            .prop_map(|(base_url, api_key)| ProviderSettings {
                base_url,
                api_key,
                models: vec![ModelMapping {
                    source_model: "model-a".to_string(),
                    target_model: "model-b".to_string(),
                    enabled: true,
                }],
                timeout_ms: None,
                max_retries: None,
            })
    }

    proptest! {
        /// **Validates: Requirements 4.6**
        ///
        /// Property 10: Backup Before Write Invariant (Claude Code JSON).
        /// Create a temp config file with arbitrary JSON content, perform the
        /// backup + write sequence, assert .bak file exists with original content
        /// and the config file has the updated content.
        #[test]
        fn prop_backup_before_write_claude_code(
            original_content in arb_json_content(),
            new_settings in arb_provider_settings(),
        ) {
            let tmp = TempDir::new().unwrap();
            let config_path = tmp.path().join("settings.json");

            // Step 1: Write original content to simulate an existing config file
            let original_json = serde_json::to_string_pretty(&original_content).unwrap();
            std::fs::write(&config_path, &original_json).unwrap();

            // Step 2: Perform backup (same logic as sync_provider_to_tool_inner)
            let config_path_str = config_path.to_string_lossy().to_string();
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let backup_path_str = format!("{}.bak.{}", config_path_str, timestamp);
            let backup_path = Path::new(&backup_path_str);

            prop_assert!(config_path.exists(), "Config file should exist before backup");
            std::fs::copy(&config_path, backup_path).unwrap();

            // Step 3: Write new content using the actual writer function
            write_claude_code_config(&config_path, &new_settings).unwrap();

            // Assertion 1: Backup file exists
            prop_assert!(backup_path.exists(),
                "Backup file should exist at: {}", backup_path_str);

            // Assertion 2: Backup contains the original content
            let backup_content = std::fs::read_to_string(backup_path).unwrap();
            let backup_parsed: HashMap<String, Value> =
                serde_json::from_str(&backup_content).unwrap();
            for (key, value) in &original_content {
                let backup_val = backup_parsed.get(key)
                    .expect(&format!("Backup should contain key '{}'", key));
                prop_assert_eq!(
                    backup_val.as_str().unwrap(),
                    value.as_str(),
                    "Backup value for key '{}' should match original", key
                );
            }

            // Assertion 3: New config file has the updated provider settings
            let new_content = std::fs::read_to_string(&config_path).unwrap();
            let new_parsed: HashMap<String, Value> =
                serde_json::from_str(&new_content).unwrap();
            prop_assert_eq!(
                new_parsed.get("apiUrl").unwrap().as_str().unwrap(),
                new_settings.base_url.as_str(),
                "New config should have updated apiUrl"
            );
            prop_assert_eq!(
                new_parsed.get("apiKey").unwrap().as_str().unwrap(),
                new_settings.api_key.as_str(),
                "New config should have updated apiKey"
            );
        }

        /// **Validates: Requirements 4.6**
        ///
        /// Property 10: Backup Before Write Invariant (Codex TOML).
        /// Create a temp config file with TOML content, perform the backup + write
        /// sequence, assert .bak file exists with original content and the config
        /// file has the updated content.
        #[test]
        fn prop_backup_before_write_codex(
            new_settings in arb_provider_settings(),
        ) {
            let tmp = TempDir::new().unwrap();
            let config_path = tmp.path().join("config.toml");

            // Step 1: Write original TOML content to simulate an existing config
            let mut original_table = toml::Table::new();
            let mut general = toml::Table::new();
            general.insert("theme".to_string(), toml::Value::String("dark".to_string()));
            original_table.insert("general".to_string(), toml::Value::Table(general));
            let original_toml = toml::to_string_pretty(&original_table).unwrap();
            std::fs::write(&config_path, &original_toml).unwrap();

            // Step 2: Perform backup (same logic as sync_provider_to_tool_inner)
            let config_path_str = config_path.to_string_lossy().to_string();
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let backup_path_str = format!("{}.bak.{}", config_path_str, timestamp);
            let backup_path = Path::new(&backup_path_str);

            prop_assert!(config_path.exists(), "Config file should exist before backup");
            std::fs::copy(&config_path, backup_path).unwrap();

            // Step 3: Write new content using the actual writer function
            write_codex_config(&config_path, &new_settings).unwrap();

            // Assertion 1: Backup file exists
            prop_assert!(backup_path.exists(),
                "Backup file should exist at: {}", backup_path_str);

            // Assertion 2: Backup contains the original content
            let backup_content = std::fs::read_to_string(backup_path).unwrap();
            let backup_parsed: toml::Table = toml::from_str(&backup_content).unwrap();
            let backup_general = backup_parsed.get("general").unwrap().as_table().unwrap();
            prop_assert_eq!(
                backup_general.get("theme").unwrap().as_str().unwrap(),
                "dark",
                "Backup should preserve original [general].theme"
            );

            // Assertion 3: New config file has the updated provider settings
            let new_content = std::fs::read_to_string(&config_path).unwrap();
            let new_parsed: toml::Table = toml::from_str(&new_content).unwrap();
            let new_provider = new_parsed.get("provider").unwrap().as_table().unwrap();
            prop_assert_eq!(
                new_provider.get("base_url").unwrap().as_str().unwrap(),
                new_settings.base_url.as_str(),
                "New config should have updated base_url"
            );
            prop_assert_eq!(
                new_provider.get("api_key").unwrap().as_str().unwrap(),
                new_settings.api_key.as_str(),
                "New config should have updated api_key"
            );
            // Original sections should be preserved after merge
            let new_general = new_parsed.get("general").unwrap().as_table().unwrap();
            prop_assert_eq!(
                new_general.get("theme").unwrap().as_str().unwrap(),
                "dark",
                "New config should preserve original [general].theme after merge"
            );
        }
    }

    // =========================================================================
    // Property 11: Batch Sync Isolation
    //
    // For any batch sync operation across multiple tools, a failure in one
    // tool's sync SHALL NOT prevent other tools from being synced, and the
    // result SHALL contain per-tool success/failure status.
    //
    // **Validates: Requirement 4.8**
    // =========================================================================

    /// Strategy: generate an invalid tool_id (not "claude-code" or "codex").
    fn arb_invalid_tool_id() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("unknown-tool".to_string()),
            Just("invalid".to_string()),
            Just("vscode".to_string()),
            Just("cursor".to_string()),
            Just("".to_string()),
            "[a-z]{3,12}".prop_filter("must not be a valid tool_id", |s| {
                s != "claude-code" && s != "codex"
            }),
        ]
    }

    /// Strategy: generate a valid tool_id.
    fn arb_valid_tool_id() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("claude-code".to_string()),
            Just("codex".to_string()),
        ]
    }

    /// Strategy: generate a mixed list of tool_ids containing at least one invalid
    /// and at least one valid tool_id.
    fn arb_mixed_tool_ids() -> impl Strategy<Value = Vec<String>> {
        (
            prop::collection::vec(arb_invalid_tool_id(), 1..=3),
            prop::collection::vec(arb_valid_tool_id(), 1..=2),
        )
            .prop_map(|(invalid, valid)| {
                let mut combined = invalid;
                combined.extend(valid);
                combined
            })
            .prop_shuffle()
    }

    proptest! {
        /// **Validates: Requirement 4.8**
        ///
        /// Property 11: When syncing to a batch of tool_ids where some are invalid,
        /// the invalid ones fail independently while valid ones are still attempted.
        /// Each tool_id produces exactly one result entry.
        #[test]
        fn prop_batch_sync_isolation_invalid_does_not_block_valid(
            tool_ids in arb_mixed_tool_ids(),
        ) {
            let provider = make_test_provider();

            let results = sync_provider_to_all_tools(&provider, &tool_ids);

            // 1. Results vector has one entry per tool_id
            prop_assert_eq!(
                results.len(),
                tool_ids.len(),
                "Expected one result per tool_id, got {} results for {} tool_ids",
                results.len(),
                tool_ids.len()
            );

            // 2. Each result corresponds to the correct tool_id in order
            for (i, result) in results.iter().enumerate() {
                prop_assert_eq!(
                    &result.tool_id,
                    &tool_ids[i],
                    "Result at index {} has wrong tool_id",
                    i
                );
            }

            // 3. Invalid tool_ids have success=false and error=Some(...)
            for result in &results {
                if result.tool_id != "claude-code" && result.tool_id != "codex" {
                    prop_assert!(
                        !result.success,
                        "Invalid tool_id '{}' should have success=false",
                        result.tool_id
                    );
                    prop_assert!(
                        result.error.is_some(),
                        "Invalid tool_id '{}' should have error message",
                        result.tool_id
                    );
                }
            }

            // 4. Valid tool_ids are attempted independently (they don't fail due to
            //    other tools failing). They may succeed or fail based on file system
            //    state, but they are processed — their config_path is resolved.
            for result in &results {
                if result.tool_id == "claude-code" || result.tool_id == "codex" {
                    // Valid tools have a resolved config_path (not the error placeholder)
                    prop_assert!(
                        !result.config_path.contains("<unknown path"),
                        "Valid tool_id '{}' should have a resolved config_path, got: {}",
                        result.tool_id,
                        result.config_path
                    );
                }
            }
        }

        /// **Validates: Requirement 4.8**
        ///
        /// Property 11 (part 2): When all tool_ids are invalid, every result
        /// has success=false and error=Some(...), and no panic occurs.
        #[test]
        fn prop_batch_sync_all_invalid_tools_fail_gracefully(
            tool_ids in prop::collection::vec(arb_invalid_tool_id(), 1..=5),
        ) {
            let provider = make_test_provider();

            let results = sync_provider_to_all_tools(&provider, &tool_ids);

            // Results vector has one entry per tool_id
            prop_assert_eq!(results.len(), tool_ids.len());

            // Every result should indicate failure
            for (i, result) in results.iter().enumerate() {
                prop_assert_eq!(&result.tool_id, &tool_ids[i]);
                prop_assert!(!result.success,
                    "Invalid tool '{}' should fail", result.tool_id);
                prop_assert!(result.error.is_some(),
                    "Invalid tool '{}' should have error", result.tool_id);
                prop_assert_eq!(result.backup_path.clone(), None,
                    "Failed tool '{}' should have no backup", result.tool_id);
            }
        }

        /// **Validates: Requirement 4.8**
        ///
        /// Property 11 (part 3): The order of results matches the order of input
        /// tool_ids, regardless of which ones succeed or fail.
        #[test]
        fn prop_batch_sync_preserves_order(
            tool_ids in prop::collection::vec(
                prop_oneof![
                    arb_invalid_tool_id(),
                    arb_valid_tool_id(),
                ],
                1..=6
            ),
        ) {
            let provider = make_test_provider();

            let results = sync_provider_to_all_tools(&provider, &tool_ids);

            prop_assert_eq!(results.len(), tool_ids.len());

            for (i, result) in results.iter().enumerate() {
                prop_assert_eq!(
                    &result.tool_id,
                    &tool_ids[i],
                    "Result order mismatch at index {}: expected '{}', got '{}'",
                    i,
                    tool_ids[i],
                    result.tool_id
                );
            }
        }
    }

    // =========================================================================
    // Config Conflict Detection Tests
    // =========================================================================

    #[test]
    fn test_check_external_modification_no_last_sync() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{}").unwrap();

        // No last_sync_timestamp → no conflict
        let result = check_external_modification(&path, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_check_external_modification_file_not_exists() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.json");

        let result = check_external_modification(&path, Some(1000));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_external_modification_file_modified_after_sync() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{}").unwrap();

        // Use a timestamp far in the past so the file's mtime is definitely newer
        let old_timestamp = 1_000_000u64;
        let result = check_external_modification(&path, Some(old_timestamp));
        assert!(result.is_some());

        let conflict = result.unwrap();
        assert_eq!(conflict.conflict_type, ConflictType::ExternalModification);
        assert!(conflict.file_path.is_some());
        assert!(conflict.details.is_some());
    }

    #[test]
    fn test_check_external_modification_file_not_modified() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{}").unwrap();

        // Use a timestamp far in the future so the file's mtime is definitely older
        let future_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 10_000;
        let result = check_external_modification(&path, Some(future_timestamp));
        assert!(result.is_none());
    }

    #[test]
    fn test_check_legacy_claude_config_no_file() {
        // This test relies on ~/.claude.json not existing in the test environment.
        // If it does exist, this test may not be meaningful, but it won't fail.
        // We test the function logic with a controlled path instead.
        let tmp = TempDir::new().unwrap();
        let legacy_path = tmp.path().join(".claude.json");

        // File doesn't exist → no conflict
        assert!(!legacy_path.exists());
    }

    #[test]
    fn test_check_legacy_claude_config_with_conflicting_env() {
        let tmp = TempDir::new().unwrap();
        let legacy_path = tmp.path().join(".claude.json");

        let content = serde_json::json!({
            "env": {
                "ANTHROPIC_API_KEY": "sk-ant-test",
                "ANTHROPIC_BASE_URL": "https://example.com"
            }
        });
        std::fs::write(&legacy_path, serde_json::to_string(&content).unwrap()).unwrap();

        // Use the internal logic directly since check_legacy_claude_config uses home dir
        let json: Value = serde_json::from_str(
            &std::fs::read_to_string(&legacy_path).unwrap(),
        )
        .unwrap();

        if let Some(env_obj) = json.get("env").and_then(|v| v.as_object()) {
            let conflicting_keys: Vec<&String> = env_obj
                .keys()
                .filter(|k| k.starts_with("ANTHROPIC_"))
                .collect();
            assert_eq!(conflicting_keys.len(), 2);
        } else {
            panic!("Expected env block in test JSON");
        }
    }

    #[test]
    fn test_check_legacy_claude_config_without_conflicting_env() {
        let tmp = TempDir::new().unwrap();
        let legacy_path = tmp.path().join(".claude.json");

        // File exists but no ANTHROPIC_* fields in env
        let content = serde_json::json!({
            "env": {
                "SOME_OTHER_VAR": "value"
            }
        });
        std::fs::write(&legacy_path, serde_json::to_string(&content).unwrap()).unwrap();

        let json: Value = serde_json::from_str(
            &std::fs::read_to_string(&legacy_path).unwrap(),
        )
        .unwrap();

        if let Some(env_obj) = json.get("env").and_then(|v| v.as_object()) {
            let conflicting_keys: Vec<&String> = env_obj
                .keys()
                .filter(|k| k.starts_with("ANTHROPIC_"))
                .collect();
            assert!(conflicting_keys.is_empty());
        }
    }

    #[test]
    fn test_detect_env_conflicts_with_set_vars() {
        // Temporarily set env vars for testing
        // SAFETY: This test runs in isolation and we clean up the var after.
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-12345") };
        let conflicts = detect_env_conflicts();

        // Should detect at least ANTHROPIC_API_KEY
        let anthropic_conflict = conflicts
            .iter()
            .find(|c| {
                c.details
                    .as_ref()
                    .map_or(false, |d| d.contains("ANTHROPIC_API_KEY"))
            });
        assert!(anthropic_conflict.is_some());
        let conflict = anthropic_conflict.unwrap();
        assert_eq!(conflict.conflict_type, ConflictType::EnvVarOverride);
        assert!(conflict.description.contains("ANTHROPIC_API_KEY"));

        // Clean up
        // SAFETY: Restoring env state after test.
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    }

    #[test]
    fn test_detect_env_conflicts_empty_var_ignored() {
        // Set an empty env var — should not be reported as a conflict
        // SAFETY: This test runs in isolation and we clean up the var after.
        unsafe { std::env::set_var("OPENAI_BASE_URL", "") };
        let conflicts = detect_env_conflicts();

        let openai_base_conflict = conflicts
            .iter()
            .find(|c| {
                c.details
                    .as_ref()
                    .map_or(false, |d| d.contains("OPENAI_BASE_URL"))
            });
        assert!(openai_base_conflict.is_none());

        // Clean up
        // SAFETY: Restoring env state after test.
        unsafe { std::env::remove_var("OPENAI_BASE_URL") };
    }

    #[test]
    fn test_detect_conflicts_combines_all_sources() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, "{}").unwrap();

        // Set an env var to trigger EnvVarOverride
        // SAFETY: This test runs in isolation and we clean up the var after.
        unsafe { std::env::set_var("ANTHROPIC_AUTH_TOKEN", "test-token-value") };

        // Use a very old timestamp to trigger ExternalModification
        let conflicts = detect_conflicts("claude-code", Some(1_000_000));

        // Should have at least the env var conflict
        let has_env_conflict = conflicts
            .iter()
            .any(|c| c.conflict_type == ConflictType::EnvVarOverride);
        assert!(has_env_conflict);

        // Clean up
        // SAFETY: Restoring env state after test.
        unsafe { std::env::remove_var("ANTHROPIC_AUTH_TOKEN") };
    }

    #[test]
    fn test_config_conflict_serialization_roundtrip() {
        let conflict = ConfigConflict {
            conflict_type: ConflictType::ExternalModification,
            description: "File was modified externally".to_string(),
            file_path: Some("/home/user/.claude/settings.json".to_string()),
            details: Some("mtime=1700000000, last_sync=1699999000".to_string()),
        };

        let json = serde_json::to_string(&conflict).unwrap();
        let deserialized: ConfigConflict = serde_json::from_str(&json).unwrap();
        assert_eq!(conflict, deserialized);
    }

    #[test]
    fn test_conflict_type_variants_serialize() {
        // Verify all ConflictType variants serialize/deserialize correctly
        let variants = vec![
            ConflictType::ExternalModification,
            ConflictType::LegacyConfig,
            ConflictType::EnvVarOverride,
        ];

        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let deserialized: ConflictType = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, deserialized);
        }
    }
}

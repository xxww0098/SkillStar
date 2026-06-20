//! Rolling backups, JSON/TOML merge writers, and active-tool re-sync.

use super::*;

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
pub(crate) fn cleanup_old_backups(path: &Path, keep: usize) -> Result<()> {
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
            if let Some(suffix) = entry_name_str.strip_prefix(&prefix)
                && let Ok(ts) = suffix.parse::<u128>()
            {
                backups.push((ts, entry.path()));
            }
        }
    }

    // Sort by timestamp descending (newest first)
    backups.sort_by_key(|b| std::cmp::Reverse(b.0));

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
    let output =
        serde_json::to_string_pretty(&Value::Object(json)).context("Failed to serialize JSON")?;
    std::fs::write(path, output).with_context(|| format!("Failed to write {}", path.display()))?;

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
        // Update managed fields in the env block. A `Null` value means
        // "remove this key" — used to clear optional fields (e.g. the Claude
        // tier-model overrides) when the user leaves them blank.
        for (key, value) in managed_fields {
            if value.is_null() {
                env_map.remove(*key);
            } else {
                env_map.insert(key.to_string(), value.clone());
            }
        }
    } else {
        // env exists but is not an object — replace it
        let mut new_env = serde_json::Map::new();
        for (key, value) in managed_fields {
            if !value.is_null() {
                new_env.insert(key.to_string(), value.clone());
            }
        }
        json.insert("env".to_string(), Value::Object(new_env));
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Write back as pretty JSON
    let output =
        serde_json::to_string_pretty(&Value::Object(json)).context("Failed to serialize JSON")?;
    std::fs::write(path, output).with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

/// Write Codex config.toml with flat store format.
///
/// Sets:
/// - `model_provider = "skillstar"`
/// - `model = "<activation.model>"`
/// - `[model_providers.skillstar]` table with name, base_url, wire_api, requires_openai_auth
///
/// `settings` controls:
/// - `wire_api`: `"responses"` (default) or `"chat"`
/// - `auth_mode`: `"api_key"` (default) or `"oauth"`
///
/// Preserves all other existing sections/fields.
pub fn write_codex_config_flat(
    path: &Path,
    provider: &ProviderEntryFlat,
    activation: &ToolActivation,
    settings: &CodexSettings,
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
        toml::Value::String(activation.model.clone()),
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
        toml::Value::String(settings.wire_api.clone()),
    );
    skillstar_section.insert(
        "requires_openai_auth".to_string(),
        toml::Value::Boolean(settings.auth_mode == "api_key"),
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
        table.insert("model_providers".to_string(), toml::Value::Table(mp_table));
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Write back as TOML
    let output = toml::to_string_pretty(&table).context("Failed to serialize Codex config.toml")?;
    std::fs::write(path, output).with_context(|| format!("Failed to write {}", path.display()))?;

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
            "claude-code" => match sync_to_claude_code(provider, &activation.model) {
                Ok(r) => r,
                Err(e) => ToolSyncResultFlat {
                    tool_id: tool_id.clone(),
                    success: false,
                    config_path: None,
                    error: Some(e.to_string()),
                    backup_path: None,
                },
            },
            "codex" => match sync_to_codex(provider, activation) {
                Ok(r) => r,
                Err(e) => ToolSyncResultFlat {
                    tool_id: tool_id.clone(),
                    success: false,
                    config_path: None,
                    error: Some(e.to_string()),
                    backup_path: None,
                },
            },
            "opencode" => match sync_to_opencode(provider, &activation.model) {
                Ok(r) => r,
                Err(e) => ToolSyncResultFlat {
                    tool_id: tool_id.clone(),
                    success: false,
                    config_path: None,
                    error: Some(e.to_string()),
                    backup_path: None,
                },
            },
            "zcode" => match sync_to_zcode(provider, &activation.model) {
                Ok(r) => r,
                Err(e) => ToolSyncResultFlat {
                    tool_id: tool_id.clone(),
                    success: false,
                    config_path: None,
                    error: Some(e.to_string()),
                    backup_path: None,
                },
            },
            "gemini" => match sync_to_gemini(provider, &activation.model) {
                Ok(r) => r,
                Err(e) => ToolSyncResultFlat {
                    tool_id: tool_id.clone(),
                    success: false,
                    config_path: None,
                    error: Some(e.to_string()),
                    backup_path: None,
                },
            },
            _ => ToolSyncResultFlat {
                tool_id: tool_id.clone(),
                success: false,
                config_path: None,
                error: Some(format!(
                    "Unknown tool_id '{}'. Supported: claude-code, codex, opencode, gemini, zcode.",
                    tool_id
                )),
                backup_path: None,
            },
        };

        results.push(result);
    }

    // 4. Return all collected results
    results
}

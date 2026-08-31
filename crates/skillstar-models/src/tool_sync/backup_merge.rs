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
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
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

/// Read an existing config file for a merge write, failing closed on garbage.
///
/// Missing or blank files yield `None` (callers start fresh); a file that
/// exists but does not parse must be a hard error — answering it with a fresh
/// root would make the next write replace the user's config with a
/// managed-only skeleton, the exact v3 defect `store_v4` exists to end.
pub(crate) fn read_existing_config(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(content))
}

/// Merge write: read existing JSON, update managed fields at top level, write back.
///
/// If the file doesn't exist, creates a new JSON object with just the managed fields.
/// Preserves all existing fields that are not in the managed_fields list.
pub fn merge_json_write(path: &Path, managed_fields: &[(&str, Value)]) -> Result<()> {
    let mut json: serde_json::Map<String, Value> = match read_existing_config(path)? {
        Some(content) => match serde_json::from_str::<Value>(&content).with_context(|| {
            format!(
                "Failed to parse {} — fix or remove it before syncing",
                path.display()
            )
        })? {
            Value::Object(map) => map,
            _ => bail!(
                "{} root must be a JSON object — fix or remove it before syncing",
                path.display()
            ),
        },
        None => serde_json::Map::new(),
    };

    // Update managed fields
    for (key, value) in managed_fields {
        json.insert(key.to_string(), value.clone());
    }

    // Write back as pretty JSON (atomic: a crash mid-write must not truncate
    // the user's config file)
    let output =
        serde_json::to_string_pretty(&Value::Object(json)).context("Failed to serialize JSON")?;
    skillstar_core::infra::fs_ops::atomic_write(path, output.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

/// Merge write for Claude Code's env block specifically.
///
/// Reads existing `~/.claude/settings.json`, updates only the `env` sub-object
/// with the managed fields, preserving all other top-level fields and non-managed
/// env fields.
pub fn merge_json_env_write(path: &Path, managed_fields: &[(&str, Value)]) -> Result<()> {
    let mut json: serde_json::Map<String, Value> = match read_existing_config(path)? {
        Some(content) => match serde_json::from_str::<Value>(&content).with_context(|| {
            format!(
                "Failed to parse {} — fix or remove it before syncing",
                path.display()
            )
        })? {
            Value::Object(map) => map,
            _ => bail!(
                "{} root must be a JSON object — fix or remove it before syncing",
                path.display()
            ),
        },
        None => serde_json::Map::new(),
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

    // Write back as pretty JSON (atomic: a crash mid-write must not truncate
    // the user's config file)
    let output =
        serde_json::to_string_pretty(&Value::Object(json)).context("Failed to serialize JSON")?;
    skillstar_core::infra::fs_ops::atomic_write(path, output.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Re-sync active tools after provider settings update
// ---------------------------------------------------------------------------

/// After a provider's settings are saved, re-sync all tools whose binding
/// references this provider. Each tool's individually selected model is
/// preserved.
///
/// Returns a list of sync results (one per affected tool).
///
/// # Per-tool error isolation
/// One tool failing does not prevent syncing others. Each tool is synced
/// independently and its result is collected regardless of success/failure.
///
/// # Registry-driven filter, single dispatch
/// This function only decides *which* tools are affected; the actual write is
/// delegated to [`sync_tool_binding`] so there is exactly one sync dispatch.
/// [`AgentKind`] drives the affectedness rule: multi-provider agents rewrite
/// their whole binding when *any* entry references the provider (every managed
/// table must stay consistent), single-provider agents only when the *active*
/// entry does. Retired / unknown tool ids (e.g. removed `gemini`) are skipped.
pub fn resync_active_tools(store: &ProvidersStoreV4, provider_id: &str) -> Vec<ToolSyncResultFlat> {
    if !store.providers.iter().any(|p| p.id == provider_id) {
        // Provider not found — return a single error result
        return vec![ToolSyncResultFlat {
            tool_id: String::new(),
            success: false,
            config_path: None,
            error: Some(format!("Provider '{}' not found in store", provider_id)),
            backup_path: None,
            dropped_roles: Vec::new(),
        }];
    }

    let mut results: Vec<ToolSyncResultFlat> = Vec::new();

    for (tool_id, binding) in &store.bindings {
        // Skip tools that don't reference this provider at all.
        if !binding.entries.iter().any(|e| e.provider_id == provider_id) {
            continue;
        }
        // Skip retired / unknown tool ids left behind by older SkillStar versions
        // (e.g. removed `gemini`) so provider saves do not fail on leftovers.
        let Some(spec) = agent_spec(tool_id) else {
            continue;
        };
        // Single-provider agents only resync when the active entry matches;
        // multi-provider agents always rewrite their whole binding.
        let affected = match spec.kind {
            AgentKind::Single => binding
                .active()
                .is_some_and(|a| a.provider_id == provider_id),
            AgentKind::Multi => true,
        };
        if !affected {
            continue;
        }

        results.push(sync_tool_binding(store, tool_id));
    }

    results
}

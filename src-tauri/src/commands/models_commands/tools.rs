//! Agent binding, config-file, installation and conflict-detection commands.

use super::compat;
use super::*;

// ---------------------------------------------------------------------------
// Tool config commands
// ---------------------------------------------------------------------------

/// Returns the supported external tool config targets with paths and existence.
#[tauri::command]
pub async fn get_tool_config_targets() -> Result<Vec<ToolConfigTarget>, AppError> {
    Ok(tool_sync::get_tool_config_targets()?)
}

/// The agent registry itself: identity, binding kind, wire protocol, config
/// files, and each agent's role list.
///
/// This is what lets the role panel be one component instead of one per agent.
/// Which roles Claude Code has, what OMP calls `fast` on disk, and which rows
/// fall back to `default` are all facts the backend already had to know in order
/// to write the files; serving them is what stops the frontend from keeping a
/// second, drifting copy.
#[tauri::command]
pub async fn list_agent_descriptors() -> Result<Vec<AgentDescriptorDto>, AppError> {
    Ok(skillstar_app::models::agents::agent_descriptors())
}

/// Bind a provider to an agent and make it the active one.
///
/// Replaces v3's `activate_tool`, which also did the jobs now held by
/// [`set_active_binding`] and [`update_binding_entry`]. Fails when the provider
/// has no endpoint for the protocol the agent speaks — for Codex that means
/// `/v1/responses`, and that refusal is the point: binding a chat-only host to
/// Codex is what produced `config.toml` files Codex could not parse.
#[tauri::command]
pub async fn bind_provider(
    lock: State<'_, ProvidersWriteLock>,
    provider_id: String,
    tool_id: String,
    model: Option<String>,
    settings: Option<serde_json::Value>,
) -> Result<ToolSyncResultFlat, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    providers::bind_provider(
        &mut store,
        &tool_id,
        &provider_id,
        model.as_deref(),
        settings,
    )?;
    providers::write_store_v4(&store, &path)?;

    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);
    stamp_sync(&mut store, &tool_id, &sync_result, &path)?;
    Ok(sync_result)
}

/// Move an agent's active pointer to an already-bound provider.
#[tauri::command]
pub async fn set_active_binding(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
    provider_id: String,
) -> Result<ToolSyncResultFlat, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    providers::set_active_binding(&mut store, &tool_id, &provider_id)?;
    providers::write_store_v4(&store, &path)?;

    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);
    stamp_sync(&mut store, &tool_id, &sync_result, &path)?;
    Ok(sync_result)
}

/// Change a bound entry's model and/or settings without moving the pointer.
#[tauri::command]
pub async fn update_binding_entry(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
    provider_id: String,
    model: Option<String>,
    settings: Option<serde_json::Value>,
) -> Result<ToolSyncResultFlat, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    providers::update_binding_entry(
        &mut store,
        &tool_id,
        &provider_id,
        model.as_deref(),
        settings,
    )?;
    providers::write_store_v4(&store, &path)?;

    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);
    stamp_sync(&mut store, &tool_id, &sync_result, &path)?;
    Ok(sync_result)
}

/// Unbind **one** provider from an agent.
///
/// v3 wired every per-row unbind button to `deactivate_tool`, which cleared the
/// agent's whole list — so unbinding one of three providers unbound all three.
/// This removes exactly the row asked for; [`unbind_agent`] is the one that
/// clears everything, and it now has to be named.
#[tauri::command]
pub async fn unbind_provider(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
    provider_id: String,
) -> Result<ToolSyncResultFlat, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    providers::unbind_provider(&mut store, &tool_id, &provider_id)?;
    providers::write_store_v4(&store, &path)?;

    // sync_tool_binding unsyncs automatically when the binding is now empty.
    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);
    stamp_sync(&mut store, &tool_id, &sync_result, &path)?;
    Ok(sync_result)
}

/// Clear an agent's binding entirely and restore its config file.
#[tauri::command]
pub async fn unbind_agent(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
) -> Result<(), AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    providers::unbind_agent(&mut store, &tool_id)?;
    providers::write_store_v4(&store, &path)?;
    tool_sync::unsync_tool(&tool_id)?;

    Ok(())
}

/// Replace the active entry's per-provider settings bag.
#[tauri::command]
pub async fn update_binding_entry_settings(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
    settings: serde_json::Value,
) -> Result<ToolSyncResultFlat, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    providers::update_binding_entry_settings(&mut store, &tool_id, settings)?;
    providers::write_store_v4(&store, &path)?;

    Ok(tool_sync::sync_tool_binding(&store, &tool_id))
}

/// Replace an agent's own settings — including its role → model routing.
///
/// The renderer still sends roles inside the settings bag under the key v3
/// used; they are split back out into the typed `roles` field here. The bag and
/// the roles are two fields in v4 and one on the wire until WP-4.
#[tauri::command]
pub async fn update_agent_settings(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
    settings: serde_json::Value,
) -> Result<ToolSyncResultFlat, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    let roles = compat::roles_from_flat(&settings);
    let rest = compat::settings_without_roles(&settings);
    providers::update_agent_settings(
        &mut store,
        &tool_id,
        rest.unwrap_or(serde_json::Value::Null),
    )?;
    providers::set_agent_roles(&mut store, &tool_id, roles)?;
    providers::write_store_v4(&store, &path)?;

    Ok(tool_sync::sync_tool_binding(&store, &tool_id))
}

/// Stamp the active entry's last-sync time after a successful disk write, and
/// persist. The stamp is the baseline external-modification detection compares
/// against, so it is only meaningful when the write actually happened.
fn stamp_sync(
    store: &mut providers::ProvidersStoreV4,
    tool_id: &str,
    result: &ToolSyncResultFlat,
    path: &std::path::Path,
) -> Result<(), AppError> {
    if !result.success {
        return Ok(());
    }
    if let Some(entry) = store.bindings.get_mut(tool_id).and_then(|b| b.active_mut()) {
        entry.last_sync_at_ms = Some(now_unix_ms());
    }
    providers::write_store_v4(store, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tool Installation Detection
// ---------------------------------------------------------------------------

/// Detect whether an Agent tool is installed on the system.
///
/// Checks:
/// 1. Whether the CLI binary exists on the **enriched** PATH (Homebrew, agent
///    self-install bins like `~/.opencode/bin` / `~/.grok/bin`, npm global, …)
/// 2. Whether the tool's config directory exists (e.g., `~/.claude` for claude-code)
///
/// Returns a JSON object: `{ "installed": bool, "binary_found": bool, "config_dir_found": bool }`
///
/// A tool is considered "installed" if the binary is found on the enriched PATH;
/// Claude Code also accepts the shared Desktop app surface. The config_dir_found
/// field provides additional context (a tool may be installed but not yet
/// configured, or config may exist from a previous installation).
///
/// This probe belongs only to the explicit Models tool-setup workflow. It must
/// never feed Agent profile activation or card/MCP rail visibility; those are
/// controlled solely by the user's Settings switch.
#[tauri::command]
pub async fn detect_tool_installation(tool_id: String) -> Result<serde_json::Value, AppError> {
    let spec = tool_sync::agent_spec(&tool_id).ok_or_else(|| {
        AppError::Other(format!(
            "Unknown tool_id: '{}'. Supported: claude-code, claude-desktop, codex, opencode, pi.",
            tool_id
        ))
    })?;

    // Claude Desktop is a GUI app binding — never treat the `claude` CLI as
    // evidence that Desktop itself is installed (Windows + macOS).
    let binary_found = if tool_id == "claude-desktop" {
        false
    } else {
        skillstar_core::infra::path_env::which_in_enriched(spec.binary_name).is_some()
    };

    let config_dir_found = dirs::home_dir()
        .map(|home| {
            spec.config_dir_probes.iter().any(|probe| {
                probe
                    .split('/')
                    .fold(home.clone(), |path, seg| path.join(seg))
                    .is_dir()
            })
        })
        .unwrap_or(false);

    let desktop_app_found = matches!(tool_id.as_str(), "claude-code" | "claude-desktop")
        && skillstar_core::infra::path_env::desktop_app_installed("Claude");
    let installed = agent_tool_installed_from_signals(
        &tool_id,
        binary_found,
        desktop_app_found,
        config_dir_found,
    );

    Ok(serde_json::json!({
        "installed": installed,
        "binary_found": binary_found,
        "config_dir_found": config_dir_found
    }))
}

fn agent_tool_installed_from_signals(
    tool_id: &str,
    binary_found: bool,
    desktop_app_found: bool,
    _config_dir_found: bool,
) -> bool {
    match tool_id {
        "claude-code" => binary_found || desktop_app_found,
        // Desktop = Anthropic Claude.app only. Never treat SkillStar's own
        // `~/.claude-desktop` marker (created on sync) as install evidence.
        "claude-desktop" => desktop_app_found,
        _ => binary_found,
    }
}

// ---------------------------------------------------------------------------
// Tool config file read/write (JSON / TOML editor)
// ---------------------------------------------------------------------------

/// List on-disk config files for a tool (Claude / Codex / OpenCode).
#[tauri::command]
pub async fn list_tool_config_files(
    tool_id: String,
) -> Result<Vec<tool_sync::ToolConfigFileInfo>, AppError> {
    Ok(tool_sync::list_tool_config_files(&tool_id)?)
}

/// Read raw config file contents.
#[tauri::command]
pub async fn read_tool_config_file(tool_id: String, file_id: String) -> Result<String, AppError> {
    Ok(tool_sync::read_tool_config_file(&tool_id, &file_id)?)
}

/// Validate and save config file contents (with rolling backup).
#[tauri::command]
pub async fn write_tool_config_file(
    tool_id: String,
    file_id: String,
    content: String,
) -> Result<tool_sync::WriteToolConfigFileResult, AppError> {
    Ok(tool_sync::write_tool_config_file(
        &tool_id, &file_id, &content,
    ))
}

/// Pretty-format JSON/TOML without writing to disk.
#[tauri::command]
pub async fn format_tool_config_file(tool_id: String, file_id: String) -> Result<String, AppError> {
    Ok(tool_sync::format_tool_config_file(&tool_id, &file_id)?)
}

/// Push a bound provider's credentials to a tool's config files.
#[tauri::command]
pub async fn push_provider_to_tool_config(
    lock: State<'_, ProvidersWriteLock>,
    provider_id: String,
    tool_id: String,
) -> Result<ToolSyncResultFlat, AppError> {
    let _guard = lock.0.lock().await;
    let store = load_store()?;

    // The provider must be bound to this tool (in any entry).
    let bound = store
        .bindings
        .get(&tool_id)
        .is_some_and(|b| b.binds_provider(&provider_id));
    if !bound {
        return Err(AppError::Other(format!(
            "Tool '{}' is not activated for provider '{}'",
            tool_id, provider_id
        )));
    }

    let result = tool_sync::sync_tool_binding(&store, &tool_id);
    Ok(result)
}

// ---------------------------------------------------------------------------
// Environment Conflict Detection
// ---------------------------------------------------------------------------

/// Current Unix time in **milliseconds** (best-effort; 0 before the epoch).
///
/// v3 stored this one in seconds while storing `created_at` in milliseconds, in
/// the same file. v4 puts the unit in the field name and this is where the
/// value that fills it comes from.
fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Detect config conflicts relevant to a provider: for each tool this provider
/// is active on, check external modification (vs that tool's `last_sync_at`) and
/// legacy `~/.claude.json`; plus global shell env overrides (added once).
#[tauri::command]
pub async fn detect_provider_conflicts(
    provider_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    let store = load_store()?;

    let mut conflicts: Vec<tool_sync::ConfigConflict> = Vec::new();
    for (tool_id, binding) in &store.bindings {
        if let Some(entry) = binding
            .entries
            .iter()
            .find(|e| e.provider_id == provider_id)
        {
            // detect_conflicts compares against a file mtime, which it reads in
            // seconds; the store now holds milliseconds.
            let last_sync_at = entry.last_sync_at_ms.map(|ms| ms / 1000);
            for c in tool_sync::detect_conflicts(tool_id, last_sync_at) {
                // Env overrides are global — added once below to avoid dupes.
                if !matches!(c.conflict_type, tool_sync::ConflictType::EnvVarOverride) {
                    conflicts.push(c);
                }
            }
        }
    }
    conflicts.extend(tool_sync::detect_env_conflicts());

    let serialized: Vec<serde_json::Value> = conflicts
        .into_iter()
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();
    Ok(serialized)
}

/// Re-sync a tool's current activation to disk, overwriting any external edits,
/// and refresh its `last_sync_at`. Backs the "overwrite" conflict action.
#[tauri::command]
pub async fn resync_tool(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
) -> Result<ToolSyncResultFlat, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    let is_bound = store.bindings.get(&tool_id).is_some_and(|b| !b.is_empty());
    if !is_bound {
        return Err(AppError::Other(format!("Tool '{tool_id}' is not bound")));
    }

    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);
    stamp_sync(&mut store, &tool_id, &sync_result, &path)?;
    Ok(sync_result)
}

#[cfg(test)]
mod tests {
    use super::agent_tool_installed_from_signals;

    #[test]
    fn claude_code_installation_accepts_cli_or_desktop_app() {
        assert!(agent_tool_installed_from_signals(
            "claude-code",
            true,
            false,
            false
        ));
        assert!(agent_tool_installed_from_signals(
            "claude-code",
            false,
            true,
            false
        ));
        assert!(!agent_tool_installed_from_signals(
            "claude-code",
            false,
            false,
            false
        ));
        assert!(!agent_tool_installed_from_signals(
            "codex", false, true, false
        ));
    }

    #[test]
    fn claude_desktop_requires_desktop_app_not_cli_or_marker() {
        // CLI-on-PATH must not unlock Desktop bind/resync flows.
        assert!(!agent_tool_installed_from_signals(
            "claude-desktop",
            true,
            false,
            false
        ));
        // SkillStar's own binding dir is not install evidence.
        assert!(!agent_tool_installed_from_signals(
            "claude-desktop",
            false,
            false,
            true
        ));
        assert!(agent_tool_installed_from_signals(
            "claude-desktop",
            false,
            true,
            false
        ));
    }
}

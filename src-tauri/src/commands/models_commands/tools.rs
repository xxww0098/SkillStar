//! Tool activation, config-file, installation and conflict-detection commands.
//!
//! Carved out of `models_commands` mechanically — no logic changes.

use super::*;

// ---------------------------------------------------------------------------
// Tool config commands
// ---------------------------------------------------------------------------

/// Returns the list of supported external tool config targets with their paths and existence status.
#[tauri::command]
pub async fn get_tool_config_targets() -> Result<Vec<ToolConfigTarget>, String> {
    tool_sync::get_tool_config_targets().map_err(|e| e.to_string())
}

/// Sync a provider's configuration to a single external tool.
///
/// Creates a backup of the existing config file before writing.
#[tauri::command]
pub async fn sync_provider_to_tool(
    app_id: String,
    provider_id: String,
    tool_id: String,
) -> Result<ToolSyncResult, String> {
    let store = providers::read_store().map_err(|e| e.to_string())?;

    let provider = match app_id.as_str() {
        "claude" => store.claude.providers.get(&provider_id),
        "codex" => store.codex.providers.get(&provider_id),
        "opencode" => store.opencode.providers.get(&provider_id),
        "gemini" => store.gemini.providers.get(&provider_id),
        _ => return Err(format!("Unknown app_id: {}", app_id)),
    }
    .ok_or_else(|| format!("Provider '{}' not found in app '{}'", provider_id, app_id))?;

    Ok(tool_sync::sync_provider_to_tool(provider, &tool_id))
}

/// Sync a provider's configuration to all supported external tools.
///
/// Syncs to each tool independently — a failure in one tool does not prevent others.
#[tauri::command]
pub async fn sync_provider_to_all_tools(
    app_id: String,
    provider_id: String,
    tool_ids: Vec<String>,
) -> Result<Vec<ToolSyncResult>, String> {
    let store = providers::read_store().map_err(|e| e.to_string())?;

    let provider = match app_id.as_str() {
        "claude" => store.claude.providers.get(&provider_id),
        "codex" => store.codex.providers.get(&provider_id),
        "opencode" => store.opencode.providers.get(&provider_id),
        "gemini" => store.gemini.providers.get(&provider_id),
        _ => return Err(format!("Unknown app_id: {}", app_id)),
    }
    .ok_or_else(|| format!("Provider '{}' not found in app '{}'", provider_id, app_id))?;

    Ok(tool_sync::sync_provider_to_all_tools(provider, &tool_ids))
}

/// Activate a provider for a specific Agent tool.
///
/// Updates the `tool_activations` map and syncs the provider's credentials
/// to the tool's config file. Only one provider can be active per tool —
/// activating a new provider replaces any previous activation.
///
/// If `model` is None, the provider's `default_model` is used.
///
/// `settings` is an optional per-tool config object (e.g. `{ "wire_api": "chat", "auth_mode": "oauth" }` for Codex).
/// When omitted, the previous activation's settings are preserved if re-activating the same tool.
#[tauri::command]
pub async fn activate_tool(
    lock: State<'_, ProvidersWriteLock>,
    provider_id: String,
    tool_id: String,
    model: Option<String>,
    settings: Option<serde_json::Value>,
) -> Result<ToolSyncResultFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    // 1. Update the tool_activations map (binding upsert + active pointer)
    providers::activate_tool(
        &mut store,
        &provider_id,
        &tool_id,
        model.as_deref(),
        settings,
    )
    .map_err(|e| e.to_string())?;

    // 2. Persist the updated store
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    // 3. Sync to disk. Multi-provider agents (codex, opencode) project their
    //    whole binding; single-provider agents write the active entry. Routing
    //    lives in `sync_tool_binding` so the command layer stays kind-agnostic.
    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);

    // On a successful disk write, stamp last_sync_at (baseline for
    // external-modification detection) and persist.
    if sync_result.success {
        if let Some(act) = store
            .tool_activations
            .get_mut(&tool_id)
            .and_then(|b| b.active_mut())
        {
            act.last_sync_at = Some(now_unix_secs());
        }
        providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;
    }

    Ok(sync_result)
}

/// Deactivate a tool by removing its activation entry and restoring its config.
///
/// Clears the tool's entry in `tool_activations` and calls the appropriate
/// unsync function to remove managed fields from the tool's config file.
#[tauri::command]
pub async fn deactivate_tool(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
) -> Result<(), String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    // 1. Remove the activation from the map
    providers::deactivate_tool(&mut store, &tool_id).map_err(|e| e.to_string())?;

    // 2. Persist the updated store
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    // 3. Unsync the tool's config file (remove ALL managed fields/entries)
    match tool_id.as_str() {
        "claude-code" => {
            tool_sync::unsync_claude_code().map_err(|e| e.to_string())?;
        }
        "codex" => {
            tool_sync::unsync_codex_all().map_err(|e| e.to_string())?;
        }
        "opencode" => {
            tool_sync::unsync_opencode_all().map_err(|e| e.to_string())?;
        }
        "gemini" => {
            tool_sync::unsync_gemini().map_err(|e| e.to_string())?;
        }
        _ => {}
    }

    Ok(())
}

/// Switch which bound provider is active for a multi-provider tool.
///
/// Moves the binding's active pointer to `provider_id` (which must already be
/// bound) and rewrites the tool's config so the active selector follows.
#[tauri::command]
pub async fn set_active_binding(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
    provider_id: String,
) -> Result<ToolSyncResultFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    providers::set_active_binding(&mut store, &tool_id, &provider_id)
        .map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);
    if sync_result.success {
        if let Some(act) = store
            .tool_activations
            .get_mut(&tool_id)
            .and_then(|b| b.active_mut())
        {
            act.last_sync_at = Some(now_unix_secs());
        }
        providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;
    }
    Ok(sync_result)
}

/// Remove a single provider entry from a multi-provider tool's binding.
///
/// Drops the entry and rewrites the tool's config (the provider's managed
/// table is removed; the active pointer re-clamps to a remaining entry). If the
/// last entry is removed, the tool is fully unsynced.
#[tauri::command]
pub async fn remove_binding_entry(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
    provider_id: String,
) -> Result<ToolSyncResultFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    providers::remove_binding_entry(&mut store, &tool_id, &provider_id)
        .map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    // sync_tool_binding unsyncs automatically when the binding is now empty.
    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);
    if sync_result.success {
        if let Some(act) = store
            .tool_activations
            .get_mut(&tool_id)
            .and_then(|b| b.active_mut())
        {
            act.last_sync_at = Some(now_unix_secs());
        }
        providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;
    }
    Ok(sync_result)
}

/// Update only the settings of an active tool without changing provider or model.
///
/// Useful for toggling per-tool options (e.g. Codex's `wire_api` or `auth_mode`)
/// without a full re-activation. Automatically re-syncs the tool's config file.
#[tauri::command]
pub async fn update_tool_settings(
    lock: State<'_, ProvidersWriteLock>,
    tool_id: String,
    settings: serde_json::Value,
) -> Result<ToolSyncResultFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    // 1. Update settings on the active entry of the binding
    providers::update_tool_settings(&mut store, &tool_id, settings)
        .map_err(|e| e.to_string())?;

    // 2. Persist the updated store
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    // 3. Re-sync the tool's config file (whole binding for multi-provider tools)
    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);

    Ok(sync_result)
}

// ---------------------------------------------------------------------------
// Tool Installation Detection
// ---------------------------------------------------------------------------

/// Detect whether an Agent tool (CLI) is installed on the system.
///
/// Checks:
/// 1. Whether the CLI binary exists in PATH (e.g., `claude` for claude-code, `codex` for codex)
/// 2. Whether the tool's config directory exists (e.g., `~/.claude` for claude-code, `~/.codex` for codex)
///
/// Returns a JSON object: `{ "installed": bool, "binary_found": bool, "config_dir_found": bool }`
///
/// A tool is considered "installed" if the binary is found in PATH.
/// The config_dir_found field provides additional context (a tool may be installed
/// but not yet configured, or config may exist from a previous installation).
#[tauri::command]
pub async fn detect_tool_installation(tool_id: String) -> Result<serde_json::Value, String> {
    // Claude Desktop is a GUI app, not a CLI — detect by app bundle / install path instead
    // of by binary on PATH.
    if tool_id == "claude-desktop" {
        let binary_found = detect_claude_desktop_app();
        let config_dir_found = dirs::config_dir()
            .map(|base| base.join("Claude").is_dir())
            .unwrap_or(false);
        return Ok(serde_json::json!({
            "installed": binary_found,
            "binary_found": binary_found,
            "config_dir_found": config_dir_found,
        }));
    }

    let binary_name = match tool_id.as_str() {
        "claude-code" => "claude",
        "codex" => "codex",
        "opencode" => "opencode",
        "gemini" => "gemini",
        _ => {
            return Err(format!(
                "Unknown tool_id: '{}'. Supported: claude-code, codex, opencode, claude-desktop, gemini.",
                tool_id
            ));
        }
    };

    let binary_found = which::which(binary_name).is_ok();

    let config_dir_found = dirs::home_dir()
        .map(|home| match tool_id.as_str() {
            "claude-code" => home.join(".claude").is_dir(),
            "codex" => home.join(".codex").is_dir(),
            "opencode" => home.join(".config").join("opencode").is_dir(),
            "gemini" => home.join(".gemini").is_dir(),
            _ => false,
        })
        .unwrap_or(false);

    // A tool is considered installed if the binary is found in PATH
    let installed = binary_found;

    Ok(serde_json::json!({
        "installed": installed,
        "binary_found": binary_found,
        "config_dir_found": config_dir_found
    }))
}

/// Detect Claude Desktop App installation by scanning common install paths per OS.
///
/// - macOS: `/Applications/Claude.app` or `~/Applications/Claude.app`
/// - Windows: `%LOCALAPPDATA%\Programs\Claude\Claude.exe` (per-user) or
///   `%ProgramFiles%\Claude\Claude.exe` (machine-wide)
/// - Linux: no official Linux Claude Desktop — returns false.
fn detect_claude_desktop_app() -> bool {
    #[cfg(target_os = "macos")]
    {
        if std::path::Path::new("/Applications/Claude.app").exists() {
            return true;
        }
        if let Some(home) = dirs::home_dir()
            && home.join("Applications").join("Claude.app").exists()
        {
            return true;
        }
        false
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(local) = dirs::data_local_dir() {
            if local
                .join("Programs")
                .join("Claude")
                .join("Claude.exe")
                .exists()
            {
                return true;
            }
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            if std::path::Path::new(&pf)
                .join("Claude")
                .join("Claude.exe")
                .exists()
            {
                return true;
            }
        }
        false
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Tool config file read/write (JSON / TOML editor)
// ---------------------------------------------------------------------------

/// List on-disk config files for a tool (Claude / Codex / OpenCode).
#[tauri::command]
pub async fn list_tool_config_files(
    tool_id: String,
) -> Result<Vec<tool_sync::ToolConfigFileInfo>, String> {
    tool_sync::list_tool_config_files(&tool_id).map_err(|e| e.to_string())
}

/// Read raw config file contents.
#[tauri::command]
pub async fn read_tool_config_file(tool_id: String, file_id: String) -> Result<String, String> {
    tool_sync::read_tool_config_file(&tool_id, &file_id).map_err(|e| e.to_string())
}

/// Validate and save config file contents (with rolling backup).
#[tauri::command]
pub async fn write_tool_config_file(
    tool_id: String,
    file_id: String,
    content: String,
) -> Result<tool_sync::WriteToolConfigFileResult, String> {
    Ok(tool_sync::write_tool_config_file(
        &tool_id, &file_id, &content,
    ))
}

/// Pretty-format JSON/TOML without writing to disk.
#[tauri::command]
pub async fn format_tool_config_file(tool_id: String, file_id: String) -> Result<String, String> {
    tool_sync::format_tool_config_file(&tool_id, &file_id).map_err(|e| e.to_string())
}

/// Push the active flat-store provider credentials to a tool's config files.
#[tauri::command]
pub async fn push_provider_to_tool_config(
    lock: State<'_, ProvidersWriteLock>,
    provider_id: String,
    tool_id: String,
) -> Result<ToolSyncResultFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    // The provider must be bound to this tool (in any entry).
    let bound = store
        .tool_activations
        .get(&tool_id)
        .is_some_and(|b| b.binds_provider(&provider_id));
    if !bound {
        return Err(format!(
            "Tool '{}' is not activated for provider '{}'",
            tool_id, provider_id
        ));
    }

    let result = tool_sync::sync_tool_binding(&store, &tool_id);
    Ok(result)
}

// ---------------------------------------------------------------------------
// Environment Conflict Detection
// ---------------------------------------------------------------------------

/// Detect shell environment variable conflicts that may override tool config files.
///
/// Delegates to `tool_sync::detect_env_conflicts()` which checks for:
/// - Anthropic/Claude-related env vars (ANTHROPIC_API_KEY, ANTHROPIC_BASE_URL, etc.)
/// - OpenAI/Codex-related env vars (OPENAI_API_KEY, OPENAI_BASE_URL, etc.)
///
/// Returns a list of detected conflicts as serialized JSON values.
#[tauri::command]
pub async fn detect_env_conflicts() -> Result<Vec<serde_json::Value>, String> {
    let conflicts = tool_sync::detect_env_conflicts();
    let serialized: Vec<serde_json::Value> = conflicts
        .into_iter()
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();
    Ok(serialized)
}

/// Current Unix time in seconds (best-effort; 0 if the clock is before epoch).
fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Detect config conflicts relevant to a provider: for each tool this provider
/// is active on, check external modification (vs that tool's `last_sync_at`) and
/// legacy `~/.claude.json`; plus global shell env overrides (added once).
#[tauri::command]
pub async fn detect_provider_conflicts(
    provider_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let path = providers::flat_store_path();
    let store = providers::read_flat_store(&path).map_err(|e| e.to_string())?;

    let mut conflicts: Vec<tool_sync::ConfigConflict> = Vec::new();
    for (tool_id, binding) in &store.tool_activations {
        if let Some(act) = binding.entries.iter().find(|e| e.provider_id == provider_id) {
            for c in tool_sync::detect_conflicts(tool_id, act.last_sync_at) {
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
) -> Result<ToolSyncResultFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    let is_active = store
        .tool_activations
        .get(&tool_id)
        .is_some_and(|b| !b.is_empty());
    if !is_active {
        return Err(format!("Tool '{}' is not active", tool_id));
    }

    let sync_result = tool_sync::sync_tool_binding(&store, &tool_id);

    if sync_result.success {
        if let Some(act) = store
            .tool_activations
            .get_mut(&tool_id)
            .and_then(|b| b.active_mut())
        {
            act.last_sync_at = Some(now_unix_secs());
        }
        providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;
    }

    Ok(sync_result)
}

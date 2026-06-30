//! Provider / preset / ref CRUD commands (legacy per-app + flat store v2).
//!
//! Carved out of `models_commands` mechanically — no logic changes.

use super::*;

// ---------------------------------------------------------------------------
// Read commands (no lock needed)
// ---------------------------------------------------------------------------

/// Returns the full ProvidersStore (all apps).
#[tauri::command]
pub async fn get_providers_store() -> Result<ProvidersStore, String> {
    providers::read_store().map_err(|e| e.to_string())
}

/// Returns providers and current active provider for a single AppId.
#[tauri::command]
pub async fn get_app_providers(app_id: String) -> Result<AppProviders, String> {
    let store = providers::read_store().map_err(|e| e.to_string())?;
    let app = match app_id.as_str() {
        "claude" => store.claude,
        "codex" => store.codex,
        "opencode" => store.opencode,
        "gemini" => store.gemini,
        _ => return Err(format!("Unknown app_id: {}", app_id)),
    };
    Ok(app)
}

/// Returns the list of built-in provider presets.
#[tauri::command]
pub async fn get_provider_presets() -> Result<Vec<ProviderPreset>, String> {
    Ok(providers::get_provider_presets())
}

/// Returns built-in flat provider presets (v2) — single source of truth for the UI.
#[tauri::command]
pub async fn get_provider_presets_flat() -> Result<Vec<ProviderPresetFlat>, String> {
    Ok(providers::get_all_presets_flat())
}

/// Point application AI (`ai.json`) at a flat-store provider.
///
/// `app_id` must be `claude` (Anthropic) or `codex` (OpenAI). Validates that the
/// provider exists and can be resolved before persisting.
#[tauri::command]
pub async fn set_app_ai_provider_ref(app_id: String, provider_id: String) -> Result<(), String> {
    let app_id = app_id.trim();
    let provider_id = provider_id.trim();
    if !matches!(app_id, "claude" | "codex") {
        return Err(format!("Unsupported app_id for app AI: '{app_id}'"));
    }
    if provider_id.is_empty() {
        return Err("provider_id cannot be empty".to_string());
    }

    let path = providers::flat_store_path();
    let store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;
    if !store.providers.iter().any(|p| p.id == provider_id) {
        return Err(format!("Provider '{}' not found", provider_id));
    }

    let mut ai_config = ai_provider::load_config();
    ai_config.enabled = true;
    ai_config.provider_ref = Some(AiProviderRef {
        app_id: app_id.to_string(),
        provider_id: provider_id.to_string(),
    });
    ai_config.api_format = match app_id {
        "claude" => ai_provider::ApiFormat::Anthropic,
        _ => ai_provider::ApiFormat::Openai,
    };

    ai_provider::resolve_provider_ref(&mut ai_config).map_err(|e| e.to_string())?;
    ai_provider::save_config(&ai_config).map_err(|e| e.to_string())?;

    Ok(())
}

/// Clear application AI provider reference (switch back to manual/local config).
#[tauri::command]
pub async fn clear_app_ai_provider_ref() -> Result<(), String> {
    let mut ai_config = ai_provider::load_config();
    ai_config.provider_ref = None;
    ai_provider::save_config(&ai_config).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Write commands (lock required)
// ---------------------------------------------------------------------------

/// Create a new provider entry for the given app_id.
///
/// Validates name, URL, model count, and ID uniqueness.
/// Auto-activates if this is the first provider for the app.
#[tauri::command]
pub async fn create_provider(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    entry: ProviderEntry,
) -> Result<ProviderEntry, String> {
    let _guard = lock.0.lock().await;
    providers::create_provider(&app_id, entry).map_err(|e| e.to_string())
}

/// Create a provider from a built-in preset.
///
/// Only requires the API key; all other fields are pre-filled from the preset.
#[tauri::command]
pub async fn create_provider_from_preset(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    preset_id: String,
    api_key: String,
) -> Result<ProviderEntry, String> {
    let _guard = lock.0.lock().await;
    providers::create_from_preset(&app_id, &preset_id, &api_key).map_err(|e| e.to_string())
}

/// Update an existing provider with a partial patch.
#[tauri::command]
pub async fn update_provider(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    id: String,
    patch: ProviderPatch,
) -> Result<ProviderEntry, String> {
    let _guard = lock.0.lock().await;
    providers::update_provider(&app_id, &id, patch).map_err(|e| e.to_string())
}

/// Delete a provider by ID.
///
/// If the deleted provider is the currently active one, `current` is set to null.
#[tauri::command]
pub async fn delete_provider(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    id: String,
) -> Result<(), String> {
    let _guard = lock.0.lock().await;
    providers::delete_provider(&app_id, &id).map_err(|e| e.to_string())
}

/// Switch the active provider for an app.
///
/// Updates `model_providers.json`, updates `ai.json` provider_ref,
/// and optionally syncs to external tool config files.
#[tauri::command]
pub async fn switch_active_provider(
    lock: State<'_, ProvidersWriteLock>,
    app_id: String,
    provider_id: String,
    sync_tools: Option<Vec<String>>,
) -> Result<SwitchResult, String> {
    let _guard = lock.0.lock().await;

    // Step 1: Update providers store
    providers::switch_active_provider(&app_id, &provider_id).map_err(|e| e.to_string())?;

    // Read back the provider name for the result
    let store = providers::read_store().map_err(|e| e.to_string())?;
    let provider = match app_id.as_str() {
        "claude" => store.claude.providers.get(&provider_id),
        "codex" => store.codex.providers.get(&provider_id),
        "opencode" => store.opencode.providers.get(&provider_id),
        "gemini" => store.gemini.providers.get(&provider_id),
        _ => None,
    }
    .ok_or_else(|| format!("Provider '{}' not found after switch", provider_id))?
    .clone();

    // Step 2: Update ai.json provider_ref
    let mut ai_config = ai_provider::load_config();
    ai_config.provider_ref = Some(AiProviderRef {
        app_id: app_id.clone(),
        provider_id: provider_id.clone(),
    });
    ai_provider::save_config(&ai_config).map_err(|e| e.to_string())?;

    // Step 3: Optionally sync to external tools
    let tools_synced = if let Some(tool_ids) = sync_tools {
        if !tool_ids.is_empty() {
            tool_sync::sync_provider_to_all_tools(&provider, &tool_ids)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    Ok(SwitchResult {
        app_id,
        provider_id,
        provider_name: provider.name,
        tools_synced,
    })
}

// ===========================================================================
// Flat Store Commands (v2 architecture)
// ===========================================================================
//
// These commands operate on the new flat `FlatProvidersStore` format.
// They coexist with the legacy per-app commands above during the transition.

// ---------------------------------------------------------------------------
// Flat store: Read commands (no lock needed)
// ---------------------------------------------------------------------------

/// Returns the full flat provider store (version + providers + tool_activations).
///
/// Performs v1→v2 migration on first access if needed.
#[tauri::command]
pub async fn get_providers_flat() -> Result<FlatProvidersResponse, String> {
    let path = providers::flat_store_path();
    let store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;
    Ok(FlatProvidersResponse {
        version: store.version,
        providers: store.providers,
        tool_activations: store.tool_activations,
    })
}

/// Returns the current tool activations map.
///
/// This is a lightweight read that only returns which providers + models each
/// tool is bound to, without the full provider list.
#[tauri::command]
pub async fn get_tool_activations()
-> Result<std::collections::HashMap<String, ToolBinding>, String> {
    let path = providers::flat_store_path();
    let store = providers::read_flat_store(&path).map_err(|e| e.to_string())?;
    Ok(store.tool_activations)
}

// ---------------------------------------------------------------------------
// Flat store: Write commands (lock required)
// ---------------------------------------------------------------------------

/// Create a new provider in the flat store.
///
/// Validates the entry (name non-empty, URL format), generates a UUID,
/// sets `created_at` and `sort_index`, then persists atomically.
#[tauri::command]
pub async fn create_provider_flat(
    lock: State<'_, ProvidersWriteLock>,
    entry: ProviderEntryFlat,
) -> Result<ProviderEntryFlat, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    let created = providers::create_provider_flat(&mut store, entry).map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    Ok(created)
}

/// Update an existing provider with a partial patch.
///
/// Only non-None fields in the patch are applied. If the provider is currently
/// active for any tools, those tools are automatically re-synced with the
/// updated credentials (preserving each tool's individually selected model).
#[tauri::command]
pub async fn update_provider_flat(
    lock: State<'_, ProvidersWriteLock>,
    id: String,
    patch: ProviderPatchFlat,
) -> Result<ProviderUpdateFlatResult, String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    let updated =
        providers::update_provider_flat(&mut store, &id, patch).map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    let tool_sync_results = tool_sync::resync_active_tools(&store, &id);

    Ok(ProviderUpdateFlatResult {
        provider: updated,
        tool_sync_results,
    })
}

/// Delete a provider from the flat store.
///
/// Also clears any `tool_activations` entries that reference this provider.
/// The caller should handle tool config file restoration (deactivation) before
/// calling this command if needed.
#[tauri::command]
pub async fn delete_provider_flat(
    lock: State<'_, ProvidersWriteLock>,
    id: String,
) -> Result<(), String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    providers::delete_provider_flat(&mut store, &id).map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    Ok(())
}

/// Reorder providers by assigning new `sort_index` values based on the given ID list.
///
/// Each ID in `ordered_ids` gets `sort_index = position` (0-based).
/// Providers not in the list keep their existing `sort_index`.
#[tauri::command]
pub async fn reorder_providers(
    lock: State<'_, ProvidersWriteLock>,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path).map_err(|e| e.to_string())?;

    providers::reorder_providers(&mut store, &ordered_ids).map_err(|e| e.to_string())?;
    providers::write_flat_store(&store, &path).map_err(|e| e.to_string())?;

    Ok(())
}

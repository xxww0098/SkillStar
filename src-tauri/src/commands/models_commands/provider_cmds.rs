//! Provider / preset / ref CRUD commands, on the v4 store.

use super::*;
use super::compat;

// ---------------------------------------------------------------------------
// Read commands (no lock needed)
// ---------------------------------------------------------------------------

/// Returns built-in flat provider presets — single source of truth for the UI.
#[tauri::command]
pub async fn get_provider_presets_flat() -> Result<Vec<ProviderPresetFlat>, AppError> {
    Ok(providers::get_all_presets_flat())
}

/// Point application AI (`ai.json`) at a provider.
///
/// `agent_id` is an id from the agent registry. v3 called this `app_id` and
/// took `"claude"` / `"codex"` — a private two-value id space that shadowed the
/// registry's without matching it. The legacy spelling is still accepted and
/// mapped forward so a UI build in flight does not break.
#[tauri::command]
pub async fn set_app_ai_provider_ref(
    app_id: String,
    provider_id: String,
) -> Result<(), AppError> {
    let agent_id = skillstar_models::normalize_agent_id(&app_id).to_string();
    let provider_id = provider_id.trim();
    if !matches!(agent_id.as_str(), "claude-code" | "codex") {
        return Err(AppError::Other(format!(
            "Unsupported agent for app AI: '{agent_id}'"
        )));
    }
    if provider_id.is_empty() {
        return Err(AppError::Other("provider_id cannot be empty".to_string()));
    }

    let store = load_store()?;
    if store.provider(provider_id).is_none() {
        return Err(AppError::Other(format!(
            "Provider '{provider_id}' not found"
        )));
    }

    let mut ai_config = ai_provider::load_config();
    ai_config.enabled = true;
    ai_config.provider_ref = Some(AiProviderRef {
        agent_id: agent_id.clone(),
        provider_id: provider_id.to_string(),
    });
    ai_config.api_format = match agent_id.as_str() {
        "claude-code" => ai_provider::ApiFormat::Anthropic,
        _ => ai_provider::ApiFormat::Openai,
    };

    ai_provider::resolve_provider_ref(&mut ai_config)?;
    ai_provider::save_config(&ai_config)?;

    Ok(())
}

/// Clear the application AI provider reference (back to manual/local config).
#[tauri::command]
pub async fn clear_app_ai_provider_ref() -> Result<(), AppError> {
    let mut ai_config = ai_provider::load_config();
    ai_config.provider_ref = None;
    ai_provider::save_config(&ai_config)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Store: read
// ---------------------------------------------------------------------------

/// Returns the full provider store.
///
/// Migrates v1 → v4 on first access and, on that same run, repairs the agent
/// config files the old format had already written — see
/// [`providers::load_store_and_repair`]. Then ensures the native-login seed
/// rows exist.
#[tauri::command]
pub async fn get_providers_flat(
    lock: State<'_, ProvidersWriteLock>,
) -> Result<FlatProvidersResponse, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let loaded = providers::load_store_and_repair(&path)
        .map_err(|e| AppError::Other(e.to_string()))?;
    let mut store = loaded.store;
    if providers::ensure_official_providers(&mut store) {
        providers::write_store_v4(&store, &path)?;
    }
    Ok(compat::store_to_flat(&store))
}

// ---------------------------------------------------------------------------
// Store: write (lock required)
// ---------------------------------------------------------------------------

/// Create a new provider.
#[tauri::command]
pub async fn create_provider_flat(
    lock: State<'_, ProvidersWriteLock>,
    entry: ProviderEntryFlat,
) -> Result<ProviderEntryFlat, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    // v3 minted the id here and overwrote whatever the caller sent. v4 lets the
    // caller keep a stable slug, so the id is only generated when absent.
    let created = providers::create_provider(&mut store, compat::provider_from_flat_new(&entry))?;
    providers::write_store_v4(&store, &path)?;

    Ok(compat::provider_to_flat(&created))
}

/// Update an existing provider with a partial patch, then re-sync any agent
/// that is bound to it.
#[tauri::command]
pub async fn update_provider_flat(
    lock: State<'_, ProvidersWriteLock>,
    id: String,
    patch: ProviderPatchFlat,
) -> Result<ProviderUpdateFlatResult, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    let mut provider = store
        .provider(&id)
        .cloned()
        .ok_or_else(|| AppError::Other(format!("Provider '{id}' not found")))?;
    compat::apply_flat_patch(&mut provider, &patch);
    let updated = providers::replace_provider(&mut store, provider)?;
    providers::write_store_v4(&store, &path)?;

    let tool_sync_results = tool_sync::resync_active_tools(&store, &id);

    Ok(ProviderUpdateFlatResult {
        provider: compat::provider_to_flat(&updated),
        tool_sync_results,
    })
}

/// Delete a provider, along with every binding entry and role that named it.
#[tauri::command]
pub async fn delete_provider_flat(
    lock: State<'_, ProvidersWriteLock>,
    id: String,
) -> Result<(), AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    providers::delete_provider(&mut store, &id)?;
    providers::write_store_v4(&store, &path)?;

    Ok(())
}

/// Reorder providers by assigning `sort_index = position`.
#[tauri::command]
pub async fn reorder_providers(
    lock: State<'_, ProvidersWriteLock>,
    ordered_ids: Vec<String>,
) -> Result<(), AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = load_store()?;

    providers::reorder_providers(&mut store, &ordered_ids)?;
    providers::write_store_v4(&store, &path)?;

    Ok(())
}

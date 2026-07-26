//! Provider / preset / ref CRUD commands (flat store v2).
//!
//! Carved out of `models_commands` mechanically — no logic changes.

use super::*;

// ---------------------------------------------------------------------------
// Read commands (no lock needed)
// ---------------------------------------------------------------------------

/// Returns built-in flat provider presets (v2) — single source of truth for the UI.
#[tauri::command]
pub async fn get_provider_presets_flat() -> Result<Vec<ProviderPresetFlat>, AppError> {
    Ok(providers::get_all_presets_flat())
}

/// Point application AI (`ai.json`) at a flat-store provider.
///
/// `app_id` must be `claude` (Anthropic) or `codex` (OpenAI). Validates that the
/// provider exists and can be resolved before persisting.
#[tauri::command]
pub async fn set_app_ai_provider_ref(app_id: String, provider_id: String) -> Result<(), AppError> {
    let app_id = app_id.trim();
    let provider_id = provider_id.trim();
    if !matches!(app_id, "claude" | "codex") {
        return Err(AppError::Other(format!(
            "Unsupported app_id for app AI: '{app_id}'"
        )));
    }
    if provider_id.is_empty() {
        return Err(AppError::Other("provider_id cannot be empty".to_string()));
    }

    let path = providers::flat_store_path();
    let store = providers::migrate_store_if_needed(&path)?;
    if !store.providers.iter().any(|p| p.id == provider_id) {
        return Err(AppError::Other(format!(
            "Provider '{}' not found",
            provider_id
        )));
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

    ai_provider::resolve_provider_ref(&mut ai_config)?;
    ai_provider::save_config(&ai_config)?;

    Ok(())
}

/// Clear application AI provider reference (switch back to manual/local config).
#[tauri::command]
pub async fn clear_app_ai_provider_ref() -> Result<(), AppError> {
    let mut ai_config = ai_provider::load_config();
    ai_config.provider_ref = None;
    ai_provider::save_config(&ai_config)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Flat store: Read commands (no lock needed)
// ---------------------------------------------------------------------------

/// Returns the full flat provider store (version + providers + tool_activations).
///
/// Performs v1→v2 migration on first access if needed.
#[tauri::command]
pub async fn get_providers_flat() -> Result<FlatProvidersResponse, AppError> {
    let path = providers::flat_store_path();
    let store = providers::migrate_store_if_needed(&path)?;
    Ok(FlatProvidersResponse {
        version: store.version,
        providers: store.providers,
        tool_activations: store.tool_activations,
    })
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
) -> Result<ProviderEntryFlat, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path)?;

    let created = providers::create_provider_flat(&mut store, entry)?;
    providers::write_flat_store(&store, &path)?;

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
) -> Result<ProviderUpdateFlatResult, AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path)?;

    let updated = providers::update_provider_flat(&mut store, &id, patch)?;
    providers::write_flat_store(&store, &path)?;

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
) -> Result<(), AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path)?;

    providers::delete_provider_flat(&mut store, &id)?;
    providers::write_flat_store(&store, &path)?;

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
) -> Result<(), AppError> {
    let _guard = lock.0.lock().await;
    let path = providers::flat_store_path();
    let mut store = providers::migrate_store_if_needed(&path)?;

    providers::reorder_providers(&mut store, &ordered_ids)?;
    providers::write_flat_store(&store, &path)?;

    Ok(())
}

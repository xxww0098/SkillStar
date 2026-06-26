//! Provider CRUD: flat-store and legacy per-app operations, tool activation, v1 presets.

use super::*;

// ---------------------------------------------------------------------------
// Flat store CRUD operations (v2 architecture)
// ---------------------------------------------------------------------------

/// Validate that a URL string is a valid HTTP/HTTPS URL.
///
/// Returns Ok(()) if the URL is valid, or an error describing the issue.
fn validate_url(url_str: &str) -> Result<()> {
    if url_str.is_empty() {
        return Ok(()); // Empty URLs are allowed (e.g., base_url_anthropic may be empty)
    }
    let parsed =
        Url::parse(url_str).with_context(|| format!("Invalid URL format: '{}'", url_str))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => bail!("URL must use http or https scheme, got: '{}'", scheme),
    }
}

/// Create a new provider in the flat store.
///
/// - Validates that `name` is non-empty
/// - Validates URL format for `base_url_openai` and `base_url_anthropic`
/// - Generates a new UUID for the `id` field (overwrites any provided id)
/// - Sets `created_at` to the current timestamp if not already set
/// - Sets `sort_index` to max existing + 1
/// - Pushes the entry to `store.providers`
///
/// # Errors
/// Returns an error if:
/// - `name` is empty
/// - `base_url_openai` or `base_url_anthropic` has an invalid URL format
pub fn create_provider_flat(
    store: &mut FlatProvidersStore,
    mut entry: ProviderEntryFlat,
) -> Result<ProviderEntryFlat> {
    // Validate name is non-empty
    if entry.name.trim().is_empty() {
        bail!("Provider name must not be empty");
    }

    // Validate URL formats
    validate_url(&entry.base_url_openai)?;
    validate_url(&entry.base_url_anthropic)?;
    validate_url(&entry.models_url)?;

    // Generate new UUID (overwrite any provided id)
    entry.id = Uuid::new_v4().to_string();

    // Set created_at to current timestamp if not set
    if entry.created_at.is_none() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        entry.created_at = Some(now);
    }

    // Set sort_index to max existing + 1
    let max_sort_index = store
        .providers
        .iter()
        .map(|p| p.sort_index)
        .max()
        .unwrap_or(0);
    entry.sort_index = if store.providers.is_empty() {
        0
    } else {
        max_sort_index + 1
    };

    // Push to store
    store.providers.push(entry.clone());

    Ok(entry)
}

/// Update an existing provider in the flat store with a partial patch.
///
/// Finds the provider by `id` and applies all non-None fields from the patch.
///
/// # Errors
/// Returns an error if:
/// - No provider with the given `id` exists in the store
pub fn update_provider_flat(
    store: &mut FlatProvidersStore,
    id: &str,
    patch: ProviderPatchFlat,
) -> Result<ProviderEntryFlat> {
    let provider = store
        .providers
        .iter_mut()
        .find(|p| p.id == id)
        .with_context(|| format!("Provider '{}' not found", id))?;

    // Apply non-None fields from patch
    if let Some(name) = patch.name {
        provider.name = name;
    }
    if let Some(base_url_openai) = patch.base_url_openai {
        provider.base_url_openai = base_url_openai;
    }
    if let Some(base_url_anthropic) = patch.base_url_anthropic {
        provider.base_url_anthropic = base_url_anthropic;
    }
    if let Some(models_url) = patch.models_url {
        provider.models_url = models_url;
    }
    if let Some(api_key) = patch.api_key {
        provider.api_key = api_key;
    }
    if let Some(models) = patch.models {
        provider.models = models;
    }
    if let Some(default_model) = patch.default_model {
        provider.default_model = default_model;
    }
    if let Some(sort_index) = patch.sort_index {
        provider.sort_index = sort_index;
    }
    if let Some(preset_id) = patch.preset_id {
        provider.preset_id = Some(preset_id);
    }
    if let Some(icon_color) = patch.icon_color {
        provider.icon_color = Some(icon_color);
    }
    if let Some(notes) = patch.notes {
        provider.notes = Some(notes);
    }
    if let Some(meta) = patch.meta {
        provider.meta = Some(meta);
    }
    if let Some(codex_wire_api) = patch.codex_wire_api {
        provider.codex_wire_api = codex_wire_api;
    }
    if let Some(codex_auth_mode) = patch.codex_auth_mode {
        provider.codex_auth_mode = codex_auth_mode;
    }

    Ok(provider.clone())
}

/// Delete a provider from the flat store by ID.
///
/// Also cleans up `tool_activations`: any activation referencing this provider
/// is set to `None`.
///
/// # Errors
/// Returns an error if no provider with the given `id` exists in the store.
pub fn delete_provider_flat(store: &mut FlatProvidersStore, id: &str) -> Result<()> {
    let idx = store
        .providers
        .iter()
        .position(|p| p.id == id)
        .with_context(|| format!("Provider '{}' not found", id))?;

    // Remove the provider
    store.providers.remove(idx);

    // Clean up tool_activations: set any activation referencing this provider to None
    for activation in store.tool_activations.values_mut() {
        if let Some(act) = activation
            && act.provider_id == id
        {
            *activation = None;
        }
    }

    Ok(())
}

/// Reorder providers by assigning new `sort_index` values based on the given ID list.
///
/// For each ID in `ordered_ids`, assigns `sort_index = position index` (0-based).
/// Providers not in `ordered_ids` keep their existing `sort_index`.
///
/// # Errors
/// Returns an error if any ID in `ordered_ids` doesn't exist in the store.
pub fn reorder_providers(store: &mut FlatProvidersStore, ordered_ids: &[String]) -> Result<()> {
    // Validate all IDs exist
    for id in ordered_ids {
        if !store.providers.iter().any(|p| p.id == *id) {
            bail!("Provider '{}' not found in store", id);
        }
    }

    // Assign sort_index based on position in ordered_ids
    for (index, id) in ordered_ids.iter().enumerate() {
        if let Some(provider) = store.providers.iter_mut().find(|p| p.id == *id) {
            provider.sort_index = index as u32;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tool activation/deactivation (v2 architecture)
// ---------------------------------------------------------------------------

/// Activate a provider for a specific Agent tool.
///
/// This updates the `tool_activations` map to record which provider and model
/// a given tool should use. Only one provider can be active per tool at a time —
/// activating a new provider automatically replaces any previous activation.
///
/// # Validation
/// - The provider must exist in the store
/// - The required URL must be non-empty based on the tool:
///   - `"claude-code"` requires `base_url_anthropic` to be non-empty
///   - `"codex"` requires `base_url_openai` to be non-empty
///   - Other tools: require `base_url_openai` to be non-empty (default)
///
/// # Model Resolution
/// Uses the provided `model` if given, otherwise falls back to the provider's `default_model`.
///
/// # Settings
/// Optional per-tool settings (e.g. Codex's `wire_api` and `auth_mode`).
/// When `None`, the tool's previous settings are preserved (if re-activating),
/// otherwise sensible defaults are used by the sync layer.
///
/// # Returns
/// The `ToolActivation` that was inserted into the map.
///
/// # Errors
/// - Provider not found
/// - Required URL is empty for the target tool
pub fn activate_tool(
    store: &mut FlatProvidersStore,
    provider_id: &str,
    tool_id: &str,
    model: Option<&str>,
    settings: Option<serde_json::Value>,
) -> Result<ToolActivation> {
    // 1. Find provider by provider_id
    let provider = store
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .with_context(|| format!("Provider '{}' not found", provider_id))?;

    // 2. Validate the required URL is non-empty based on tool_id
    match tool_id {
        "claude-code" => {
            if provider.base_url_anthropic.trim().is_empty() {
                bail!(
                    "Provider '{}' has no Anthropic-compatible endpoint (base_url_anthropic is empty). \
                     Claude Code requires an Anthropic-compatible URL.",
                    provider.name
                );
            }
        }
        "codex" | "opencode" => {
            if provider.base_url_openai.trim().is_empty() {
                bail!(
                    "Provider '{}' has no OpenAI-compatible endpoint (base_url_openai is empty). \
                     {} requires an OpenAI-compatible URL.",
                    provider.name,
                    tool_id
                );
            }
        }
        _ => {
            // Default: require base_url_openai
            if provider.base_url_openai.trim().is_empty() {
                bail!(
                    "Provider '{}' has no OpenAI-compatible endpoint (base_url_openai is empty). \
                     Tool '{}' requires an OpenAI-compatible URL.",
                    provider.name,
                    tool_id
                );
            }
        }
    }

    // 3. Determine model: use provided model, or fall back to provider's default_model
    let resolved_model = match model {
        Some(m) if !m.trim().is_empty() => m.to_string(),
        _ => provider.default_model.clone(),
    };

    // 4. Determine settings: use provided, or preserve previous activation's settings
    let resolved_settings = settings.or_else(|| {
        store
            .tool_activations
            .get(tool_id)
            .and_then(|opt| opt.as_ref())
            .and_then(|a| a.settings.clone())
    });

    // 5. Create ToolActivation
    let activation = ToolActivation {
        provider_id: provider_id.to_string(),
        model: resolved_model,
        settings: resolved_settings,
        last_sync_at: None,
    };

    // 6. Insert into store.tool_activations (replaces any previous activation for this tool)
    store
        .tool_activations
        .insert(tool_id.to_string(), Some(activation.clone()));

    // 7. Return the activation
    Ok(activation)
}

/// Update only the settings of an active tool without changing provider or model.
///
/// This is useful for front-end toggles like Codex's `wire_api` or `auth_mode`
/// where the user wants to tweak per-tool config without a full re-activation.
///
/// # Errors
/// - Tool is not currently active
pub fn update_tool_settings(
    store: &mut FlatProvidersStore,
    tool_id: &str,
    settings: serde_json::Value,
) -> Result<ToolActivation> {
    let activation = store
        .tool_activations
        .get(tool_id)
        .and_then(|opt| opt.as_ref())
        .with_context(|| format!("Tool '{}' is not currently active", tool_id))?;

    let updated = ToolActivation {
        provider_id: activation.provider_id.clone(),
        model: activation.model.clone(),
        settings: Some(settings),
        last_sync_at: activation.last_sync_at,
    };

    store
        .tool_activations
        .insert(tool_id.to_string(), Some(updated.clone()));

    Ok(updated)
}

/// Deactivate a tool by removing its activation entry.
///
/// Sets the tool's entry in `tool_activations` to `None`, effectively clearing
/// the active provider for that tool.
///
/// # Returns
/// The previous `ToolActivation` (if any) so the caller can use it for backup
/// restoration or undo operations.
pub fn deactivate_tool(
    store: &mut FlatProvidersStore,
    tool_id: &str,
) -> Result<Option<ToolActivation>> {
    // Remove or set to None the tool_id entry in tool_activations
    let previous = store
        .tool_activations
        .insert(tool_id.to_string(), None)
        .flatten();

    Ok(previous)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the mutable AppProviders for a given app_id.
fn get_app_mut<'a>(store: &'a mut ProvidersStore, app_id: &str) -> Result<&'a mut AppProviders> {
    match app_id {
        "claude" => Ok(&mut store.claude),
        "codex" => Ok(&mut store.codex),
        "opencode" => Ok(&mut store.opencode),
        "gemini" => Ok(&mut store.gemini),
        _ => bail!("Unknown app_id: {app_id}"),
    }
}

/// Get the immutable AppProviders for a given app_id.
pub(crate) fn get_app<'a>(store: &'a ProvidersStore, app_id: &str) -> &'a AppProviders {
    match app_id {
        "claude" => &store.claude,
        "codex" => &store.codex,
        "opencode" => &store.opencode,
        "gemini" => &store.gemini,
        _ => &store.claude,
    }
}

/// Validate a provider entry before creation.
fn validate_entry(entry: &ProviderEntry) -> Result<()> {
    // Name: non-empty, max 64 chars
    if entry.name.is_empty() {
        bail!("Provider name must not be empty");
    }
    if entry.name.len() > 64 {
        bail!(
            "Provider name must be at most 64 characters (got {})",
            entry.name.len()
        );
    }

    // Base URL: must be a valid URL
    let settings: ProviderSettings = serde_json::from_value(entry.settings_config.clone())
        .context("Invalid settings_config structure")?;

    if Url::parse(&settings.base_url).is_err() {
        bail!("Invalid base_url: {}", settings.base_url);
    }

    // Models: at least one
    if settings.models.is_empty() {
        bail!("At least one model mapping is required");
    }

    Ok(())
}

pub fn get_providers(app_id: &str) -> Result<(HashMap<String, ProviderEntry>, Option<String>)> {
    let store = read_store()?;
    let app = get_app(&store, app_id);
    Ok((app.providers.clone(), app.current.clone()))
}

// ---------------------------------------------------------------------------
// CRUD operations
// ---------------------------------------------------------------------------

/// Create a new provider entry for the given app_id.
///
/// Validates name length, URL format, model count, and ID uniqueness.
/// If this is the first provider for the app, it becomes the current active provider.
pub fn create_provider(app_id: &str, entry: ProviderEntry) -> Result<ProviderEntry> {
    create_provider_at(app_id, entry, &store_path())
}

/// Create a new provider entry, writing to a specific store path.
pub fn create_provider_at(
    app_id: &str,
    mut entry: ProviderEntry,
    path: &Path,
) -> Result<ProviderEntry> {
    validate_entry(&entry)?;

    let mut store = read_store_from(path)?;
    let app = get_app_mut(&mut store, app_id)?;

    // Check ID uniqueness
    if app.providers.contains_key(&entry.id) {
        bail!(
            "Provider ID '{}' already exists for app '{}'",
            entry.id,
            app_id
        );
    }

    // Assign metadata
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    entry.created_at = Some(now);
    entry.sort_index = Some(app.providers.len() as u32);

    // Auto-activate if first provider
    if app.current.is_none() {
        app.current = Some(entry.id.clone());
    }

    // Insert
    app.providers.insert(entry.id.clone(), entry.clone());

    // Persist
    write_store_to(&store, path)?;

    Ok(entry)
}

/// Update an existing provider with a partial patch.
///
/// Only non-None fields in the patch are applied.
pub fn update_provider(app_id: &str, id: &str, patch: ProviderPatch) -> Result<ProviderEntry> {
    update_provider_at(app_id, id, patch, &store_path())
}

/// Update an existing provider, writing to a specific store path.
pub fn update_provider_at(
    app_id: &str,
    id: &str,
    patch: ProviderPatch,
    path: &Path,
) -> Result<ProviderEntry> {
    let mut store = read_store_from(path)?;
    let app = get_app_mut(&mut store, app_id)?;

    let entry = app
        .providers
        .get_mut(id)
        .with_context(|| format!("Provider '{}' not found in app '{}'", id, app_id))?;

    // Apply patch fields
    if let Some(name) = &patch.name {
        if name.is_empty() {
            bail!("Provider name must not be empty");
        }
        if name.len() > 64 {
            bail!("Provider name must be at most 64 characters");
        }
        entry.name = name.clone();
    }
    if let Some(category) = patch.category {
        entry.category = category;
    }
    if let Some(settings_config) = patch.settings_config {
        // Validate the new settings if provided
        let settings: ProviderSettings = serde_json::from_value(settings_config.clone())
            .context("Invalid settings_config structure")?;
        if Url::parse(&settings.base_url).is_err() {
            bail!("Invalid base_url: {}", settings.base_url);
        }
        if settings.models.is_empty() {
            bail!("At least one model mapping is required");
        }
        entry.settings_config = settings_config;
    }
    if let Some(website_url) = patch.website_url {
        entry.website_url = Some(website_url);
    }
    if let Some(api_key_url) = patch.api_key_url {
        entry.api_key_url = Some(api_key_url);
    }
    if let Some(icon_color) = patch.icon_color {
        entry.icon_color = Some(icon_color);
    }
    if let Some(notes) = patch.notes {
        entry.notes = Some(notes);
    }
    if let Some(sort_index) = patch.sort_index {
        entry.sort_index = Some(sort_index);
    }
    if let Some(meta) = patch.meta {
        entry.meta = Some(meta);
    }

    let updated = entry.clone();

    // Persist
    write_store_to(&store, path)?;

    Ok(updated)
}

/// Delete a provider by ID.
///
/// If the deleted provider is the currently active one, `current` is set to None.
pub fn delete_provider(app_id: &str, id: &str) -> Result<()> {
    delete_provider_at(app_id, id, &store_path())
}

/// Delete a provider by ID, writing to a specific store path.
pub fn delete_provider_at(app_id: &str, id: &str, path: &Path) -> Result<()> {
    let mut store = read_store_from(path)?;
    let app = get_app_mut(&mut store, app_id)?;

    if app.providers.remove(id).is_none() {
        bail!("Provider '{}' not found in app '{}'", id, app_id);
    }

    // Nullify current if we just deleted the active provider
    if app.current.as_deref() == Some(id) {
        app.current = None;
    }

    // Persist
    write_store_to(&store, path)?;

    Ok(())
}

/// Switch the active provider for an app.
///
/// Updates the `current` field to the given provider_id.
pub fn switch_active_provider(app_id: &str, provider_id: &str) -> Result<()> {
    switch_active_provider_at(app_id, provider_id, &store_path())
}

/// Switch the active provider, writing to a specific store path.
pub fn switch_active_provider_at(app_id: &str, provider_id: &str, path: &Path) -> Result<()> {
    let mut store = read_store_from(path)?;
    let app = get_app_mut(&mut store, app_id)?;

    if !app.providers.contains_key(provider_id) {
        bail!(
            "Provider '{}' not found in app '{}', cannot activate",
            provider_id,
            app_id
        );
    }

    app.current = Some(provider_id.to_string());

    // Persist
    write_store_to(&store, path)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

/// Returns the list of built-in provider presets.
pub fn get_provider_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "official".to_string(),
            name: "Official (Anthropic)".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key_url: "https://console.anthropic.com/settings/keys".to_string(),
            icon_color: "#D97757".to_string(),
            models: vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-opus-4-20250514".to_string(),
            ],
        },
        ProviderPreset {
            id: "official".to_string(),
            name: "Official (OpenAI)".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key_url: "https://platform.openai.com/api-keys".to_string(),
            icon_color: "#10A37F".to_string(),
            models: vec![
                "codex-mini-latest".to_string(),
                "o3".to_string(),
                "o4-mini".to_string(),
            ],
        },
        ProviderPreset {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key_url: "https://platform.deepseek.com/api_keys".to_string(),
            icon_color: "#4D6BFE".to_string(),
            models: vec!["deepseek-chat".to_string(), "deepseek-reasoner".to_string()],
        },
        ProviderPreset {
            id: "kimi".to_string(),
            name: "Kimi".to_string(),
            base_url: "https://api.moonshot.cn/v1".to_string(),
            api_key_url: "https://platform.moonshot.cn/console/api-keys".to_string(),
            icon_color: "#5B45E0".to_string(),
            models: vec![
                "moonshot-v1-128k".to_string(),
                "moonshot-v1-32k".to_string(),
            ],
        },
        ProviderPreset {
            id: "glm".to_string(),
            name: "GLM".to_string(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            api_key_url: "https://open.bigmodel.cn/usercenter/apikeys".to_string(),
            icon_color: "#3366FF".to_string(),
            models: vec!["glm-4-plus".to_string(), "glm-4-flash".to_string()],
        },
    ]
}

/// Resolve the correct preset for a given app_id and preset_id.
///
/// For "official" preset: Claude uses Anthropic, Codex uses OpenAI.
/// For other presets (deepseek, kimi, glm): same preset for any app.
fn resolve_preset(app_id: &str, preset_id: &str) -> Result<ProviderPreset> {
    let presets = get_provider_presets();

    match preset_id {
        "official" => {
            let target_name = match app_id {
                "claude" => "Official (Anthropic)",
                "codex" => "Official (OpenAI)",
                _ => "Official (Anthropic)",
            };
            presets
                .into_iter()
                .find(|p| p.id == "official" && p.name == target_name)
                .with_context(|| format!("Official preset not found for app '{}'", app_id))
        }
        _ => presets
            .into_iter()
            .find(|p| p.id == preset_id)
            .with_context(|| format!("Preset '{}' not found", preset_id)),
    }
}

/// Create a provider from a built-in preset.
///
/// The user only needs to provide an API key; all other fields are pre-filled from the preset.
pub fn create_from_preset(app_id: &str, preset_id: &str, api_key: &str) -> Result<ProviderEntry> {
    create_from_preset_at(app_id, preset_id, api_key, &store_path())
}

/// Create a provider from a built-in preset, writing to a specific store path.
pub fn create_from_preset_at(
    app_id: &str,
    preset_id: &str,
    api_key: &str,
    path: &Path,
) -> Result<ProviderEntry> {
    let preset = resolve_preset(app_id, preset_id)?;

    let models: Vec<ModelMapping> = preset
        .models
        .iter()
        .map(|m| ModelMapping {
            source_model: m.clone(),
            target_model: m.clone(),
            enabled: true,
        })
        .collect();

    let settings = ProviderSettings {
        base_url: preset.base_url.clone(),
        api_key: api_key.to_string(),
        models,
        timeout_ms: None,
        max_retries: None,
    };

    let entry = ProviderEntry {
        id: Uuid::new_v4().to_string(),
        name: preset.name.clone(),
        category: "cloud".to_string(),
        settings_config: serde_json::to_value(&settings)
            .context("Failed to serialize preset settings")?,
        preset_id: Some(preset.id.clone()),
        website_url: None,
        api_key_url: Some(preset.api_key_url.clone()),
        icon_color: Some(preset.icon_color.clone()),
        notes: None,
        created_at: None, // Will be set by create_provider_at
        sort_index: None, // Will be set by create_provider_at
        meta: None,
    };

    create_provider_at(app_id, entry, path)
}

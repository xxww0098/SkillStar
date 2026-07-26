//! Provider CRUD: flat-store operations and tool activation.

use super::*;

// ---------------------------------------------------------------------------
// Flat store CRUD operations (v2 architecture)
// ---------------------------------------------------------------------------

/// Infer the recommended Codex `wire_api` and `auth_mode` for a provider from
/// its OpenAI-compatible base URL.
///
/// Rule (single source of truth; mirrored verbatim in the frontend):
/// - `api.openai.com` → `("responses", "api_key")` — OpenAI's native Responses
///   API + official key path.
/// - everything else → `("chat", "third_party")` — third-party OpenAI-compatible
///   endpoints only implement `/v1/chat/completions` (not the Responses API),
///   and `third_party` routes the key through `env_key` so `auth.json` is never
///   touched (a concurrent ChatGPT OAuth login survives intact).
///
/// Empty / non-OpenAI URLs fall through to `("chat", "third_party")` — the safe
/// default for any custom OpenAI-compatible endpoint.
pub fn recommended_codex_defaults(base_url_openai: &str) -> (&'static str, &'static str) {
    if base_url_openai.contains("api.openai.com") {
        ("responses", "api_key")
    } else {
        ("chat", "third_party")
    }
}

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

    // Infer Codex defaults from the base URL when the caller didn't set them
    // explicitly. PresetPicker (and most callers) omit these fields, so serde's
    // hardcoded default (`responses` + `api_key`) fills them in — which breaks
    // every third-party provider (DeepSeek/Kimi/GLM/…) since they only speak
    // `/v1/chat/completions`. Here we correct that to the URL-appropriate pair.
    // Any caller that deliberately chose a non-default value is preserved.
    let (rec_wire, rec_auth) = recommended_codex_defaults(&entry.base_url_openai);
    if entry.codex_wire_api == default_codex_wire_api() {
        entry.codex_wire_api = rec_wire.to_string();
    }
    if entry.codex_auth_mode == default_codex_auth_mode() {
        entry.codex_auth_mode = rec_auth.to_string();
    }

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

    // Clean up tool_activations: drop any binding entry referencing this
    // provider and re-clamp the active pointer so it stays valid.
    for binding in store.tool_activations.values_mut() {
        if let Some(pos) = binding.entries.iter().position(|e| e.provider_id == id) {
            binding.entries.remove(pos);
            if binding.active_index >= pos && binding.active_index > 0 {
                binding.active_index -= 1;
            }
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
        "codex" | "opencode" | "pi" => {
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

    // 4. Determine settings: use provided, or preserve the existing entry's
    //    settings for this provider if re-activating it.
    let resolved_settings = settings.or_else(|| {
        store
            .tool_activations
            .get(tool_id)
            .and_then(|binding| {
                binding
                    .entries
                    .iter()
                    .find(|e| e.provider_id == provider_id)
            })
            .and_then(|e| e.settings.clone())
    });

    // 5. Create the entry
    let entry = ToolActivation {
        provider_id: provider_id.to_string(),
        model: resolved_model,
        settings: resolved_settings,
        last_sync_at: None,
    };

    // 6. Insert into the tool's binding.
    //    - Multi-provider agents (codex, opencode): upsert the entry (update the
    //      model/settings if this provider is already bound, else append) and
    //      point `active_index` at it.
    //    - Single-provider agents (claude-code, gemini): the binding holds at
    //      most one entry, so activating replaces it wholesale.
    let binding = store
        .tool_activations
        .entry(tool_id.to_string())
        .or_default();
    if agent_supports_multiple_providers(tool_id) {
        if let Some(pos) = binding
            .entries
            .iter()
            .position(|e| e.provider_id == provider_id)
        {
            binding.entries[pos] = entry.clone();
            binding.active_index = pos;
        } else {
            binding.entries.push(entry.clone());
            binding.active_index = binding.entries.len() - 1;
        }
    } else {
        binding.entries = vec![entry.clone()];
        binding.active_index = 0;
    }

    // 7. Return the activated entry
    Ok(entry)
}

/// Whether a tool's config format natively supports several providers coexisting
/// (Codex `[model_providers.*]`, OpenCode `provider.*`, Pi `providers.*`).
/// Single-provider agents (claude-code, gemini) write a single global env block
/// and hold one entry.
///
/// This is the one place agent "kind" is decided in the store layer; keep it
/// data-driven here rather than scattering `tool_id == ...` checks.
pub fn agent_supports_multiple_providers(tool_id: &str) -> bool {
    matches!(tool_id, "codex" | "opencode" | "pi")
}

/// Point a multi-provider tool's active pointer at an already-bound provider
/// without otherwise touching the entry list.
///
/// # Errors
/// - Tool has no binding / the provider is not bound to this tool
pub fn set_active_binding(
    store: &mut FlatProvidersStore,
    tool_id: &str,
    provider_id: &str,
) -> Result<ToolActivation> {
    let binding = store
        .tool_activations
        .get_mut(tool_id)
        .filter(|b| !b.is_empty())
        .with_context(|| format!("Tool '{}' has no active bindings", tool_id))?;

    let pos = binding
        .entries
        .iter()
        .position(|e| e.provider_id == provider_id)
        .with_context(|| {
            format!(
                "Provider '{}' is not bound to tool '{}'",
                provider_id, tool_id
            )
        })?;
    binding.active_index = pos;
    Ok(binding.entries[pos].clone())
}

/// Remove a single provider entry from a tool's binding.
///
/// Returns the binding's new active entry (if any remain). `active_index` is
/// re-clamped so it keeps pointing at a valid entry.
///
/// # Errors
/// - The provider is not bound to this tool
pub fn remove_binding_entry(
    store: &mut FlatProvidersStore,
    tool_id: &str,
    provider_id: &str,
) -> Result<Option<ToolActivation>> {
    let binding = store
        .tool_activations
        .get_mut(tool_id)
        .with_context(|| format!("Tool '{}' has no bindings", tool_id))?;

    let pos = binding
        .entries
        .iter()
        .position(|e| e.provider_id == provider_id)
        .with_context(|| {
            format!(
                "Provider '{}' is not bound to tool '{}'",
                provider_id, tool_id
            )
        })?;

    binding.entries.remove(pos);
    // Re-clamp the active pointer: if we removed at/below it, shift left.
    if binding.active_index >= pos && binding.active_index > 0 {
        binding.active_index -= 1;
    }
    Ok(binding.active().cloned())
}

/// Update only the settings of an active tool without changing provider or model.
///
/// This is useful for front-end toggles like Codex's `wire_api` or `auth_mode`
/// where the user wants to tweak per-tool config without a full re-activation.
///
/// Updates the binding's currently-active entry.
///
/// # Errors
/// - Tool is not currently active (no entries bound)
pub fn update_tool_settings(
    store: &mut FlatProvidersStore,
    tool_id: &str,
    settings: serde_json::Value,
) -> Result<ToolActivation> {
    let binding = store
        .tool_activations
        .get_mut(tool_id)
        .filter(|b| !b.is_empty())
        .with_context(|| format!("Tool '{}' is not currently active", tool_id))?;

    let active = binding
        .active_mut()
        .with_context(|| format!("Tool '{}' has no active binding", tool_id))?;
    active.settings = Some(settings);
    Ok(active.clone())
}

/// Deactivate a tool by clearing all of its provider bindings.
///
/// Empties the tool's binding (`entries: []`), clearing every bound provider.
///
/// # Returns
/// The previously-active entry (if any) so the caller can use it for backup
/// restoration or undo operations.
pub fn deactivate_tool(
    store: &mut FlatProvidersStore,
    tool_id: &str,
) -> Result<Option<ToolActivation>> {
    let previous = store
        .tool_activations
        .insert(tool_id.to_string(), ToolBinding::default())
        .and_then(|b| b.active().cloned());

    Ok(previous)
}


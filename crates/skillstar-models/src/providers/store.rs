//! Provider store paths, file I/O, and v1->v2 migration.

use super::*;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

pub(crate) fn store_path() -> PathBuf {
    // Route through the centralized paths layer (honors SKILLSTAR_DATA_DIR and
    // the cfg(test) HOME sandbox) instead of `dirs::home_dir()` directly —
    // otherwise tests that set SKILLSTAR_DATA_DIR still clobber the user's real
    // ~/.skillstar/config/model_providers.json. Behavior-identical in production
    // (data_root defaults to ~/.skillstar). See lessons in tool_sync.
    skillstar_core::infra::paths::config_dir().join("model_providers.json")
}

/// Returns the default path for the flat provider store (`model_providers.json`).
///
/// This is the same file as the legacy store — the format is detected by the
/// presence of a `version` field.
pub fn flat_store_path() -> PathBuf {
    store_path()
}

// ---------------------------------------------------------------------------
// Flat store read/write (v2 architecture)
// ---------------------------------------------------------------------------

/// Read the flat provider store (v2) from a specific path.
///
/// Returns a default empty store if:
/// - The file doesn't exist
/// - The file cannot be read (permission error, etc.)
/// - The file contains invalid JSON
///
/// This ensures the application can always start with a valid state.
pub fn read_flat_store(path: &Path) -> Result<FlatProvidersStore> {
    if !path.exists() {
        return Ok(FlatProvidersStore::default());
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            warn!(
                "Failed to read flat store at {}: {e}. Returning default store.",
                path.display()
            );
            return Ok(FlatProvidersStore::default());
        }
    };
    // Strip BOM if present
    let text = text.trim_start_matches('\u{FEFF}');
    match serde_json::from_str::<FlatProvidersStore>(text) {
        Ok(store) => Ok(store),
        Err(e) => {
            warn!(
                "Malformed JSON in flat store {}: {e}. Returning default empty store.",
                path.display()
            );
            Ok(FlatProvidersStore::default())
        }
    }
}

/// Write the flat provider store (v2) to a specific path atomically.
///
/// Uses a temp file + rename strategy to prevent partial writes:
/// 1. Creates parent directories if they don't exist
/// 2. Serializes the store to pretty JSON
/// 3. Writes to a temporary file (`.json.tmp`) in the same directory
/// 4. Renames the temp file to the target path (atomic on most filesystems)
pub fn write_flat_store(store: &FlatProvidersStore, path: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Serialize to pretty JSON
    let json =
        serde_json::to_string_pretty(store).context("Failed to serialize FlatProvidersStore")?;

    // Write to a temp file in the same directory, then rename for atomicity
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json.as_bytes())
        .with_context(|| format!("Failed to write temp file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Migration: v1 → v2
// ---------------------------------------------------------------------------

/// Detect store version and migrate if needed.
///
/// - If the file does not exist, returns a default empty v2 store.
/// - If the file is already v2 (has `version: 2`), parses and returns it directly.
/// - Otherwise, parses as v1 `ProvidersStore`, converts to v2, backs up the original
///   file as `.json.bak`, writes the new v2 store, and returns it.
pub fn migrate_store_if_needed(path: &Path) -> Result<FlatProvidersStore> {
    // Edge case: file not found → return default
    if !path.exists() {
        return Ok(FlatProvidersStore::default());
    }

    // Read raw JSON
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read store file {}", path.display()))?;
    let raw = raw.trim_start_matches('\u{FEFF}');

    // Parse as generic JSON Value
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                "Failed to parse {} as JSON: {e}. Returning default store.",
                path.display()
            );
            return Ok(FlatProvidersStore::default());
        }
    };

    // Check if already v2
    if value.get("version").and_then(|v| v.as_u64()) == Some(2) {
        // Already v2 — parse directly
        let store: FlatProvidersStore = serde_json::from_value(value)
            .context("Failed to parse v2 FlatProvidersStore")?;
        return Ok(store);
    }

    // Parse as v1 ProvidersStore
    let old: ProvidersStore = serde_json::from_value(value)
        .context("Failed to parse v1 ProvidersStore during migration")?;

    // Convert v1 → v2
    let new_store = convert_v1_to_v2(&old);

    // Backup original file before overwriting
    let backup_path = path.with_extension("json.bak");
    if let Err(e) = std::fs::copy(path, &backup_path) {
        warn!(
            "Failed to create backup at {}: {e}. Proceeding with migration anyway.",
            backup_path.display()
        );
    }

    // Write the new v2 store
    write_flat_store(&new_store, path)?;

    Ok(new_store)
}

/// Convert a v1 `ProvidersStore` (per-app) to a v2 `FlatProvidersStore` (flat).
///
/// Deduplication strategy: providers with the same (base_url, api_key) pair are
/// merged into a single entry, combining their model lists.
fn convert_v1_to_v2(old: &ProvidersStore) -> FlatProvidersStore {
    use std::collections::HashSet;

    // Collect all providers from all apps with their app context
    struct CollectedProvider {
        entry: ProviderEntry,
        base_url: String,
        api_key: String,
        models: Vec<String>,
    }

    let apps: &[(&str, &AppProviders)] = &[
        ("claude", &old.claude),
        ("codex", &old.codex),
        ("opencode", &old.opencode),
        ("gemini", &old.gemini),
    ];

    let mut collected: Vec<CollectedProvider> = Vec::new();

    for (_app_id, app) in apps {
        for entry in app.providers.values() {
            // Try to extract settings from settings_config
            let (base_url, api_key, models) = extract_v1_settings(&entry.settings_config);
            collected.push(CollectedProvider {
                entry: entry.clone(),
                base_url,
                api_key,
                models,
            });
        }
    }

    // Deduplicate by (base_url, api_key) — merge models
    // Key: (base_url, api_key) → index in deduped vec
    let mut dedup_map: HashMap<(String, String), usize> = HashMap::new();
    let mut deduped: Vec<ProviderEntryFlat> = Vec::new();

    for cp in &collected {
        let key = (cp.base_url.clone(), cp.api_key.clone());
        if let Some(&idx) = dedup_map.get(&key) {
            // Merge models into existing entry
            let existing = &mut deduped[idx];
            let mut existing_models: HashSet<String> =
                existing.models.iter().cloned().collect();
            for model in &cp.models {
                if existing_models.insert(model.clone()) {
                    existing.models.push(model.clone());
                }
            }
        } else {
            let idx = deduped.len();
            // Try to inherit models_url from the matching preset, if any.
            let models_url = cp
                .entry
                .preset_id
                .as_ref()
                .and_then(|preset_id| {
                    get_all_presets_flat()
                        .into_iter()
                        .find(|p| &p.id == preset_id)
                        .map(|p| p.models_url)
                })
                .unwrap_or_default();
            let flat_entry = ProviderEntryFlat {
                id: Uuid::new_v4().to_string(),
                name: cp.entry.name.clone(),
                // Map v1 base_url to base_url_openai (most v1 providers use OpenAI-compatible format)
                base_url_openai: cp.base_url.clone(),
                // base_url_anthropic left empty — can be derived later from presets
                base_url_anthropic: String::new(),
                models_url,
                api_key: cp.api_key.clone(),
                models: cp.models.clone(),
                default_model: cp.models.first().cloned().unwrap_or_default(),
                sort_index: idx as u32,
                preset_id: cp.entry.preset_id.clone(),
                icon_color: cp.entry.icon_color.clone(),
                notes: cp.entry.notes.clone(),
                created_at: cp.entry.created_at,
                meta: cp.entry.meta.clone(),
                codex_wire_api: default_codex_wire_api(),
                codex_auth_mode: default_codex_auth_mode(),
            };
            dedup_map.insert(key, idx);
            deduped.push(flat_entry);
        }
    }

    // Build tool_activations from each app's `current` field
    let mut tool_activations: HashMap<String, Option<ToolActivation>> = HashMap::new();

    // Map app_id to tool_id: claude.current → tool_activations["claude-code"]
    //                         codex.current → tool_activations["codex"]
    let app_to_tool: &[(&str, &AppProviders)] = &[
        ("claude-code", &old.claude),
        ("codex", &old.codex),
    ];

    for &(tool_id, app) in app_to_tool {
        if let Some(ref current_id) = app.current {
            // Find the provider in the app's providers
            if let Some(entry) = app.providers.get(current_id) {
                let (base_url, api_key, models) = extract_v1_settings(&entry.settings_config);
                let key = (base_url, api_key);

                // Find the corresponding flat provider by dedup key
                if let Some(&idx) = dedup_map.get(&key) {
                    let flat_provider = &deduped[idx];
                    let model = models.first().cloned().unwrap_or_default();
                    tool_activations.insert(
                        tool_id.to_string(),
                        Some(ToolActivation {
                            provider_id: flat_provider.id.clone(),
                            model,
                            settings: None,
                            last_sync_at: None,
                        }),
                    );
                }
            }
        }
    }

    FlatProvidersStore {
        version: 2,
        providers: deduped,
        tool_activations,
    }
}

/// Extract base_url, api_key, and model names from a v1 settings_config Value.
fn extract_v1_settings(settings_config: &Value) -> (String, String, Vec<String>) {
    // Try to parse as ProviderSettings
    if let Ok(settings) = serde_json::from_value::<ProviderSettings>(settings_config.clone()) {
        let models: Vec<String> = settings
            .models
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.source_model.clone())
            .collect();
        return (settings.base_url, settings.api_key, models);
    }

    // Fallback: try to extract fields directly from the JSON object
    let base_url = settings_config
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let api_key = settings_config
        .get("api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let models = settings_config
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    m.get("source_model")
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    (base_url, api_key, models)
}

// ---------------------------------------------------------------------------
// Store read/write (v1 — legacy)
// ---------------------------------------------------------------------------

/// Read the providers store from a specific path.
/// Returns a default empty store if the file doesn't exist or contains malformed JSON.
pub fn read_store_from(path: &Path) -> Result<ProvidersStore> {
    if !path.exists() {
        return Ok(ProvidersStore::default());
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            warn!("Failed to read {}: {e}. Returning default store.", path.display());
            return Ok(ProvidersStore::default());
        }
    };
    let text = text.trim_start_matches('\u{FEFF}');
    match serde_json::from_str::<ProvidersStore>(text) {
        Ok(store) => Ok(store),
        Err(e) => {
            warn!(
                "Malformed JSON in {}: {e}. Returning default empty store.",
                path.display()
            );
            Ok(ProvidersStore::default())
        }
    }
}

/// Read the providers store from the default path.
/// Returns a default empty store if the file doesn't exist or contains malformed JSON.
pub fn read_store() -> Result<ProvidersStore> {
    read_store_from(&store_path())
}

/// Write the providers store to a specific path atomically (write to temp file, then rename).
pub fn write_store_to(store: &ProvidersStore, path: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Serialize to pretty JSON
    let json =
        serde_json::to_string_pretty(store).context("Failed to serialize ProvidersStore")?;

    // Write to a temp file in the same directory, then rename for atomicity
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json.as_bytes())
        .with_context(|| format!("Failed to write temp file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to rename {} to {}", temp_path.display(), path.display()))?;

    Ok(())
}

/// Write the providers store to the default path atomically.
pub fn write_store(store: &ProvidersStore) -> Result<()> {
    write_store_to(store, &store_path())
}

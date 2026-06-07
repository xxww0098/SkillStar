//! Model catalog normalization and merging.

use super::*;

pub const MODEL_CATALOG_META_KEY: &str = "model_catalog";

/// Parse an OpenAI-compatible `/models` response into normalized entries.
pub fn catalog_from_provider_models(body: &Value) -> Vec<ModelCatalogEntry> {
    body.get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| model_entry_from_value(item))
                .collect()
        })
        .unwrap_or_default()
}

/// Parse known registry shapes into normalized model entries.
///
/// Supported shapes:
/// - CLIProxyAPI: `{ "claude": [{ "id": "...", ... }], ... }`
/// - models.dev: `{ "openai": { "models": { "gpt-4o": { ... } } }, ... }`
/// - OpenAI-compatible: `{ "data": [{ "id": "...", ... }] }`
pub fn catalog_from_registry(body: &Value) -> Vec<ModelCatalogEntry> {
    let mut entries = catalog_from_provider_models(body);

    if let Some(root) = body.as_object() {
        for provider_value in root.values() {
            if let Some(items) = provider_value.as_array() {
                entries.extend(items.iter().filter_map(model_entry_from_value));
                continue;
            }

            if let Some(models) = provider_value.get("models").and_then(Value::as_object) {
                entries.extend(models.iter().filter_map(|(id, value)| {
                    let mut entry = model_entry_from_value(value)?;
                    if entry.id.trim().is_empty() {
                        entry.id = id.clone();
                    }
                    Some(entry)
                }));
            }
        }
    }

    dedupe_catalog(entries)
}

/// Merge provider-discovered models with registry metadata.
///
/// Provider models define the allowed output set. Registry entries fill in
/// display names, context, output limit, and cost when the IDs match.
pub fn merge_model_catalog(
    provider_models: Vec<ModelCatalogEntry>,
    registries: &[Vec<ModelCatalogEntry>],
) -> ModelCatalogFetchResult {
    let mut merged = Vec::new();
    let mut model_ids = Vec::new();

    for mut entry in dedupe_catalog(provider_models) {
        let model_id = entry.id.clone();
        for registry in registries {
            if let Some(candidate) = registry.iter().find(|candidate| candidate.id == model_id) {
                merge_entry(&mut entry, candidate);
            }
        }
        if !model_id.trim().is_empty() {
            model_ids.push(model_id);
            merged.push(entry);
        }
    }

    let missing_cost_count = merged.iter().filter(|entry| entry.cost.is_none()).count();
    ModelCatalogFetchResult {
        models: model_ids,
        catalog: merged,
        metadata_sources: Vec::new(),
        missing_cost_count,
    }
}

/// Read a cached catalog from a provider's `meta` object.
pub fn catalog_from_meta(meta: Option<&Value>) -> Vec<ModelCatalogEntry> {
    meta.and_then(|value| value.get(MODEL_CATALOG_META_KEY))
        .and_then(|value| serde_json::from_value::<Vec<ModelCatalogEntry>>(value.clone()).ok())
        .unwrap_or_default()
}

fn model_entry_from_value(value: &Value) -> Option<ModelCatalogEntry> {
    let id = first_string(value, &["id", "model", "name"])?;
    let display_name = first_string(value, &["display_name", "displayName"]).or_else(|| {
        value
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|name| name != &id)
    });
    let source_name = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|name| name != &id && Some(name.clone()) != display_name);

    Some(ModelCatalogEntry {
        id,
        display_name,
        source_name,
        description: first_string(value, &["description"]),
        context_length: first_u64(value, &["context_length", "inputTokenLimit"])
            .or_else(|| nested_u64(value, "limit", "context")),
        max_completion_tokens: first_u64(value, &["max_completion_tokens", "outputTokenLimit"])
            .or_else(|| nested_u64(value, "limit", "output")),
        cost: first_value(value, &["cost", "pricing", "price"]),
        raw: Some(value.clone()),
    })
}

fn merge_entry(target: &mut ModelCatalogEntry, source: &ModelCatalogEntry) {
    if target.display_name.is_none() {
        target.display_name = source.display_name.clone();
    }
    if target.source_name.is_none() {
        target.source_name = source.source_name.clone();
    }
    if target.description.is_none() {
        target.description = source.description.clone();
    }
    if target.context_length.is_none() {
        target.context_length = source.context_length;
    }
    if target.max_completion_tokens.is_none() {
        target.max_completion_tokens = source.max_completion_tokens;
    }
    if target.cost.is_none() {
        target.cost = source.cost.clone();
    }
    if target.raw.is_none() {
        target.raw = source.raw.clone();
    }
}

fn dedupe_catalog(entries: Vec<ModelCatalogEntry>) -> Vec<ModelCatalogEntry> {
    let mut by_id: HashMap<String, ModelCatalogEntry> = HashMap::new();
    let mut order = Vec::new();

    for entry in entries {
        let id = entry.id.trim().to_string();
        if id.is_empty() {
            continue;
        }
        if let Some(existing) = by_id.get_mut(&id) {
            merge_entry(existing, &entry);
        } else {
            order.push(id.clone());
            by_id.insert(id, entry);
        }
    }

    order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect()
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn first_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn nested_u64(value: &Value, parent: &str, key: &str) -> Option<u64> {
    value
        .get(parent)
        .and_then(|value| value.get(key))
        .and_then(Value::as_u64)
}

fn first_value(value: &Value, keys: &[&str]) -> Option<Value> {
    keys.iter().find_map(|key| value.get(*key).cloned())
}

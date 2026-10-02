//! Fit a request's effort to the levels in the models.dev cache.
//!
//! The lookup key is the upstream id `provider/model`. A display name is not
//! a key. `model_efforts` in `model_gateway.json` may keep a subset of those
//! levels; a missing, empty, or non-matching subset offers the whole catalog. A group member
//! may be stored as `provider/model:<level>`; that level wins over the one in
//! the request, and the subset does not rewrite it. An unknown model keeps
//! the effort it arrived with. This module does not map `xhigh` to `max`; the
//! Claude process bridge owns that mapping.

use serde_json::{Value, json};

const KNOWN: &[&str] = &["none", "minimal", "low", "medium", "high", "xhigh", "max"];
const RANK: &[&str] = &[
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

/// Levels offered for this id. A saved subset replaces the cache list.
/// Missing data, and a missing or empty subset, is the cache list or empty.
///
/// The cache file is only read. A display name is not consulted.
pub fn model_efforts(id: &str) -> Vec<String> {
    let (model, _) = split_member(id.trim());
    if model.starts_with("group/") || !model.contains('/') {
        return Vec::new();
    }
    offered_levels(model)
}

/// The catalog level for `member`, written into `reasoning_effort`.
///
/// `member` empty means the body's `model`. A saved group uses its first
/// member. Bytes stay as they arrived when the effort does not change.
pub fn apply_upstream_effort(body: &[u8], member: &str) -> Vec<u8> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return body.to_vec();
    };
    let Some(object) = value.as_object() else {
        return body.to_vec();
    };
    let body_model = object.get("model").and_then(Value::as_str).unwrap_or("");
    let source = member_source(member, body_model);
    let (model, fixed) = split_member(&source);
    let current = object
        .get("reasoning_effort")
        .and_then(Value::as_str)
        .unwrap_or("");
    if fixed.is_empty() && current.is_empty() {
        return body.to_vec();
    }
    let catalog = catalog_levels(model);
    let fitted = if fixed.is_empty() {
        fit_offered(current, model, &catalog)
    } else {
        fit_effort(fixed, &catalog)
    };
    if fitted == current {
        return body.to_vec();
    }
    let mut value = value;
    let Some(object) = value.as_object_mut() else {
        return body.to_vec();
    };
    object.insert("reasoning_effort".to_string(), json!(fitted));
    serde_json::to_vec(&value).unwrap_or_else(|_| body.to_vec())
}

fn member_source(explicit: &str, body_model: &str) -> String {
    if !explicit.is_empty() {
        return explicit.to_string();
    }
    if let Some(id) = body_model.trim().strip_prefix("group/") {
        let id = id.trim();
        if let Some(member) = crate::store::groups::stored_groups()
            .into_iter()
            .find(|group| group.id == id)
            .and_then(|group| group.members.into_iter().next())
        {
            return member;
        }
    }
    body_model.to_string()
}

fn offered_levels(model: &str) -> Vec<String> {
    let catalog = catalog_levels(model);
    kept_in_catalog(model, &catalog).unwrap_or(catalog)
}

fn fit_offered(want: &str, model: &str, catalog: &[String]) -> String {
    match kept_in_catalog(model, catalog) {
        Some(kept) => fit_kept(want, catalog, &kept),
        None => fit_effort(want, catalog),
    }
}

/// Catalog order. The first kept level that is not before `want`, or the
/// last kept level when `want` is past all of them. A `want` the catalog
/// does not list has no place in that order, so the first kept level is used.
fn fit_kept(want: &str, catalog: &[String], kept: &[String]) -> String {
    let Some(at) = catalog.iter().position(|level| level == want) else {
        return kept.first().cloned().unwrap_or_else(|| want.to_string());
    };
    for level in kept {
        if catalog
            .iter()
            .position(|item| item == level)
            .is_some_and(|index| index >= at)
        {
            return level.clone();
        }
    }
    kept.last().cloned().unwrap_or_else(|| want.to_string())
}

fn kept_in_catalog(model: &str, catalog: &[String]) -> Option<Vec<String>> {
    if catalog.is_empty() {
        return None;
    }
    let names = stored_names(model)?;
    if names.is_empty() {
        return None;
    }
    let kept: Vec<String> = catalog
        .iter()
        .filter(|level| names.iter().any(|name| name.eq_ignore_ascii_case(level)))
        .cloned()
        .collect();
    if kept.is_empty() { None } else { Some(kept) }
}

fn stored_names(model: &str) -> Option<Vec<String>> {
    let doc = read_gateway();
    let list = doc
        .get("model_efforts")
        .and_then(Value::as_object)
        .and_then(|map| map.get(model))
        .and_then(Value::as_array)?;
    Some(
        list.iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn read_gateway() -> Value {
    let path = skillstar_core::infra::paths::config_dir().join("model_gateway.json");
    let Ok(bytes) = std::fs::read(path) else {
        return json!({});
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value @ Value::Object(_)) => value,
        _ => json!({}),
    }
}

fn fit_effort(want: &str, levels: &[String]) -> String {
    if levels.is_empty() || levels.iter().any(|level| level == want) {
        return want.to_string();
    }
    let Some(at) = RANK.iter().position(|level| *level == want) else {
        return want.to_string();
    };
    let mut best = want.to_string();
    let mut dist = RANK.len();
    for level in levels {
        let Some(index) = RANK.iter().position(|item| *item == level) else {
            continue;
        };
        if level == "none" {
            continue;
        }
        let gap = index.abs_diff(at);
        if gap < dist || (gap == dist && index > at) {
            best = level.clone();
            dist = gap;
        }
    }
    best
}

fn split_member(id: &str) -> (&str, &str) {
    let Some((model, level)) = id.rsplit_once(':') else {
        return (id, "");
    };
    if !KNOWN.contains(&level) || catalog_lists_exact(id) {
        return (id, "");
    }
    (model, level)
}

fn catalog_lists_exact(id: &str) -> bool {
    let Some((provider, model)) = id.split_once('/') else {
        return false;
    };
    catalog_model(provider, model).is_some()
}

fn catalog_levels(model_id: &str) -> Vec<String> {
    let Some((provider, model)) = model_id.split_once('/') else {
        return Vec::new();
    };
    if provider.is_empty() || provider == "group" || model.is_empty() || model.contains('/') {
        return Vec::new();
    }
    let Some(entry) = catalog_model(provider, model) else {
        return Vec::new();
    };
    entry
        .get("reasoning_options")
        .and_then(Value::as_array)
        .and_then(|options| {
            options.iter().find(|option| option.get("type").and_then(Value::as_str) == Some("effort"))
        })
        .and_then(|option| option.get("values"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn catalog_model(provider: &str, model: &str) -> Option<Value> {
    let bytes = crate::catalog::cache::models_dev_load();
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value
        .get(provider)?
        .get("models")?
        .get(model)
        .cloned()
}

//! Which catalog rows an agent is shown.
//!
//! `visible` in `model_gateway.json` maps an agent id to names. A name is a
//! family tag, a provider id, or a group id (`group/<id>` counts as that
//! group). `family` is a field on a provider or group row in the same file.
//! A missing agent and an empty list both show every row. Routing does not
//! read this map. A display name is not a name.

use serde_json::Value;

use crate::GROUP_PREFIX;

/// Names that narrow `agent`. `None` means the agent sees every row.
pub fn visible_names(agent: &str) -> Option<Vec<String>> {
    let agent = agent.trim();
    if agent.is_empty() {
        return None;
    }
    let doc = read_doc();
    let map = doc.get("visible")?.as_object()?;
    let value = map
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(agent))
        .map(|(_, value)| value)?;
    let list = value.as_array()?;
    let names: Vec<String> = list
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();
    if names.is_empty() { None } else { Some(names) }
}

/// Whether `agent` is shown `id`. An agent that is not narrowed sees every id.
pub fn model_shown(agent: &str, id: &str) -> bool {
    let Some(allowed) = visible_names(agent) else {
        return true;
    };
    let aliases = names_of(id);
    allowed.iter().any(|name| {
        aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(name))
    })
}

/// Catalog ids this agent is shown, provider models then saved groups.
pub fn shown_model_ids(agent: &str) -> Vec<String> {
    catalog_ids()
        .into_iter()
        .filter(|id| model_shown(agent, id))
        .collect()
}

/// Ids to write into an agent's model list.
///
/// No narrowing and an empty catalog keeps the single saved ref, which is
/// what the writers already emit. A narrowed list does not add a hidden ref
/// back; the saved field outside the list is left alone by the caller.
pub fn listed_ids(agent: &str, model_ref: &str) -> Vec<String> {
    let mut ids = shown_model_ids(agent);
    if visible_names(agent).is_none() {
        if ids.is_empty() {
            if model_ref.is_empty() {
                return Vec::new();
            }
            return vec![model_ref.to_string()];
        }
        if !model_ref.is_empty() && !ids.iter().any(|id| id == model_ref) {
            ids.push(model_ref.to_string());
        }
        return ids;
    }
    ids
}

/// The catalog or a saved group actually has this id.
///
/// A display name does not count. Visibility does not count either: a hidden
/// id still answers.
pub fn catalog_serves(id: &str) -> bool {
    let id = id.trim();
    if let Some(bare) = id.strip_prefix(GROUP_PREFIX) {
        let bare = bare.trim();
        return !bare.is_empty()
            && !bare.contains('/')
            && crate::group::stored_group_ids().iter().any(|saved| saved == bare);
    }
    let Some((provider, model)) = id.split_once('/') else {
        return false;
    };
    if provider.is_empty() || provider == "group" || model.is_empty() || model.contains('/') {
        return false;
    }
    let Ok(value) = serde_json::from_slice::<Value>(&crate::models_dev::models_dev_load()) else {
        return false;
    };
    value
        .get(provider)
        .and_then(|entry| entry.get("models"))
        .and_then(Value::as_object)
        .is_some_and(|models| models.contains_key(model))
}

fn names_of(id: &str) -> Vec<String> {
    let id = id.trim();
    if let Some(bare) = id.strip_prefix(GROUP_PREFIX) {
        let bare = bare.trim();
        if bare.is_empty() {
            return Vec::new();
        }
        let mut names = vec![bare.to_string(), format!("{GROUP_PREFIX}{bare}")];
        if let Some(family) = family_of("groups", bare) {
            names.push(family);
        }
        return names;
    }
    let Some((provider, model)) = id.split_once('/') else {
        return Vec::new();
    };
    if provider.is_empty() || model.is_empty() {
        return Vec::new();
    }
    let mut names = vec![provider.to_string()];
    if let Some(family) = family_of("providers", provider) {
        names.push(family);
    }
    names
}

fn family_of(list: &str, id: &str) -> Option<String> {
    read_doc()
        .get(list)?
        .as_array()?
        .iter()
        .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
        .and_then(|row| row.get("family"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|family| !family.is_empty())
        .map(str::to_string)
}

fn catalog_ids() -> Vec<String> {
    let mut ids = Vec::new();
    if let Ok(Value::Object(providers)) =
        serde_json::from_slice::<Value>(&crate::models_dev::models_dev_load())
    {
        for (provider, entry) in providers {
            if provider.is_empty() || provider.contains('/') {
                continue;
            }
            let Some(models) = entry.get("models").and_then(Value::as_object) else {
                continue;
            };
            for model in models.keys() {
                if model.is_empty() || model.contains('/') {
                    continue;
                }
                ids.push(format!("{provider}/{model}"));
            }
        }
    }
    for id in crate::group::stored_group_ids() {
        if id.is_empty() || id.contains('/') {
            continue;
        }
        ids.push(format!("{GROUP_PREFIX}{id}"));
    }
    ids
}

fn read_doc() -> Value {
    let path = skillstar_core::infra::paths::config_dir().join("model_gateway.json");
    let Ok(bytes) = std::fs::read(path) else {
        return serde_json::json!({});
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value @ Value::Object(_)) => value,
        _ => serde_json::json!({}),
    }
}

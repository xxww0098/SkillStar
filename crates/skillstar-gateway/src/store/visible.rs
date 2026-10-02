//! Which catalog rows an agent is shown: the stored `visible` map and the
//! `family` field read off provider and group rows.
//!
//! `visible` in `model_gateway.json` maps an agent id to names. A name is a
//! family tag, a provider id, or a group id (`group/<id>` counts as that
//! group). `family` is a field on a provider or group row in the same file.
//! A missing agent and an empty list both show every row. Routing does not
//! read this map. A display name is not a name. The projections built on
//! this map live in `crate::visible`.

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

/// The `family` field on one provider or group row, trimmed. Empty is `None`.
pub(crate) fn family_of(list: &str, id: &str) -> Option<String> {
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

/// Every catalog id, provider models then saved groups.
pub(crate) fn catalog_ids() -> Vec<String> {
    let mut ids = crate::catalog::catalog_ids();
    for id in crate::store::groups::stored_group_ids() {
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

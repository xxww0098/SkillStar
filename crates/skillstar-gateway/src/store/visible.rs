//! Which catalog rows an agent is shown: the stored `visible` map and the
//! `family` field read off provider and group rows.
//!
//! `visible` in `model_gateway.json` maps an agent id to names. A name is a
//! family tag, a provider id, or a group id (`group/<id>` counts as that
//! group). `family` is a field on a provider or group row in the same file.
//! A missing agent and an empty list both show every row. Routing does not
//! read this map. A display name is not a name. The projections built on
//! this map live in `crate::visible`.

use super::doc::ModelGatewayDoc;
use crate::GROUP_PREFIX;

/// Names that narrow `agent`. `None` means the agent sees every row.
pub fn visible_names(agent: &str) -> Option<Vec<String>> {
    let agent = agent.trim();
    if agent.is_empty() {
        return None;
    }
    let doc = ModelGatewayDoc::open_lenient();
    let (_, stored) = doc
        .visible()
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(agent))?;
    let names: Vec<String> = stored
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();
    if names.is_empty() { None } else { Some(names) }
}

/// The `family` field on one provider or group row, trimmed. Empty is `None`.
pub(crate) fn family_of(list: &str, id: &str) -> Option<String> {
    let doc = ModelGatewayDoc::open_lenient();
    let rows = match list {
        "providers" => doc.providers(),
        "groups" => doc.groups(),
        _ => return None,
    };
    rows.iter()
        .find(|row| row.id == id)
        .and_then(|row| row.family.as_deref())
        .map(str::trim)
        .filter(|family| !family.is_empty())
        .map(str::to_string)
}

/// Every id the visible map can name: catalog models plus saved groups —
/// wider than [`crate::catalog::catalog_ids`], which lists catalog models
/// only.
pub(crate) fn nameable_ids() -> Vec<String> {
    let mut ids = crate::catalog::catalog_ids();
    for id in crate::store::groups::stored_group_ids() {
        if id.is_empty() || id.contains('/') {
            continue;
        }
        ids.push(format!("{GROUP_PREFIX}{id}"));
    }
    ids
}

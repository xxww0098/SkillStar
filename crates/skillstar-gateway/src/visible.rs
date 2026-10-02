//! Which catalog rows an agent is shown: the projections built on the
//! stored `visible` map. The map itself, and the `family` field, are read
//! by `store::visible`.

use crate::GROUP_PREFIX;
use crate::store::visible::{catalog_ids, family_of, visible_names};

/// Whether `agent` is shown `id`. An agent that is not narrowed sees every id.
pub fn model_shown(agent: &str, id: &str) -> bool {
    let Some(allowed) = visible_names(agent) else {
        return true;
    };
    let aliases = names_of(id);
    allowed
        .iter()
        .any(|name| aliases.iter().any(|alias| alias.eq_ignore_ascii_case(name)))
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
            && crate::store::groups::stored_group_ids()
                .iter()
                .any(|saved| saved == bare);
    }
    let Some((provider, model)) = id.split_once('/') else {
        return false;
    };
    crate::catalog::serves(provider, model)
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

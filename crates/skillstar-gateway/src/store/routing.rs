//! Saved routing mode and affinity for one provider or group.
//!
//! The fields live on that row in `config_dir()/model_gateway.json`.
//! Smart and auto are left out of the file. A missing file reads as those
//! two. The file is opened only through [`ModelGatewayDoc`] (see
//! `store::doc`). This module does not open `model_providers.json`.

use super::doc::{ModelGatewayDoc, OwnerRow};
use super::groups::GROUP_PREFIX;
use crate::route::affinity::AffinityMode;
use crate::route::order::RouteMode;

/// The gateway file could not be read or replaced, or the id is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveRoutingError {
    Store,
}

impl std::fmt::Display for SaveRoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("routing_store")
    }
}

/// Mode and affinity stored for `id`. A missing file is smart and auto.
pub fn routing_state(
    owner: RouteOwner,
    id: &str,
) -> Result<(RouteMode, AffinityMode), SaveRoutingError> {
    let id = normalize(owner, id)?;
    Ok(read_routing(&ModelGatewayDoc::open_lenient(), owner, &id))
}

/// Write `mode` and `affinity` onto this provider or group.
///
/// Other fields on the row, and the rest of the file, stay. Smart removes
/// `routing`. Auto removes `affinity`. A file that is not a JSON object is
/// left untouched.
pub fn save_routing(
    owner: RouteOwner,
    id: &str,
    mode: RouteMode,
    affinity: AffinityMode,
) -> Result<(), SaveRoutingError> {
    let id = normalize(owner, id)?;
    let mut doc = ModelGatewayDoc::open().map_err(|_| SaveRoutingError::Store)?;
    write_routing(&mut doc, owner, &id, mode, affinity);
    doc.save().map_err(|_| SaveRoutingError::Store)
}

fn normalize(owner: RouteOwner, id: &str) -> Result<String, SaveRoutingError> {
    let trimmed = id.trim();
    let bare = match owner {
        RouteOwner::Provider => trimmed,
        RouteOwner::Group => trimmed
            .strip_prefix(GROUP_PREFIX)
            .unwrap_or(trimmed)
            .trim(),
    };
    if bare.is_empty() {
        return Err(SaveRoutingError::Store);
    }
    if owner == RouteOwner::Group && bare.contains('/') {
        return Err(SaveRoutingError::Store);
    }
    Ok(bare.to_string())
}

/// The routing write lens: first-match-by-id over the owner's rows, a
/// missing row is appended at the end (see `store::doc`).
pub(crate) fn write_routing(
    doc: &mut ModelGatewayDoc,
    owner: RouteOwner,
    id: &str,
    mode: RouteMode,
    affinity: AffinityMode,
) {
    let rows = match owner {
        RouteOwner::Provider => doc.providers_mut(),
        RouteOwner::Group => doc.groups_mut(),
    };
    let row = match rows.iter_mut().find(|row| row.id == id) {
        Some(row) => row,
        None => {
            rows.push(OwnerRow::from_id(id));
            rows.last_mut().expect("just pushed")
        }
    };
    apply(row, mode, affinity);
}

/// Smart means the `routing` key is gone, Auto means `affinity` is gone.
fn apply(row: &mut OwnerRow, mode: RouteMode, affinity: AffinityMode) {
    row.routing = match mode {
        RouteMode::Smart => None,
        other => Some(other.as_str().to_string()),
    };
    row.affinity = match affinity {
        AffinityMode::Auto => None,
        other => Some(other.as_str().to_string()),
    };
}

/// Whose `routing` field to read in `model_gateway.json`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteOwner {
    Provider,
    Group,
}

/// Routing stored for one provider or group.
///
/// A missing file, a missing row, a missing or empty `routing` field, an
/// unknown string, or a file that is not JSON is smart. This does not
/// create or rewrite the file.
pub fn stored_route_mode(owner: RouteOwner, id: &str) -> RouteMode {
    read_routing(&ModelGatewayDoc::open_lenient(), owner, id).0
}

/// Mode and affinity stored on one row of an already-open document.
///
/// The row lookup is first-match-by-id over the owner's list; a missing
/// row, a missing field, or an unknown string reads as the default
/// (smart/auto), which is also what a key-absent row means.
fn read_routing(
    doc: &ModelGatewayDoc,
    owner: RouteOwner,
    id: &str,
) -> (RouteMode, AffinityMode) {
    let rows = match owner {
        RouteOwner::Provider => doc.providers(),
        RouteOwner::Group => doc.groups(),
    };
    let row = rows.iter().find(|row| row.id == id);
    (
        row.and_then(|row| row.routing.as_deref())
            .map(RouteMode::parse)
            .unwrap_or(RouteMode::Smart),
        row.and_then(|row| row.affinity.as_deref())
            .map(AffinityMode::parse)
            .unwrap_or(AffinityMode::Auto),
    )
}

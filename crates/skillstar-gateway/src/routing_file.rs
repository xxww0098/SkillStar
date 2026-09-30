//! Saved routing mode and affinity for one provider or group.
//!
//! The fields live on that row in `config_dir()/model_gateway.json`.
//! Smart and auto are left out of the file. A missing file reads as those
//! two. This module does not open `model_providers.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use skillstar_core::infra::fs_ops::atomic_write;

use crate::affinity::AffinityMode;
use crate::group::GROUP_PREFIX;
use crate::route::{RouteMode, RouteOwner, stored_route_mode};

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
    Ok((stored_route_mode(owner, &id), affinity_of(owner, &id)))
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
    let path = gateway_path();
    let mut doc = load_object(&path)?;
    write_row(&mut doc, owner, &id, mode, affinity)?;
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|_| SaveRoutingError::Store)?;
    atomic_write(&path, &bytes).map_err(|_| SaveRoutingError::Store)
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

fn list_key(owner: RouteOwner) -> &'static str {
    match owner {
        RouteOwner::Provider => "providers",
        RouteOwner::Group => "groups",
    }
}

fn gateway_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join("model_gateway.json")
}

fn load_object(path: &Path) -> Result<Value, SaveRoutingError> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value @ Value::Object(_)) => Ok(value),
            _ => Err(SaveRoutingError::Store),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(json!({})),
        Err(_) => Err(SaveRoutingError::Store),
    }
}

fn write_row(
    doc: &mut Value,
    owner: RouteOwner,
    id: &str,
    mode: RouteMode,
    affinity: AffinityMode,
) -> Result<(), SaveRoutingError> {
    let key = list_key(owner);
    let list = doc
        .as_object_mut()
        .ok_or(SaveRoutingError::Store)?
        .entry(key)
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(list) = list.as_array_mut() else {
        return Err(SaveRoutingError::Store);
    };
    if let Some(existing) = list
        .iter_mut()
        .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
    {
        let Some(object) = existing.as_object_mut() else {
            return Err(SaveRoutingError::Store);
        };
        apply(object, mode, affinity);
        return Ok(());
    }
    let mut object = Map::new();
    object.insert("id".to_string(), Value::String(id.to_string()));
    apply(&mut object, mode, affinity);
    list.push(Value::Object(object));
    Ok(())
}

fn apply(object: &mut Map<String, Value>, mode: RouteMode, affinity: AffinityMode) {
    match mode {
        RouteMode::Smart => {
            object.remove("routing");
        }
        other => {
            object.insert(
                "routing".to_string(),
                Value::String(other.as_str().to_string()),
            );
        }
    }
    match affinity {
        AffinityMode::Auto => {
            object.remove("affinity");
        }
        other => {
            object.insert(
                "affinity".to_string(),
                Value::String(other.as_str().to_string()),
            );
        }
    }
}

fn affinity_of(owner: RouteOwner, id: &str) -> AffinityMode {
    let Ok(bytes) = fs::read(gateway_path()) else {
        return AffinityMode::Auto;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return AffinityMode::Auto;
    };
    let Some(list) = value.get(list_key(owner)).and_then(Value::as_array) else {
        return AffinityMode::Auto;
    };
    let Some(row) = list
        .iter()
        .find(|row| row.get("id").and_then(Value::as_str) == Some(id))
    else {
        return AffinityMode::Auto;
    };
    AffinityMode::parse(row.get("affinity").and_then(Value::as_str).unwrap_or(""))
}

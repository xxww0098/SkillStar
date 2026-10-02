//! Display names stored in `model_gateway.json`.
//!
//! The object is `model_names`. A key is `provider/model`. The value is the
//! name the picker shows. The models.dev cache is not opened for writing, and
//! the translator does not read this object.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use skillstar_core::infra::fs_ops::atomic_write;

const NAME_CAP: usize = 80;

/// The name was refused, or the gateway file could not be replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveModelNameError {
    Name,
    Store,
}

impl std::fmt::Display for SaveModelNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Name => "model_name",
            Self::Store => "model_store",
        })
    }
}

/// Names already stored. A missing file is empty and is not created.
pub fn stored_model_names() -> BTreeMap<String, String> {
    let Ok(bytes) = fs::read(gateway_path()) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return BTreeMap::new();
    };
    let Some(object) = value.get("model_names").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    object
        .iter()
        .filter_map(|(id, name)| name.as_str().map(|name| (id.clone(), name.to_string())))
        .collect()
}

/// The stored name, or `id` when there is nothing usable.
pub fn model_label(id: &str, names: &BTreeMap<String, String>) -> String {
    let Some(name) = names.get(id) else {
        return id.to_string();
    };
    let name = name.trim();
    if name.is_empty() || name.chars().count() > NAME_CAP || name_forbidden(name) {
        return id.to_string();
    }
    name.to_string()
}

/// Remember a display name for one catalog model. Other keys stay.
pub fn save_model_name(id: &str, name: &str) -> Result<(), SaveModelNameError> {
    let id = id.trim();
    let name = name.trim();
    if name.is_empty() || name.chars().count() > NAME_CAP || name_forbidden(name) || !catalog_lists(id) {
        return Err(SaveModelNameError::Name);
    }
    let path = gateway_path();
    let mut doc = load_object(&path)?;
    let object = doc.as_object_mut().ok_or(SaveModelNameError::Store)?;
    if let Some(value) = object.get("model_names") {
        if !value.is_object() {
            return Err(SaveModelNameError::Store);
        }
    } else {
        object.insert("model_names".to_string(), json!({}));
    }
    let names = object
        .get_mut("model_names")
        .and_then(Value::as_object_mut)
        .ok_or(SaveModelNameError::Store)?;
    names.insert(id.to_string(), json!(name));
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|_| SaveModelNameError::Store)?;
    atomic_write(&path, &bytes).map_err(|_| SaveModelNameError::Store)
}

fn name_forbidden(name: &str) -> bool {
    name.contains('\n')
        || name.contains('\r')
        || name.contains("://")
        || name.contains("sk-")
}

fn catalog_lists(id: &str) -> bool {
    let Some((provider, model)) = id.split_once('/') else {
        return false;
    };
    if provider.is_empty() || provider == "group" || model.is_empty() || model.contains('/') {
        return false;
    }
    let Ok(value) = serde_json::from_slice::<Value>(&crate::catalog::cache::models_dev_load())
    else {
        return false;
    };
    value
        .get(provider)
        .and_then(|entry| entry.get("models"))
        .and_then(Value::as_object)
        .is_some_and(|models| models.contains_key(model))
}

fn gateway_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join("model_gateway.json")
}

fn load_object(path: &Path) -> Result<Value, SaveModelNameError> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value @ Value::Object(_)) => Ok(value),
            _ => Err(SaveModelNameError::Store),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(json!({})),
        Err(_) => Err(SaveModelNameError::Store),
    }
}

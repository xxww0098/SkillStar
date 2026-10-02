//! Display names stored in `model_gateway.json`.
//!
//! The object is `model_names`. A key is `provider/model`. The value is the
//! name the picker shows. The models.dev cache is not opened for writing, and
//! the translator does not read this object. The file is opened only through
//! [`ModelGatewayDoc`] (see `store::doc`).

use std::collections::BTreeMap;

use super::doc::ModelGatewayDoc;

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
    ModelGatewayDoc::open_lenient().model_names().clone()
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
    let mut doc = ModelGatewayDoc::open().map_err(|_| SaveModelNameError::Store)?;
    write_model_name(&mut doc, id, name);
    doc.save().map_err(|_| SaveModelNameError::Store)
}

/// The display-name write lens: one `model_names` entry.
pub(crate) fn write_model_name(doc: &mut ModelGatewayDoc, id: &str, name: &str) {
    doc.model_names_mut().insert(id.to_string(), name.to_string());
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
    crate::catalog::serves(provider, model)
}

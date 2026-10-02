//! Listen mode stored in `model_gateway.json`.
//!
//! The field is `listen`. `lan` asks `serve` to bind every interface on the
//! port it already resolved. Any other value, or a missing file, stays on the
//! address from the environment. Switching to `lan` also makes sure the
//! install-level gateway key exists (see `access`). This module does not bind
//! a socket and does not choose the URL written into agent files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use skillstar_core::infra::fs_ops::atomic_write;

/// The gateway file could not be replaced, or the mode is not one of the two.
/// `Key` means `lan` was asked for while the install-level gateway key could
/// not be loaded or created — LAN listening is refused before any write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveListenError {
    Store,
    Key,
}

impl std::fmt::Display for SaveListenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store => f.write_str("listen_store"),
            Self::Key => f.write_str("listen_key"),
        }
    }
}

/// `lan` when the file asks for every interface. A missing file is loopback.
pub fn listen_is_lan() -> bool {
    let Ok(bytes) = fs::read(gateway_path()) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    value
        .get("listen")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode.trim() == "lan")
}

/// `lan` or `loopback`. A missing file is `loopback`.
pub fn listen_label() -> &'static str {
    if listen_is_lan() { "lan" } else { "loopback" }
}

/// Write `lan` or remove the field for `loopback`. Other keys stay.
///
/// Switching to `lan` also loads or lazily creates the install-level gateway
/// key first: without a usable key the gateway would refuse to serve anyway,
/// so the write is rejected with [`SaveListenError::Key`] and the file is left
/// alone.
pub fn save_listen(mode: &str) -> Result<(), SaveListenError> {
    let lan = match mode.trim() {
        "lan" => true,
        "loopback" => false,
        _ => return Err(SaveListenError::Store),
    };
    if lan {
        crate::access::gateway_key().map_err(|_| SaveListenError::Key)?;
    }
    let path = gateway_path();
    let mut doc = load_object(&path)?;
    let object = doc.as_object_mut().ok_or(SaveListenError::Store)?;
    if lan {
        object.insert("listen".to_string(), json!("lan"));
    } else {
        object.remove("listen");
    }
    let bytes = serde_json::to_vec_pretty(&doc).map_err(|_| SaveListenError::Store)?;
    atomic_write(&path, &bytes).map_err(|_| SaveListenError::Store)
}

fn gateway_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join("model_gateway.json")
}

fn load_object(path: &Path) -> Result<Value, SaveListenError> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value @ Value::Object(_)) => Ok(value),
            _ => Err(SaveListenError::Store),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(json!({})),
        Err(_) => Err(SaveListenError::Store),
    }
}

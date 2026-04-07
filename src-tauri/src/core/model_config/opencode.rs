//! OpenCode configuration management.
//!
//! Config file: `~/.config/opencode/opencode.json`
//! Key fields under `provider`:
//!   - Each provider has: npm, options (apiKey, baseURL, ...), models

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::PathBuf;

use super::{read_json_file, write_json_file};

/// Returns the OpenCode config directory.
/// On macOS/Linux: `~/.config/opencode/`
/// On Windows: `%APPDATA%/opencode/`
pub fn config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        super::config_dir().join("opencode")
    }
    #[cfg(not(target_os = "windows"))]
    {
        super::home_dir().join(".config").join("opencode")
    }
}

/// Returns the path to `opencode.json`
pub fn config_path() -> PathBuf {
    config_dir().join("opencode.json")
}

/// Read the full opencode.json as a JSON Value.
pub fn read_config() -> Result<Value> {
    read_json_file(&config_path()).context("Failed to read OpenCode opencode.json")
}

/// Write the full opencode.json atomically.
pub fn write_config(config: &Value) -> Result<()> {
    write_json_file(&config_path(), config).context("Failed to write OpenCode opencode.json")
}

/// Set a single field in opencode.json, preserving all others.
/// Supports dot-separated key paths (e.g. "permission.edit").
/// If `value` is null, the key is removed.
pub fn set_field(key: &str, value: Value) -> Result<()> {
    use serde_json::Map;

    let path = config_path();
    let mut config = read_json_file(&path).unwrap_or(Value::Object(Map::new()));
    if !config.is_object() {
        config = Value::Object(Map::new());
    }

    let parts: Vec<&str> = key.split('.').collect();
    match parts.len() {
        1 => {
            let obj = config.as_object_mut().unwrap();
            if value.is_null() {
                obj.remove(parts[0]);
            } else {
                obj.insert(parts[0].to_string(), value);
            }
        }
        2 => {
            let obj = config.as_object_mut().unwrap();
            let parent = obj.entry(parts[0]).or_insert(Value::Object(Map::new()));
            if let Some(parent_obj) = parent.as_object_mut() {
                if value.is_null() {
                    parent_obj.remove(parts[1]);
                } else {
                    parent_obj.insert(parts[1].to_string(), value);
                }
            }
        }
        _ => anyhow::bail!("Key path too deep: {key}"),
    }

    write_json_file(&path, &config)
}

/// Check if the OpenCode config file exists.
pub fn config_exists() -> bool {
    config_path().exists()
}

/// Get the resolved config path as a string (for frontend display).
pub fn config_path_string() -> String {
    config_path().to_string_lossy().to_string()
}

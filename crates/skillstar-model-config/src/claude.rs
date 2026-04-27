//! Claude Code configuration management.
//!
//! Config file: `~/.claude/settings.json`
//! Key fields live under `env`:
//!   - ANTHROPIC_AUTH_TOKEN / ANTHROPIC_API_KEY
//!   - ANTHROPIC_BASE_URL
//!   - ANTHROPIC_MODEL
//!   - ANTHROPIC_DEFAULT_HAIKU_MODEL
//!   - ANTHROPIC_DEFAULT_SONNET_MODEL
//!   - ANTHROPIC_DEFAULT_OPUS_MODEL

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::path::PathBuf;

use crate::{read_json_file, write_json_file};

/// Returns the Claude config directory: `~/.claude/`
pub fn config_dir() -> PathBuf {
    crate::home_dir().join(".claude")
}

/// Returns the path to `~/.claude/settings.json`
pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Read the full settings.json as a JSON Value.
/// Returns `Value::Null` if the file does not exist.
pub fn read_settings() -> Result<Value> {
    read_json_file(&settings_path()).context("Failed to read Claude settings.json")
}

/// Write the full settings.json atomically.
pub fn write_settings(config: &Value) -> Result<()> {
    write_json_file(&settings_path(), config).context("Failed to write Claude settings.json")
}

/// Extract the `env` object from settings.json.
/// Returns an empty map if `env` is missing.
#[allow(dead_code)]
pub fn get_env(settings: &Value) -> Map<String, Value> {
    settings
        .get("env")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

/// Update the `env` object in settings.json, preserving all other fields.
#[allow(dead_code)]
pub fn set_env(env: Map<String, Value>) -> Result<()> {
    let path = settings_path();
    let mut settings = read_json_file(&path).unwrap_or(Value::Object(Map::new()));

    if !settings.is_object() {
        settings = Value::Object(Map::new());
    }

    settings
        .as_object_mut()
        .unwrap()
        .insert("env".to_string(), Value::Object(env));

    write_json_file(&path, &settings)
}

/// Set a single top-level field in settings.json, preserving all others.
/// If `value` is null, the key is removed.
pub fn set_field(key: &str, value: Value) -> Result<()> {
    let path = settings_path();
    let mut settings = read_json_file(&path).unwrap_or(Value::Object(Map::new()));
    if !settings.is_object() {
        settings = Value::Object(Map::new());
    }
    let obj = settings.as_object_mut().unwrap();
    if value.is_null() {
        obj.remove(key);
    } else {
        obj.insert(key.to_string(), value);
    }
    write_json_file(&path, &settings)
}

/// Get a single top-level field from settings.json.
/// Returns `Value::Null` if the key is missing.
#[allow(dead_code)]
pub fn get_field(key: &str) -> Result<Value> {
    let settings = read_settings()?;
    Ok(settings.get(key).cloned().unwrap_or(Value::Null))
}

/// Check if the Claude config file exists.
pub fn config_exists() -> bool {
    settings_path().exists()
}

/// Get the resolved config path as a string (for frontend display).
pub fn config_path_string() -> String {
    settings_path().to_string_lossy().to_string()
}

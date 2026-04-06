pub mod claude;
pub mod codex;
pub mod codex_accounts;
pub mod codex_oauth;
pub mod opencode;
pub mod providers;
pub mod speedtest;

use std::path::Path;

/// Atomic write: write to a temporary file then rename to the target path.
/// This prevents partial/corrupt writes if the process is interrupted.
pub fn atomic_write(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    let temp_path = path.with_extension("tmp");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&temp_path, content)?;
    std::fs::rename(&temp_path, path)?;
    Ok(())
}

/// Read a JSON file and return its contents as a serde_json::Value.
/// Returns Ok(Value::Null) if the file does not exist.
pub fn read_json_file(path: &Path) -> anyhow::Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::Value::Null);
    }
    let content = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;
    Ok(value)
}

/// Write a serde_json::Value to a file atomically with pretty formatting.
pub fn write_json_file(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(value)?;
    atomic_write(path, content.as_bytes())
}

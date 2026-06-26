//! Per-tool config paths, installed detection, and live config readers/writers.

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

use crate::tool_sync::{
    create_rolling_backup, resolve_claude_desktop_config_path, resolve_codex_config_path,
    resolve_opencode_config_path, resolve_zcode_config_path,
};

/// ZCode desktop loads MCP from `~/.zcode/cli/config.json` (`mcp.servers`), not `v2/config.json`.
pub fn resolve_zcode_cli_mcp_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".zcode").join("cli").join("config.json"))
}

use super::*;

/// `~/.claude.json` — where Claude Code reads user-scope MCP servers.
pub fn resolve_claude_json_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".claude.json"))
}

/// `~/.gemini/settings.json`
pub fn resolve_gemini_settings_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".gemini").join("settings.json"))
}

/// `~/.grok/config.toml`
pub fn resolve_grok_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".grok").join("config.toml"))
}

/// Resolve the live MCP config file for a tool.
pub fn resolve_mcp_config_path(tool_id: &str) -> Result<PathBuf> {
    match tool_id {
        "claude-code" => resolve_claude_json_path(),
        "claude-desktop" => resolve_claude_desktop_config_path(),
        "codex" => resolve_codex_config_path(),
        "gemini" => resolve_gemini_settings_path(),
        "grok" => resolve_grok_config_path(),
        "opencode" => resolve_opencode_config_path(),
        // ZCode desktop reads `mcp.servers` from ~/.zcode/cli/config.json (see zcode_cli_*).
        "zcode" => resolve_zcode_cli_mcp_config_path(),
        _ => bail!("Unsupported tool '{tool_id}'"),
    }
}

/// Best-effort "is this tool installed?" probe used to skip pointless writes.
pub fn tool_installed(tool_id: &str) -> bool {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    match tool_id {
        "claude-code" => home.join(".claude").exists() || home.join(".claude.json").exists(),
        "claude-desktop" => resolve_claude_desktop_config_path()
            .map(|p| p.exists() || p.parent().map(|d| d.exists()).unwrap_or(false))
            .unwrap_or(false),
        "codex" => home.join(".codex").exists(),
        "gemini" => home.join(".gemini").exists(),
        "grok" => home.join(".grok").exists(),
        "opencode" => {
            home.join(".config").join("opencode").exists()
                || resolve_opencode_config_path()
                    .map(|p| p.exists())
                    .unwrap_or(false)
        }
        "zcode" => home.join(".zcode").exists(),
        _ => false,
    }
}

/// Count MCP servers currently present in a tool's live config file.
fn count_live_servers(tool_id: &str) -> usize {
    let path = match resolve_mcp_config_path(tool_id) {
        Ok(p) => p,
        Err(_) => return 0,
    };
    if !path.exists() {
        return 0;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    match tool_id {
        "codex" | "grok" => toml::from_str::<toml::Table>(&content)
            .ok()
            .and_then(|t| {
                t.get("mcp_servers")
                    .and_then(|v| v.as_table())
                    .map(|m| m.len())
            })
            .unwrap_or(0),
        "opencode" => serde_json::from_str::<Value>(&content)
            .ok()
            .and_then(|v| v.get("mcp").and_then(|m| m.as_object()).map(|m| m.len()))
            .unwrap_or(0),
        "zcode" => serde_json::from_str::<Value>(&content)
            .ok()
            .and_then(|v| {
                v.get("mcp")
                    .and_then(|m| m.get("servers"))
                    .and_then(|s| s.as_object())
                    .map(|m| m.len())
            })
            .unwrap_or(0),
        // claude-code, claude-desktop, gemini all use top-level `mcpServers`.
        _ => serde_json::from_str::<Value>(&content)
            .ok()
            .and_then(|v| {
                v.get("mcpServers")
                    .and_then(|m| m.as_object())
                    .map(|m| m.len())
            })
            .unwrap_or(0),
    }
}

/// Status of every supported tool's MCP target.
pub fn tool_statuses() -> Vec<McpToolStatus> {
    MCP_TOOL_IDS
        .iter()
        .map(|&tool_id| {
            let config_path = resolve_mcp_config_path(tool_id)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            McpToolStatus {
                tool_id: tool_id.to_string(),
                label: mcp_tool_label(tool_id).to_string(),
                config_path,
                installed: tool_installed(tool_id),
                server_count: count_live_servers(tool_id),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Live config writers
// ---------------------------------------------------------------------------

pub(crate) fn backup_if_exists(path: &Path) -> Result<Option<PathBuf>> {
    if path.exists() {
        Ok(Some(create_rolling_backup(path)?))
    } else {
        Ok(None)
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    Ok(())
}

/// Read a JSON file as an object map, tolerating absence/garbage.
fn read_json_object(path: &Path) -> Map<String, Value> {
    if !path.exists() {
        return Map::new();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|c| serde_json::from_str::<Value>(&c).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
}

/// Upsert `mcpServers.<name>` in a JSON config file (claude-code/desktop/gemini).
pub(crate) fn json_mcpservers_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
    let mut root = read_json_object(path);
    let servers = root
        .entry("mcpServers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(map) = servers.as_object_mut() {
        map.insert(name.to_string(), spec);
    } else {
        let mut map = Map::new();
        map.insert(name.to_string(), spec);
        root.insert("mcpServers".to_string(), Value::Object(map));
    }
    write_json_pretty(path, &Value::Object(root))
}

/// Remove `mcpServers.<name>` from a JSON config file.
pub(crate) fn json_mcpservers_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_object(path);
    if let Some(map) = root.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        map.remove(name);
    }
    write_json_pretty(path, &Value::Object(root))
}

fn write_json_pretty(path: &Path, value: &Value) -> Result<()> {
    ensure_parent(path)?;
    let out = serde_json::to_string_pretty(value).context("Failed to serialize JSON config")?;
    std::fs::write(path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Upsert `mcp.<name>` in opencode.json (preserves `$schema`).
pub(crate) fn opencode_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
    let mut root = read_json_object(path);
    root.entry("$schema".to_string())
        .or_insert_with(|| json!("https://opencode.ai/config.json"));
    let mcp = root
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(map) = mcp.as_object_mut() {
        map.insert(name.to_string(), spec);
    }
    write_json_pretty(path, &Value::Object(root))
}

pub(crate) fn opencode_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_object(path);
    if let Some(map) = root.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        map.remove(name);
    }
    write_json_pretty(path, &Value::Object(root))
}

/// Upsert `[mcp_servers.<name>]` in Codex config.toml.
pub(crate) fn codex_upsert(path: &Path, name: &str, table: toml::Table) -> Result<()> {
    let mut root: toml::Table = if path.exists() {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|c| toml::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        toml::Table::new()
    };
    let mcp_servers = root
        .entry("mcp_servers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let Some(map) = mcp_servers.as_table_mut() {
        map.insert(name.to_string(), toml::Value::Table(table));
    }
    write_toml_pretty(path, &root)
}

pub(crate) fn codex_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root: toml::Table = match std::fs::read_to_string(path)
        .ok()
        .and_then(|c| toml::from_str(&c).ok())
    {
        Some(t) => t,
        None => return Ok(()),
    };
    if let Some(map) = root.get_mut("mcp_servers").and_then(|v| v.as_table_mut()) {
        map.remove(name);
        if map.is_empty() {
            root.remove("mcp_servers");
        }
    }
    write_toml_pretty(path, &root)
}



fn ensure_mcp_servers_map(root: &mut Map<String, Value>) -> Result<Map<String, Value>> {
    let mcp_val = root
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    // Only ever create the entry when it's missing. If the user wrote a
    // non-object `mcp` (e.g. `"mcp": true` or an array), refuse instead of
    // panicking mid-write and clobbering their hand-edited value.
    let kind = match mcp_val {
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        _ => "scalar",
    };
    let mcp_obj = mcp_val
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("`mcp` field must be a JSON object, but existing value is {kind}; refusing to overwrite the user's hand-edited config"))?;
    mcp_obj
        .entry("servers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    Ok(mcp_obj
        .get("servers")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default())
}

fn write_mcp_servers_map(root: &mut Map<String, Value>, servers: Map<String, Value>) {
    let mcp_val = root
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(mcp_obj) = mcp_val.as_object_mut() {
        mcp_obj.insert("servers".to_string(), Value::Object(servers));
    }
}

/// Upsert `mcp.servers.<name>` in `~/.zcode/cli/config.json`.
pub(crate) fn zcode_cli_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
    let mut root = read_json_object(path);
    let mut servers = ensure_mcp_servers_map(&mut root)?;
    servers.insert(name.to_string(), spec);
    write_mcp_servers_map(&mut root, servers);
    write_json_pretty(path, &Value::Object(root))
}

pub(crate) fn zcode_cli_remove(path: &Path, name: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_json_object(path);
    if let Some(mcp) = root.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        if let Some(servers) = mcp.get_mut("servers").and_then(|v| v.as_object_mut()) {
            servers.remove(name);
        }
    }
    write_json_pretty(path, &Value::Object(root))
}

/// Best-effort: drop a stale OpenCode-style entry from `~/.zcode/v2/config.json` `mcp`.
pub(crate) fn zcode_v2_opencode_mcp_remove(name: &str) -> Result<()> {
    let path = resolve_zcode_config_path()?;
    if !path.exists() {
        return Ok(());
    }
    opencode_remove(&path, name)
}

fn write_toml_pretty(path: &Path, table: &toml::Table) -> Result<()> {
    ensure_parent(path)?;
    let out = toml::to_string_pretty(table).context("Failed to serialize TOML config")?;
    std::fs::write(path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

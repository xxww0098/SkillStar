//! Import servers from a tool's live config into the unified store.

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

use super::*;

/// Parse a community `mcpServers` JSON spec into store fields.
pub(crate) fn entry_from_json_spec(name: &str, spec: &Value) -> Option<McpServerEntry> {
    let obj = spec.as_object()?;
    let transport = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("stdio")
        .to_string();
    let mut entry = blank_entry(name, &transport);
    match transport.as_str() {
        "http" | "sse" => {
            entry.url = obj.get("url").and_then(|v| v.as_str()).map(String::from);
            entry.headers = obj
                .get("headers")
                .and_then(|v| v.as_object())
                .map(json_str_map)
                .unwrap_or_default();
            entry.url.as_ref()?; // require url
        }
        _ => {
            entry.command = obj
                .get("command")
                .and_then(|v| v.as_str())
                .map(String::from);
            entry.args = obj
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            entry.env = obj
                .get("env")
                .and_then(|v| v.as_object())
                .map(json_str_map)
                .unwrap_or_default();
            entry.cwd = obj.get("cwd").and_then(|v| v.as_str()).map(String::from);
            entry.command.as_ref()?; // require command
        }
    }
    Some(entry)
}

pub(crate) fn blank_entry(name: &str, transport: &str) -> McpServerEntry {
    McpServerEntry {
        id: String::new(),
        name: name.to_string(),
        transport: transport.to_string(),
        command: None,
        args: Vec::new(),
        env: BTreeMap::new(),
        cwd: None,
        url: None,
        headers: BTreeMap::new(),
        description: None,
        homepage: None,
        tags: Vec::new(),
        enabled: BTreeMap::new(),
        sort_index: 0,
        created_at: None,
        updated_at: None,
    }
}

fn json_str_map(m: &Map<String, Value>) -> BTreeMap<String, String> {
    m.iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect()
}

/// Read servers from a tool's live config into store entries (name → entry).
pub fn read_servers_from_tool(tool_id: &str) -> Result<Vec<McpServerEntry>> {
    let path = resolve_mcp_config_path(tool_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let mut out = Vec::new();
    match tool_id {
        "codex" => {
            let root: toml::Table = toml::from_str(&content)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if let Some(servers) = root.get("mcp_servers").and_then(|v| v.as_table()) {
                for (name, val) in servers {
                    if let Some(tbl) = val.as_table()
                        && let Some(e) = entry_from_codex_table(name, tbl)
                    {
                        out.push(e);
                    }
                }
            }
        }
        "opencode" => {
            let root: Value = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if let Some(map) = root.get("mcp").and_then(|v| v.as_object()) {
                for (name, val) in map {
                    if let Some(e) = entry_from_opencode_spec(name, val) {
                        out.push(e);
                    }
                }
            }
        }
        "zcode" => {
            let root: Value = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if let Some(map) = root
                .get("mcp")
                .and_then(|m| m.get("servers"))
                .and_then(|v| v.as_object())
            {
                for (name, val) in map {
                    if let Some(e) = entry_from_json_spec(name, val) {
                        out.push(e);
                    }
                }
            }
        }
        _ => {
            let root: Value = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if let Some(map) = root.get("mcpServers").and_then(|v| v.as_object()) {
                for (name, val) in map {
                    if let Some(e) = entry_from_json_spec(name, val) {
                        out.push(e);
                    }
                }
            }
        }
    }
    Ok(out)
}

fn entry_from_codex_table(name: &str, tbl: &toml::Table) -> Option<McpServerEntry> {
    let transport = tbl
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("stdio")
        .to_string();
    let mut entry = blank_entry(name, &transport);
    match transport.as_str() {
        "http" | "sse" => {
            entry.url = tbl.get("url").and_then(|v| v.as_str()).map(String::from);
            let headers = tbl
                .get("http_headers")
                .and_then(|v| v.as_table())
                .or_else(|| tbl.get("headers").and_then(|v| v.as_table()));
            if let Some(h) = headers {
                entry.headers = h
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();
            }
            entry.url.as_ref()?;
        }
        _ => {
            entry.command = tbl
                .get("command")
                .and_then(|v| v.as_str())
                .map(String::from);
            entry.args = tbl
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            entry.cwd = tbl.get("cwd").and_then(|v| v.as_str()).map(String::from);
            if let Some(env) = tbl.get("env").and_then(|v| v.as_table()) {
                entry.env = env
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();
            }
            entry.command.as_ref()?;
        }
    }
    Some(entry)
}

pub(crate) fn entry_from_opencode_spec(name: &str, spec: &Value) -> Option<McpServerEntry> {
    let obj = spec.as_object()?;
    let typ = obj.get("type").and_then(|v| v.as_str()).unwrap_or("local");
    match typ {
        "remote" => {
            let mut entry = blank_entry(name, "sse");
            entry.url = obj.get("url").and_then(|v| v.as_str()).map(String::from);
            entry.headers = obj
                .get("headers")
                .and_then(|v| v.as_object())
                .map(json_str_map)
                .unwrap_or_default();
            entry.url.as_ref()?;
            Some(entry)
        }
        _ => {
            let mut entry = blank_entry(name, "stdio");
            if let Some(arr) = obj.get("command").and_then(|v| v.as_array()) {
                let parts: Vec<String> = arr
                    .iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect();
                if let Some((first, rest)) = parts.split_first() {
                    entry.command = Some(first.clone());
                    entry.args = rest.to_vec();
                }
            }
            entry.env = obj
                .get("environment")
                .and_then(|v| v.as_object())
                .map(json_str_map)
                .unwrap_or_default();
            entry.command.as_ref()?;
            Some(entry)
        }
    }
}

/// Import servers from a tool into the store. New names are added (enabled for
/// that tool); existing names just get the tool's enable flag set true.
/// Returns the number of servers added or newly enabled.
pub fn import_from_tool(store: &mut McpStore, tool_id: &str) -> Result<usize> {
    if !is_supported_tool(tool_id) {
        bail!("Unsupported tool '{tool_id}'");
    }
    let discovered = read_servers_from_tool(tool_id)?;
    let mut changed = 0usize;
    for mut found in discovered {
        if let Some(existing) = store.servers.iter_mut().find(|s| s.name == found.name) {
            if existing.enabled.get(tool_id).copied() != Some(true) {
                existing.enabled.insert(tool_id.to_string(), true);
                existing.updated_at = Some(now_ms());
                changed += 1;
            }
        } else {
            found.id = Uuid::new_v4().to_string();
            let now = now_ms();
            found.created_at = Some(now);
            found.updated_at = Some(now);
            found.sort_index = store
                .servers
                .iter()
                .map(|s| s.sort_index)
                .max()
                .map_or(0, |m| m + 1);
            found.enabled.insert(tool_id.to_string(), true);
            store.servers.push(found);
            changed += 1;
        }
    }
    Ok(changed)
}

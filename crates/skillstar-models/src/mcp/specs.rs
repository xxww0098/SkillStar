//! Per-tool wire-format spec generation (canonical JSON, Claude Desktop, OpenCode, Codex TOML).

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use super::*;

/// Canonical "community" mcpServers value (used by Claude Code & Gemini).
///
/// stdio keeps `type` (modern Claude Code / Gemini accept it); http/sse carry
/// `url` and optional `headers`.
pub(crate) fn canonical_spec(entry: &McpServerEntry) -> Value {
    let mut obj = Map::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            obj.insert("type".into(), json!(entry.transport));
            if let Some(url) = &entry.url {
                obj.insert("url".into(), json!(url));
            }
            if !entry.headers.is_empty() {
                obj.insert("headers".into(), json!(string_map(&entry.headers)));
            }
        }
        _ => {
            obj.insert("type".into(), json!("stdio"));
            if let Some(cmd) = &entry.command {
                obj.insert("command".into(), json!(cmd));
            }
            if !entry.args.is_empty() {
                obj.insert("args".into(), json!(entry.args));
            }
            if !entry.env.is_empty() {
                obj.insert("env".into(), json!(string_map(&entry.env)));
            }
            if let Some(cwd) = &entry.cwd {
                obj.insert("cwd".into(), json!(cwd));
            }
        }
    }
    Value::Object(obj)
}

/// Claude Desktop value: stdio only, no `type` key.
pub(crate) fn claude_desktop_spec(entry: &McpServerEntry) -> Result<Value> {
    if entry.transport != "stdio" {
        bail!(
            "Claude Desktop only supports stdio MCP servers (server '{}' is {})",
            entry.name,
            entry.transport
        );
    }
    let mut obj = Map::new();
    if let Some(cmd) = &entry.command {
        obj.insert("command".into(), json!(cmd));
    }
    if !entry.args.is_empty() {
        obj.insert("args".into(), json!(entry.args));
    }
    if !entry.env.is_empty() {
        obj.insert("env".into(), json!(string_map(&entry.env)));
    }
    Ok(Value::Object(obj))
}

/// OpenCode value: stdio→`local` (command array, `environment`), http/sse→`remote`.
pub(crate) fn opencode_spec(entry: &McpServerEntry) -> Value {
    let mut obj = Map::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            obj.insert("type".into(), json!("remote"));
            if let Some(url) = &entry.url {
                obj.insert("url".into(), json!(url));
            }
            if !entry.headers.is_empty() {
                obj.insert("headers".into(), json!(string_map(&entry.headers)));
            }
            obj.insert("enabled".into(), json!(true));
        }
        _ => {
            obj.insert("type".into(), json!("local"));
            let mut command_arr: Vec<Value> = Vec::new();
            command_arr.push(json!(entry.command.clone().unwrap_or_default()));
            for a in &entry.args {
                command_arr.push(json!(a));
            }
            obj.insert("command".into(), Value::Array(command_arr));
            if !entry.env.is_empty() {
                obj.insert("environment".into(), json!(string_map(&entry.env)));
            }
            obj.insert("enabled".into(), json!(true));
        }
    }
    Value::Object(obj)
}

/// Codex `[mcp_servers.<name>]` TOML table.
pub(crate) fn codex_toml_table(entry: &McpServerEntry) -> toml::Table {
    let mut t = toml::Table::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            t.insert("type".into(), toml::Value::String(entry.transport.clone()));
            if let Some(url) = &entry.url {
                t.insert("url".into(), toml::Value::String(url.clone()));
            }
            if !entry.headers.is_empty() {
                t.insert("http_headers".into(), toml::Value::Table(toml_string_table(&entry.headers)));
            }
        }
        _ => {
            t.insert("type".into(), toml::Value::String("stdio".into()));
            if let Some(cmd) = &entry.command {
                t.insert("command".into(), toml::Value::String(cmd.clone()));
            }
            if !entry.args.is_empty() {
                let arr: Vec<toml::Value> =
                    entry.args.iter().map(|a| toml::Value::String(a.clone())).collect();
                t.insert("args".into(), toml::Value::Array(arr));
            }
            if let Some(cwd) = &entry.cwd {
                t.insert("cwd".into(), toml::Value::String(cwd.clone()));
            }
            if !entry.env.is_empty() {
                t.insert("env".into(), toml::Value::Table(toml_string_table(&entry.env)));
            }
        }
    }
    t
}

fn string_map(m: &BTreeMap<String, String>) -> Map<String, Value> {
    m.iter().map(|(k, v)| (k.clone(), json!(v))).collect()
}

fn toml_string_table(m: &BTreeMap<String, String>) -> toml::Table {
    m.iter()
        .map(|(k, v)| (k.clone(), toml::Value::String(v.clone())))
        .collect()
}

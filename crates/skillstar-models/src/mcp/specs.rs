//! Per-tool wire-format spec generation (canonical JSON, OpenCode, Codex TOML).

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

use super::*;

/// Canonical "community" mcpServers value (base shape shared by Claude Code,
/// Kiro, Cursor, and ZCode). stdio keeps `type`; http/sse carry `url` and
/// optional `headers`. Does **not** include any tool-specific
/// approval/exposure/timeout fields — callers layer those on top per tool
/// (see [`claude_code_spec`] and [`kiro_spec`]).
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

/// Claude Code value (`~/.claude.json` `mcpServers.<name>`): canonical shape
/// only. Claude Code has no verified native per-server auto-approve,
/// disabled-tools, or timeout field, so none of the approval/exposure config
/// is projected here.
pub(crate) fn claude_code_spec(entry: &McpServerEntry) -> Value {
    canonical_spec(entry)
}

/// Cursor value (`~/.cursor/mcp.json` `mcpServers.<name>`): canonical shape
/// only. Cursor has no verified native per-server auto-approve,
/// disabled-tools, or timeout field, so none of the approval/exposure config
/// is projected here.
pub(crate) fn cursor_spec(entry: &McpServerEntry) -> Value {
    canonical_spec(entry)
}

/// Kiro value (`~/.kiro/settings/mcp.json` `mcpServers.<name>`): canonical
/// shape plus `autoApprove` and `disabledTools` — the exact fields documented
/// at kiro.dev/docs/cli/mcp/configuration. `timeout_ms` has no documented Kiro
/// field and is not projected.
pub(crate) fn kiro_spec(entry: &McpServerEntry) -> Value {
    let mut obj = match canonical_spec(entry) {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    if entry.auto_approve_all {
        obj.insert("autoApprove".into(), json!(["*"]));
    } else if !entry.auto_approve_tools.is_empty() {
        obj.insert("autoApprove".into(), json!(entry.auto_approve_tools));
    }
    if !entry.disabled_tools.is_empty() {
        obj.insert("disabledTools".into(), json!(entry.disabled_tools));
    }
    Value::Object(obj)
}

/// OpenCode value: stdio→`local` (command array, `environment`), http/sse→`remote`.
/// Also carries `timeout` (ms) — OpenCode's own documented field. OpenCode has
/// no per-server auto-approve/disabled-tools field (tool exposure is
/// controlled globally via the `tools`/`agent` glob config, out of scope for a
/// single server entry), so those are not projected.
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
    if let Some(ms) = entry.timeout_ms.filter(|&ms| ms > 0) {
        obj.insert("timeout".into(), json!(ms));
    }
    Value::Object(obj)
}

/// ZCode desktop agent MCP (`~/.zcode/cli/config.json` → `mcp.servers.<name>`).
/// Uses the same community stdio / http shape as Claude Code (`command` + `args` + `env`),
/// not the OpenCode `local`/`remote` form under `v2/config.json`.
pub(crate) fn zcode_cli_spec(entry: &McpServerEntry) -> Value {
    canonical_spec(entry)
}

/// Grok `[mcp_servers.<name>]` TOML table (`~/.grok/config.toml`).
pub(crate) fn grok_toml_table(entry: &McpServerEntry) -> toml::Table {
    let mut t = toml::Table::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            if let Some(url) = &entry.url {
                t.insert("url".into(), toml::Value::String(url.clone()));
            }
            if !entry.headers.is_empty() {
                t.insert(
                    "headers".into(),
                    toml::Value::Table(toml_string_table(&entry.headers)),
                );
            }
        }
        _ => {
            if let Some(cmd) = &entry.command {
                t.insert("command".into(), toml::Value::String(cmd.clone()));
            }
            if !entry.args.is_empty() {
                let arr: Vec<toml::Value> = entry
                    .args
                    .iter()
                    .map(|a| toml::Value::String(a.clone()))
                    .collect();
                t.insert("args".into(), toml::Value::Array(arr));
            }
            if let Some(cwd) = &entry.cwd {
                t.insert("cwd".into(), toml::Value::String(cwd.clone()));
            }
            if !entry.env.is_empty() {
                t.insert(
                    "env".into(),
                    toml::Value::Table(toml_string_table(&entry.env)),
                );
            }
        }
    }
    t
}

/// Codex `[mcp_servers.<name>]` TOML table. Also carries `disabled_tools`
/// (blacklist) and `tool_timeout_sec` — both documented Codex fields.
/// `auto_approve_*` has no Codex per-server equivalent (approval is
/// controlled by the separate `[apps.<name>]` connector config) and is not
/// projected.
pub(crate) fn codex_toml_table(entry: &McpServerEntry) -> toml::Table {
    let mut t = toml::Table::new();
    match entry.transport.as_str() {
        "http" | "sse" => {
            t.insert("type".into(), toml::Value::String(entry.transport.clone()));
            if let Some(url) = &entry.url {
                t.insert("url".into(), toml::Value::String(url.clone()));
            }
            if !entry.headers.is_empty() {
                t.insert(
                    "http_headers".into(),
                    toml::Value::Table(toml_string_table(&entry.headers)),
                );
            }
        }
        _ => {
            t.insert("type".into(), toml::Value::String("stdio".into()));
            if let Some(cmd) = &entry.command {
                t.insert("command".into(), toml::Value::String(cmd.clone()));
            }
            if !entry.args.is_empty() {
                let arr: Vec<toml::Value> = entry
                    .args
                    .iter()
                    .map(|a| toml::Value::String(a.clone()))
                    .collect();
                t.insert("args".into(), toml::Value::Array(arr));
            }
            if let Some(cwd) = &entry.cwd {
                t.insert("cwd".into(), toml::Value::String(cwd.clone()));
            }
            if !entry.env.is_empty() {
                t.insert(
                    "env".into(),
                    toml::Value::Table(toml_string_table(&entry.env)),
                );
            }
        }
    }
    if !entry.disabled_tools.is_empty() {
        let arr: Vec<toml::Value> = entry
            .disabled_tools
            .iter()
            .map(|d| toml::Value::String(d.clone()))
            .collect();
        t.insert("disabled_tools".into(), toml::Value::Array(arr));
    }
    if let Some(ms) = entry.timeout_ms.filter(|&ms| ms > 0) {
        let sec = (ms / 1000).max(1) as i64;
        t.insert("tool_timeout_sec".into(), toml::Value::Integer(sec));
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

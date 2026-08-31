//! Import servers from a tool's live config into the unified store.

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

use super::*;

/// Read side of [`specs::JsonDialect`] — how one client's JSON spells the
/// transport, so a round-trip through that client's config does not silently
/// change what the entry means.
///
/// Kept separate from the writer enum because the reader must be *permissive*
/// (users hand-edit these files and paste configs between clients) while the
/// writer must be exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonReadDialect {
    /// `type` decides; endpoint under `url`. Claude Code, Cursor, Kiro, ZCode,
    /// VS Code.
    Typed,
    /// Cline's camelCase `streamableHttp`; endpoint under `url`.
    ClineCamel,
    /// No `type`; endpoint under `serverUrl` (falling back to `url`, which
    /// Windsurf also accepts).
    ServerUrlNoType,
    /// No `type`; `httpUrl` means Streamable HTTP, `url` means SSE.
    GeminiUrlKeys,
    /// No `type`; a `url` means remote, otherwise stdio. Zed.
    PlainNoType,
    /// No `type`; `command` means stdio, `url` means remote. Maka records the
    /// remote transport in its own `transport` field (`streamable-http` /
    /// `sse` / `auto`).
    Maka,
}

impl JsonReadDialect {
    /// Decide `(transport, url)` from a raw spec object.
    ///
    /// Returns `None` for the transport only when the object carries no usable
    /// endpoint *and* no command — the caller then rejects the entry.
    fn transport_and_url(self, obj: &Map<String, Value>) -> (String, Option<String>) {
        let at = |key: &str| {
            obj.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };
        let declared = obj.get("type").and_then(Value::as_str);
        match self {
            Self::Typed => {
                let transport = match declared {
                    // Claude Code accepts `streamable-http` as an alias of
                    // `http`; normalize so the store keeps one vocabulary.
                    Some("http") | Some("streamable-http") | Some("streamableHttp") => "http",
                    Some("sse") => "sse",
                    _ => "stdio",
                };
                (transport.to_string(), at("url"))
            }
            Self::ClineCamel => {
                let transport = match declared {
                    Some("streamableHttp") | Some("streamable-http") | Some("http") => "http",
                    Some("sse") => "sse",
                    _ => "stdio",
                };
                (transport.to_string(), at("url"))
            }
            Self::ServerUrlNoType => {
                let url = at("serverUrl").or_else(|| at("url"));
                let transport = if url.is_some() { "http" } else { "stdio" };
                (transport.to_string(), url)
            }
            Self::GeminiUrlKeys => match at("httpUrl") {
                Some(url) => ("http".to_string(), Some(url)),
                None => match at("url") {
                    // `url` without `httpUrl` is Gemini's SSE spelling.
                    Some(url) => ("sse".to_string(), Some(url)),
                    None => ("stdio".to_string(), None),
                },
            },
            Self::PlainNoType => match at("url") {
                Some(url) => ("http".to_string(), Some(url)),
                None => ("stdio".to_string(), None),
            },
            Self::Maka => {
                // Maka prefers `command` when both keys exist (its own
                // normalizer treats that as stdio and rejects `protocol`).
                if at("command").is_some() {
                    ("stdio".to_string(), None)
                } else {
                    match at("url") {
                        Some(url) => {
                            let transport = match obj.get("transport").and_then(Value::as_str) {
                                Some("sse") => "sse",
                                _ => "http",
                            };
                            (transport.to_string(), Some(url))
                        }
                        None => ("stdio".to_string(), None),
                    }
                }
            }
        }
    }
}

/// Parse a community `mcpServers` JSON spec into store fields.
pub(crate) fn entry_from_json_spec(name: &str, spec: &Value) -> Option<McpServerEntry> {
    entry_from_json_spec_dialect(name, spec, JsonReadDialect::Typed)
}

/// Parse a JSON server spec written in `dialect` into store fields.
pub(crate) fn entry_from_json_spec_dialect(
    name: &str,
    spec: &Value,
    dialect: JsonReadDialect,
) -> Option<McpServerEntry> {
    let obj = spec.as_object()?;
    let (transport, url) = dialect.transport_and_url(obj);
    let mut entry = blank_entry(name, &transport);
    match transport.as_str() {
        "http" | "sse" => {
            entry.url = url;
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
    apply_common_approval_fields(&mut entry, obj);
    Some(entry)
}

/// Read back the approval/exposure fields any of our JSON writers may have
/// set (`autoApprove` / `disabledTools` / `trust` / `excludeTools` /
/// `timeout`), tolerating whichever subset a given tool actually wrote.
fn apply_common_approval_fields(entry: &mut McpServerEntry, obj: &Map<String, Value>) {
    if let Some(arr) = obj.get("autoApprove").and_then(Value::as_array) {
        let tools: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        if tools.iter().any(|t| t == "*") {
            entry.auto_approve_all = true;
        } else {
            entry.auto_approve_tools = tools;
        }
    }
    if obj.get("trust").and_then(Value::as_bool) == Some(true) {
        entry.auto_approve_all = true;
    }
    let disabled = obj
        .get("disabledTools")
        .or_else(|| obj.get("excludeTools"))
        .and_then(Value::as_array);
    if let Some(arr) = disabled {
        entry.disabled_tools = arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    if let Some(ms) = obj.get("timeout").and_then(Value::as_u64) {
        entry.timeout_ms = Some(ms);
    }
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
        source_id: None,
        registry_name: None,
        installed_version: None,
        // Read back out of a tool's own config, so by definition not installed
        // from a registry runtime shape SkillStar chose.
        runtime_kind: Some(McpRuntimeKind::Manual.as_str().to_string()),
        enabled: BTreeMap::new(),
        auto_approve_all: false,
        auto_approve_tools: Vec::new(),
        disabled_tools: Vec::new(),
        timeout_ms: None,
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
///
/// Registry-driven via the `read_servers` wire-format column. Unknown ids
/// keep the historical fallback of parsing a top-level `mcpServers` map.
pub fn read_servers_from_tool(tool_id: &str) -> Result<Vec<McpServerEntry>> {
    let path = resolve_mcp_config_path(tool_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let read = mcp_tool_spec(tool_id)
        .map(|spec| spec.read_servers)
        .unwrap_or(read_json_mcpservers_entries);
    read(&content).with_context(|| format!("Failed to parse {}", path.display()))
}

// ---------------------------------------------------------------------------
// Per-format readers (registry `read_servers` column)
// ---------------------------------------------------------------------------

/// TOML `mcp_servers` table (Codex, Grok).
pub(crate) fn read_toml_mcp_servers_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    let root: toml::Table = toml::from_str(content)?;
    let mut out = Vec::new();
    if let Some(servers) = root.get("mcp_servers").and_then(|v| v.as_table()) {
        for (name, val) in servers {
            if let Some(tbl) = val.as_table()
                && let Some(e) = entry_from_codex_table(name, tbl)
            {
                out.push(e);
            }
        }
    }
    Ok(out)
}

/// OpenCode-style `mcp` JSON map.
pub(crate) fn read_opencode_mcp_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    let root: Value = serde_json::from_str(content)?;
    let mut out = Vec::new();
    if let Some(map) = root.get("mcp").and_then(|v| v.as_object()) {
        for (name, val) in map {
            if let Some(e) = entry_from_opencode_spec(name, val) {
                out.push(e);
            }
        }
    }
    Ok(out)
}

/// ZCode CLI `mcp.servers` JSON map.
pub(crate) fn read_zcode_cli_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    let root: Value = serde_json::from_str(content)?;
    let mut out = Vec::new();
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
    Ok(out)
}

/// Read a top-level JSON server map under `root_key`, parsing each value in
/// `dialect`. Shared by every JSON-format target.
fn read_json_named_map_entries(
    content: &str,
    root_key: &str,
    dialect: JsonReadDialect,
) -> Result<Vec<McpServerEntry>> {
    let root: Value = serde_json::from_str(content)?;
    let mut out = Vec::new();
    if let Some(map) = root.get(root_key).and_then(|v| v.as_object()) {
        for (name, val) in map {
            if let Some(e) = entry_from_json_spec_dialect(name, val, dialect) {
                out.push(e);
            }
        }
    }
    Ok(out)
}

/// Top-level `mcpServers` JSON map (Claude Code, Kiro, Cursor).
pub(crate) fn read_json_mcpservers_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    read_json_named_map_entries(content, MCP_SERVERS_KEY, JsonReadDialect::Typed)
}

/// Claude Desktop Chat's `mcpServers` map (no `type`: a `url` means remote).
pub(crate) fn read_claude_desktop_chat_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    read_json_named_map_entries(content, MCP_SERVERS_KEY, JsonReadDialect::PlainNoType)
}

/// VS Code's top-level `servers` map.
pub(crate) fn read_vscode_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    read_json_named_map_entries(content, VSCODE_SERVERS_KEY, JsonReadDialect::Typed)
}

/// Windsurf's `mcpServers` map (`serverUrl`, no `type`).
pub(crate) fn read_windsurf_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    read_json_named_map_entries(content, MCP_SERVERS_KEY, JsonReadDialect::ServerUrlNoType)
}

/// Cline's `mcpServers` map (camelCase `streamableHttp`).
pub(crate) fn read_cline_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    read_json_named_map_entries(content, MCP_SERVERS_KEY, JsonReadDialect::ClineCamel)
}

/// Gemini CLI's `mcpServers` map (`url` = SSE, `httpUrl` = Streamable HTTP).
pub(crate) fn read_gemini_cli_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    read_json_named_map_entries(content, MCP_SERVERS_KEY, JsonReadDialect::GeminiUrlKeys)
}

/// Zed's top-level `context_servers` map.
pub(crate) fn read_zed_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    read_json_named_map_entries(content, ZED_SERVERS_KEY, JsonReadDialect::PlainNoType)
}

/// Maka's `mcpServers` map (`version: 2`, no `type`, remote `transport`).
pub(crate) fn read_maka_entries(content: &str) -> Result<Vec<McpServerEntry>> {
    read_json_named_map_entries(content, MCP_SERVERS_KEY, JsonReadDialect::Maka)
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
    if let Some(arr) = tbl.get("disabled_tools").and_then(|v| v.as_array()) {
        entry.disabled_tools = arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    let timeout_sec = tbl
        .get("tool_timeout_sec")
        .or_else(|| tbl.get("startup_timeout_sec"))
        .and_then(|v| v.as_integer());
    if let Some(sec) = timeout_sec {
        entry.timeout_ms = Some((sec.max(0) as u64) * 1000);
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
            if let Some(ms) = obj.get("timeout").and_then(Value::as_u64) {
                entry.timeout_ms = Some(ms);
            }
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
            if let Some(ms) = obj.get("timeout").and_then(Value::as_u64) {
                entry.timeout_ms = Some(ms);
            }
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

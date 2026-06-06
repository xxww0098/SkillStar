//! MCP (Model Context Protocol) server management.
//!
//! SkillStar owns a single unified MCP server store at
//! `~/.skillstar/config/mcp_servers.json` and *projects* each server into the
//! native config file of every supported agent tool. This mirrors the mature
//! design used by `cc-switch`: one source of truth, per-tool enable flags, and
//! faithful per-tool wire formats.
//!
//! ## Unified store
//!
//! Each [`McpServerEntry`] holds a transport (`stdio` / `http` / `sse`), the
//! launch spec (command/args/env or url/headers), and a per-tool `enabled` map.
//! Toggling a tool on writes the server into that tool's live config; toggling
//! off removes it. Editing a server re-projects it to all currently-enabled
//! tools.
//!
//! ## Per-tool target files & formats
//!
//! | tool_id          | file                                   | location / format |
//! |------------------|----------------------------------------|-------------------|
//! | `claude-code`    | `~/.claude.json`                       | `mcpServers.<name>` (community JSON, keeps `type`) |
//! | `claude-desktop` | `claude_desktop_config.json`           | `mcpServers.<name>` (stdio only, no `type`) |
//! | `codex`          | `~/.codex/config.toml`                 | `[mcp_servers.<name>]` TOML table |
//! | `gemini`         | `~/.gemini/settings.json`              | `mcpServers.<name>` (community JSON) |
//! | `opencode`       | `~/.config/opencode/opencode.json`     | `mcp.<name>` (`local`/`remote` form) |
//!
//! All live writes create a rolling backup (last 5) and use merge semantics:
//! only the single managed server key is touched, every other field is left
//! untouched.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::tool_sync::{
    create_rolling_backup, resolve_claude_desktop_config_path, resolve_codex_config_path,
    resolve_opencode_config_path,
};

// ---------------------------------------------------------------------------
// Supported tools
// ---------------------------------------------------------------------------

/// Tool ids that can receive MCP servers, in display order.
pub const MCP_TOOL_IDS: &[&str] = &["claude-code", "claude-desktop", "codex", "gemini", "opencode"];

/// Human-readable label for a tool id.
pub fn mcp_tool_label(tool_id: &str) -> &'static str {
    match tool_id {
        "claude-code" => "Claude Code",
        "claude-desktop" => "Claude Desktop",
        "codex" => "Codex",
        "gemini" => "Gemini CLI",
        "opencode" => "OpenCode",
        _ => "Unknown",
    }
}

fn is_supported_tool(tool_id: &str) -> bool {
    MCP_TOOL_IDS.contains(&tool_id)
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single MCP server in the unified store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    #[serde(default)]
    pub id: String,
    /// Server key written verbatim into each tool's config.
    pub name: String,
    /// `"stdio"` (default), `"http"`, or `"sse"`.
    #[serde(default = "default_transport")]
    pub transport: String,

    // --- stdio fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    // --- http / sse fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,

    // --- metadata ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Per-tool enable flags, keyed by tool id (see [`MCP_TOOL_IDS`]).
    #[serde(default)]
    pub enabled: BTreeMap<String, bool>,

    #[serde(default)]
    pub sort_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<u64>,
}

fn default_transport() -> String {
    "stdio".to_string()
}

// ---------------------------------------------------------------------------
// Built-in / recommended MCP presets
// ---------------------------------------------------------------------------

/// A built-in, recommended-to-install MCP server template.
///
/// Mirrors the `ProviderPresetFlat` pattern: the registry below is the single
/// source of truth, exposed to the UI via the `get_mcp_presets` command. The
/// UI pre-fills the create form from a preset (leaving any `required_env` keys
/// blank for the user to fill in) and then creates a normal [`McpServerEntry`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpPreset {
    pub id: String,
    /// Server key written verbatim into each tool's config (and the entry name).
    pub name: String,
    pub description: String,
    pub homepage: String,
    /// `"stdio"` (default), `"http"`, or `"sse"`.
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Env keys the user must fill in (e.g. `["API_KEY"]`) — the UI highlights these.
    #[serde(default)]
    pub required_env: Vec<String>,
}

/// Built-in recommended MCP presets — single source of truth for the UI.
pub fn get_mcp_presets() -> Vec<McpPreset> {
    vec![McpPreset {
        id: "adspower-local-api".to_string(),
        name: "adspower-local-api".to_string(),
        description: "AdsPower 浏览器 Local API — 通过 MCP 控制指纹浏览器 / 自动化".to_string(),
        homepage: "https://github.com/AdsPower/adspower-browser".to_string(),
        transport: "stdio".to_string(),
        command: Some("npx".to_string()),
        args: vec!["-y".to_string(), "local-api-mcp-typescript".to_string()],
        env: BTreeMap::from([
            ("PORT".to_string(), "50325".to_string()),
            ("API_KEY".to_string(), String::new()),
        ]),
        url: None,
        headers: BTreeMap::new(),
        tags: vec!["browser".to_string(), "automation".to_string()],
        required_env: vec!["API_KEY".to_string()],
    }]
}

/// Partial update patch for an MCP server. Only `Some` fields are applied.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Root structure stored in `mcp_servers.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStore {
    pub version: u32,
    #[serde(default)]
    pub servers: Vec<McpServerEntry>,
}

impl Default for McpStore {
    fn default() -> Self {
        Self {
            version: 1,
            servers: Vec::new(),
        }
    }
}

/// Result of projecting one server into one tool's live config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSyncResult {
    pub tool_id: String,
    pub server_id: String,
    pub success: bool,
    /// True when the action was a no-op because the tool is not installed.
    #[serde(default)]
    pub skipped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Installed/probe status for one tool's MCP config target.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolStatus {
    pub tool_id: String,
    pub label: String,
    pub config_path: String,
    pub installed: bool,
    /// Number of MCP servers currently present in the live config file.
    pub server_count: usize,
}

// ---------------------------------------------------------------------------
// Store path + IO
// ---------------------------------------------------------------------------

/// `~/.skillstar/config/mcp_servers.json`
pub fn mcp_store_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".skillstar")
        .join("config")
        .join("mcp_servers.json")
}

/// Read the store, returning an empty default on missing/malformed files.
pub fn read_mcp_store(path: &Path) -> Result<McpStore> {
    if !path.exists() {
        return Ok(McpStore::default());
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("Failed to read MCP store {}: {e}. Using default.", path.display());
            return Ok(McpStore::default());
        }
    };
    let text = text.trim_start_matches('\u{FEFF}');
    match serde_json::from_str::<McpStore>(text) {
        Ok(store) => Ok(store),
        Err(e) => {
            tracing::warn!("Malformed MCP store {}: {e}. Using default.", path.display());
            Ok(McpStore::default())
        }
    }
}

/// Write the store atomically (temp file + rename).
pub fn write_mcp_store(store: &McpStore, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(store).context("Failed to serialize McpStore")?;
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json.as_bytes())
        .with_context(|| format!("Failed to write temp file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to rename {} to {}", temp_path.display(), path.display()))?;
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate an entry's transport-specific required fields.
pub fn validate_entry(entry: &McpServerEntry) -> Result<()> {
    if entry.name.trim().is_empty() {
        bail!("MCP server name must not be empty");
    }
    match entry.transport.as_str() {
        "stdio" => {
            if entry.command.as_deref().map(str::trim).unwrap_or("").is_empty() {
                bail!("stdio MCP server '{}' requires a command", entry.name);
            }
        }
        "http" | "sse" => {
            if entry.url.as_deref().map(str::trim).unwrap_or("").is_empty() {
                bail!("{} MCP server '{}' requires a url", entry.transport, entry.name);
            }
        }
        other => bail!("Unknown MCP transport '{other}' (expected stdio|http|sse)"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Store CRUD (pure — operate on &mut McpStore)
// ---------------------------------------------------------------------------

/// Create a new server: assigns a fresh UUID, timestamps, and sort index.
pub fn create_server(store: &mut McpStore, mut entry: McpServerEntry) -> Result<McpServerEntry> {
    validate_entry(&entry)?;
    if store.servers.iter().any(|s| s.name == entry.name) {
        bail!("An MCP server named '{}' already exists", entry.name);
    }
    entry.id = Uuid::new_v4().to_string();
    let now = now_ms();
    entry.created_at = Some(now);
    entry.updated_at = Some(now);
    entry.sort_index = store.servers.iter().map(|s| s.sort_index).max().map_or(0, |m| m + 1);
    // Drop enable flags for unknown tools.
    entry.enabled.retain(|k, _| is_supported_tool(k));
    store.servers.push(entry.clone());
    Ok(entry)
}

/// Apply a partial patch to an existing server.
pub fn update_server(
    store: &mut McpStore,
    id: &str,
    patch: McpServerPatch,
) -> Result<McpServerEntry> {
    // Guard against renaming onto another server's name.
    if let Some(new_name) = patch.name.as_ref()
        && store.servers.iter().any(|s| s.id != id && &s.name == new_name) {
            bail!("An MCP server named '{}' already exists", new_name);
        }
    let server = store
        .servers
        .iter_mut()
        .find(|s| s.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;

    if let Some(v) = patch.name {
        server.name = v;
    }
    if let Some(v) = patch.transport {
        server.transport = v;
    }
    if let Some(v) = patch.command {
        server.command = Some(v);
    }
    if let Some(v) = patch.args {
        server.args = v;
    }
    if let Some(v) = patch.env {
        server.env = v;
    }
    if let Some(v) = patch.cwd {
        server.cwd = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = patch.url {
        server.url = Some(v);
    }
    if let Some(v) = patch.headers {
        server.headers = v;
    }
    if let Some(v) = patch.description {
        server.description = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = patch.homepage {
        server.homepage = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = patch.tags {
        server.tags = v;
    }
    server.updated_at = Some(now_ms());

    let updated = server.clone();
    validate_entry(&updated)?;
    Ok(updated)
}

/// Remove a server from the store. Returns the removed entry.
pub fn delete_server(store: &mut McpStore, id: &str) -> Result<McpServerEntry> {
    let idx = store
        .servers
        .iter()
        .position(|s| s.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;
    Ok(store.servers.remove(idx))
}

/// Set the enabled flag for a server on a tool. Returns the updated entry.
pub fn set_tool_enabled(
    store: &mut McpStore,
    id: &str,
    tool_id: &str,
    enabled: bool,
) -> Result<McpServerEntry> {
    if !is_supported_tool(tool_id) {
        bail!("Unsupported tool '{tool_id}'");
    }
    let server = store
        .servers
        .iter_mut()
        .find(|s| s.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;
    server.enabled.insert(tool_id.to_string(), enabled);
    server.updated_at = Some(now_ms());
    Ok(server.clone())
}

// ---------------------------------------------------------------------------
// Spec generation — canonical + per-tool transforms
// ---------------------------------------------------------------------------

/// Canonical "community" mcpServers value (used by Claude Code & Gemini).
///
/// stdio keeps `type` (modern Claude Code / Gemini accept it); http/sse carry
/// `url` and optional `headers`.
fn canonical_spec(entry: &McpServerEntry) -> Value {
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
fn claude_desktop_spec(entry: &McpServerEntry) -> Result<Value> {
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
fn opencode_spec(entry: &McpServerEntry) -> Value {
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
fn codex_toml_table(entry: &McpServerEntry) -> toml::Table {
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

// ---------------------------------------------------------------------------
// Per-tool config paths & installed detection
// ---------------------------------------------------------------------------

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

/// Resolve the live MCP config file for a tool.
pub fn resolve_mcp_config_path(tool_id: &str) -> Result<PathBuf> {
    match tool_id {
        "claude-code" => resolve_claude_json_path(),
        "claude-desktop" => resolve_claude_desktop_config_path(),
        "codex" => resolve_codex_config_path(),
        "gemini" => resolve_gemini_settings_path(),
        "opencode" => resolve_opencode_config_path(),
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
        "opencode" => home.join(".config").join("opencode").exists()
            || resolve_opencode_config_path().map(|p| p.exists()).unwrap_or(false),
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
        "codex" => toml::from_str::<toml::Table>(&content)
            .ok()
            .and_then(|t| t.get("mcp_servers").and_then(|v| v.as_table()).map(|m| m.len()))
            .unwrap_or(0),
        "opencode" => serde_json::from_str::<Value>(&content)
            .ok()
            .and_then(|v| v.get("mcp").and_then(|m| m.as_object()).map(|m| m.len()))
            .unwrap_or(0),
        // claude-code, claude-desktop, gemini all use top-level `mcpServers`.
        _ => serde_json::from_str::<Value>(&content)
            .ok()
            .and_then(|v| v.get("mcpServers").and_then(|m| m.as_object()).map(|m| m.len()))
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

fn backup_if_exists(path: &Path) -> Result<Option<PathBuf>> {
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
fn json_mcpservers_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
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
fn json_mcpservers_remove(path: &Path, name: &str) -> Result<()> {
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
fn opencode_upsert(path: &Path, name: &str, spec: Value) -> Result<()> {
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

fn opencode_remove(path: &Path, name: &str) -> Result<()> {
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
fn codex_upsert(path: &Path, name: &str, table: toml::Table) -> Result<()> {
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

fn codex_remove(path: &Path, name: &str) -> Result<()> {
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

fn write_toml_pretty(path: &Path, table: &toml::Table) -> Result<()> {
    ensure_parent(path)?;
    let out = toml::to_string_pretty(table).context("Failed to serialize TOML config")?;
    std::fs::write(path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Project a single server into one tool's live config.
///
/// When `force` is false and the tool is not installed, the write is skipped
/// and a `skipped: true` result is returned.
pub fn sync_server_to_tool(
    entry: &McpServerEntry,
    tool_id: &str,
    force: bool,
) -> McpSyncResult {
    let mut result = McpSyncResult {
        tool_id: tool_id.to_string(),
        server_id: entry.id.clone(),
        success: false,
        skipped: false,
        config_path: resolve_mcp_config_path(tool_id)
            .ok()
            .map(|p| p.to_string_lossy().to_string()),
        backup_path: None,
        error: None,
    };

    if !force && !tool_installed(tool_id) {
        result.success = true;
        result.skipped = true;
        return result;
    }

    match sync_server_to_tool_inner(entry, tool_id) {
        Ok(backup) => {
            result.success = true;
            result.backup_path = backup.map(|p| p.to_string_lossy().to_string());
        }
        Err(e) => result.error = Some(e.to_string()),
    }
    result
}

fn sync_server_to_tool_inner(entry: &McpServerEntry, tool_id: &str) -> Result<Option<PathBuf>> {
    validate_entry(entry)?;
    let path = resolve_mcp_config_path(tool_id)?;
    let backup = backup_if_exists(&path)?;
    match tool_id {
        "claude-code" | "gemini" => json_mcpservers_upsert(&path, &entry.name, canonical_spec(entry))?,
        "claude-desktop" => json_mcpservers_upsert(&path, &entry.name, claude_desktop_spec(entry)?)?,
        "opencode" => opencode_upsert(&path, &entry.name, opencode_spec(entry))?,
        "codex" => codex_upsert(&path, &entry.name, codex_toml_table(entry))?,
        _ => bail!("Unsupported tool '{tool_id}'"),
    }
    Ok(backup)
}

/// Remove a server (by name) from one tool's live config.
pub fn remove_server_from_tool(name: &str, tool_id: &str) -> McpSyncResult {
    let mut result = McpSyncResult {
        tool_id: tool_id.to_string(),
        server_id: name.to_string(),
        success: false,
        skipped: false,
        config_path: resolve_mcp_config_path(tool_id)
            .ok()
            .map(|p| p.to_string_lossy().to_string()),
        backup_path: None,
        error: None,
    };
    match remove_server_from_tool_inner(name, tool_id) {
        Ok(backup) => {
            result.success = true;
            result.backup_path = backup.map(|p| p.to_string_lossy().to_string());
        }
        Err(e) => result.error = Some(e.to_string()),
    }
    result
}

fn remove_server_from_tool_inner(name: &str, tool_id: &str) -> Result<Option<PathBuf>> {
    let path = resolve_mcp_config_path(tool_id)?;
    if !path.exists() {
        return Ok(None);
    }
    let backup = backup_if_exists(&path)?;
    match tool_id {
        "claude-code" | "claude-desktop" | "gemini" => json_mcpservers_remove(&path, name)?,
        "opencode" => opencode_remove(&path, name)?,
        "codex" => codex_remove(&path, name)?,
        _ => bail!("Unsupported tool '{tool_id}'"),
    }
    Ok(backup)
}

/// Project a server to all tools per its `enabled` map: enabled tools get an
/// upsert, disabled tools get a removal. Returns one result per tool touched.
pub fn sync_server_all_tools(entry: &McpServerEntry, force: bool) -> Vec<McpSyncResult> {
    MCP_TOOL_IDS
        .iter()
        .map(|&tool_id| {
            let enabled = entry.enabled.get(tool_id).copied().unwrap_or(false);
            if enabled {
                sync_server_to_tool(entry, tool_id, force)
            } else {
                remove_server_from_tool(&entry.name, tool_id)
            }
        })
        .collect()
}

/// Re-project every server in the store to every tool (full reconciliation).
pub fn sync_all(store: &McpStore, force: bool) -> Vec<McpSyncResult> {
    store
        .servers
        .iter()
        .flat_map(|s| sync_server_all_tools(s, force))
        .collect()
}

// ---------------------------------------------------------------------------
// Import from a tool's live config
// ---------------------------------------------------------------------------

/// Parse a community `mcpServers` JSON spec into store fields.
fn entry_from_json_spec(name: &str, spec: &Value) -> Option<McpServerEntry> {
    let obj = spec.as_object()?;
    let transport = obj.get("type").and_then(|v| v.as_str()).unwrap_or("stdio").to_string();
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
            entry.command = obj.get("command").and_then(|v| v.as_str()).map(String::from);
            entry.args = obj
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
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

fn blank_entry(name: &str, transport: &str) -> McpServerEntry {
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
                        && let Some(e) = entry_from_codex_table(name, tbl) {
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
    let transport = tbl.get("type").and_then(|v| v.as_str()).unwrap_or("stdio").to_string();
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
            entry.command = tbl.get("command").and_then(|v| v.as_str()).map(String::from);
            entry.args = tbl
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
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

fn entry_from_opencode_spec(name: &str, spec: &Value) -> Option<McpServerEntry> {
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
                let parts: Vec<String> =
                    arr.iter().filter_map(|x| x.as_str().map(String::from)).collect();
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
            found.sort_index =
                store.servers.iter().map(|s| s.sort_index).max().map_or(0, |m| m + 1);
            found.enabled.insert(tool_id.to_string(), true);
            store.servers.push(found);
            changed += 1;
        }
    }
    Ok(changed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio(name: &str) -> McpServerEntry {
        let mut e = blank_entry(name, "stdio");
        e.command = Some("npx".into());
        e.args = vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into()];
        e.env.insert("HOME".into(), "/Users/test".into());
        e
    }

    fn http(name: &str) -> McpServerEntry {
        let mut e = blank_entry(name, "http");
        e.url = Some("https://example.com/mcp".into());
        e.headers.insert("Authorization".into(), "Bearer xxx".into());
        e
    }

    #[test]
    fn canonical_stdio_has_type_command_args_env() {
        let v = canonical_spec(&stdio("fs"));
        assert_eq!(v["type"], "stdio");
        assert_eq!(v["command"], "npx");
        assert_eq!(v["args"][0], "-y");
        assert_eq!(v["env"]["HOME"], "/Users/test");
    }

    #[test]
    fn claude_desktop_omits_type_and_rejects_http() {
        let v = claude_desktop_spec(&stdio("fs")).unwrap();
        assert!(v.get("type").is_none());
        assert_eq!(v["command"], "npx");
        assert!(claude_desktop_spec(&http("remote")).is_err());
    }

    #[test]
    fn opencode_stdio_becomes_local_command_array() {
        let v = opencode_spec(&stdio("fs"));
        assert_eq!(v["type"], "local");
        assert_eq!(v["command"][0], "npx");
        assert_eq!(v["command"][1], "-y");
        assert_eq!(v["environment"]["HOME"], "/Users/test");
        assert_eq!(v["enabled"], true);
    }

    #[test]
    fn opencode_http_becomes_remote() {
        let v = opencode_spec(&http("r"));
        assert_eq!(v["type"], "remote");
        assert_eq!(v["url"], "https://example.com/mcp");
        assert_eq!(v["headers"]["Authorization"], "Bearer xxx");
        assert_eq!(v["enabled"], true);
    }

    #[test]
    fn codex_stdio_table_shape() {
        let t = codex_toml_table(&stdio("fs"));
        assert_eq!(t["type"].as_str(), Some("stdio"));
        assert_eq!(t["command"].as_str(), Some("npx"));
        assert_eq!(t["args"].as_array().unwrap().len(), 2);
        assert_eq!(t["env"].as_table().unwrap()["HOME"].as_str(), Some("/Users/test"));
    }

    #[test]
    fn codex_http_uses_http_headers() {
        let t = codex_toml_table(&http("r"));
        assert_eq!(t["type"].as_str(), Some("http"));
        assert_eq!(t["url"].as_str(), Some("https://example.com/mcp"));
        assert!(t.get("http_headers").is_some());
    }

    #[test]
    fn create_assigns_id_and_rejects_dupes() {
        let mut store = McpStore::default();
        let e = create_server(&mut store, stdio("fs")).unwrap();
        assert!(!e.id.is_empty());
        assert!(create_server(&mut store, stdio("fs")).is_err());
    }

    #[test]
    fn validate_requires_command_or_url() {
        let mut bad = blank_entry("x", "stdio");
        assert!(validate_entry(&bad).is_err());
        bad.command = Some("echo".into());
        assert!(validate_entry(&bad).is_ok());
        let mut badurl = blank_entry("y", "http");
        assert!(validate_entry(&badurl).is_err());
        badurl.url = Some("https://x".into());
        assert!(validate_entry(&badurl).is_ok());
    }

    #[test]
    fn set_tool_enabled_updates_map() {
        let mut store = McpStore::default();
        let e = create_server(&mut store, stdio("fs")).unwrap();
        let updated = set_tool_enabled(&mut store, &e.id, "codex", true).unwrap();
        assert_eq!(updated.enabled.get("codex"), Some(&true));
        assert!(set_tool_enabled(&mut store, &e.id, "bogus", true).is_err());
    }

    #[test]
    fn store_roundtrip_and_import_parse() {
        // canonical → json spec → parse back
        let e = stdio("fs");
        let spec = canonical_spec(&e);
        let parsed = entry_from_json_spec("fs", &spec).unwrap();
        assert_eq!(parsed.command, Some("npx".to_string()));
        assert_eq!(parsed.args.len(), 2);
        assert_eq!(parsed.env.get("HOME"), Some(&"/Users/test".to_string()));

        // opencode roundtrip
        let oc = opencode_spec(&e);
        let back = entry_from_opencode_spec("fs", &oc).unwrap();
        assert_eq!(back.command, Some("npx".to_string()));
        assert_eq!(back.args, vec!["-y", "@modelcontextprotocol/server-filesystem"]);
    }

    #[test]
    fn write_then_read_store() {
        let dir = std::env::temp_dir().join(format!("ss-mcp-test-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp_servers.json");
        let mut store = McpStore::default();
        create_server(&mut store, stdio("fs")).unwrap();
        write_mcp_store(&store, &path).unwrap();
        let loaded = read_mcp_store(&path).unwrap();
        assert_eq!(loaded.servers.len(), 1);
        assert_eq!(loaded.servers[0].name, "fs");
        std::fs::remove_dir_all(&dir).ok();
    }
}

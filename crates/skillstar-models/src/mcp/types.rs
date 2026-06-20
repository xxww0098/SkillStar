//! Data types for the unified MCP store: server entries, presets, patches,
//! the store root, and sync/status result shapes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Supported tools
// ---------------------------------------------------------------------------

/// Tool ids that can receive MCP servers, in display order.
pub const MCP_TOOL_IDS: &[&str] = &[
    "claude-code",
    "claude-desktop",
    "codex",
    "gemini",
    "opencode",
    "zcode",
];

/// Human-readable label for a tool id.
pub fn mcp_tool_label(tool_id: &str) -> &'static str {
    match tool_id {
        "claude-code" => "Claude Code",
        "claude-desktop" => "Claude Desktop",
        "codex" => "Codex",
        "gemini" => "Gemini CLI",
        "opencode" => "OpenCode",
        "zcode" => "ZCode",
        _ => "Unknown",
    }
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

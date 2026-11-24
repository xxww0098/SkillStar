use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Wire transport of an external MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    #[default]
    Stdio,
    Http,
    Sse,
}

impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
            Self::Sse => "sse",
        }
    }

    pub fn is_remote(self) -> bool {
        matches!(self, Self::Http | Self::Sse)
    }
}

/// Wire encoding family used by an agent or project MCP file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpFormat {
    Claude,
    Desktop,
    Codex,
    Antigravity,
    Cursor,
    OpenCode,
    PiNative,
    Pi,
    Omp,
    Goose,
    Copilot,
    Crush,
    ZCode,
    Kimi,
    Alma,
    Hermes,
    Dsh,
    Devin,
    Grok,
    Cline,
    CommandCode,
    Zed,
    Kiro,
    LiveAgent,
}

impl McpFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Desktop => "desktop",
            Self::Codex => "codex",
            Self::Antigravity => "antigravity",
            Self::Cursor => "cursor",
            Self::OpenCode => "opencode",
            Self::PiNative => "pi-native",
            Self::Pi => "pi",
            Self::Omp => "omp",
            Self::Goose => "goose",
            Self::Copilot => "copilot",
            Self::Crush => "crush",
            Self::ZCode => "zcode",
            Self::Kimi => "kimi",
            Self::Alma => "alma",
            Self::Hermes => "hermes",
            Self::Dsh => "dsh",
            Self::Devin => "devin",
            Self::Grok => "grok",
            Self::Cline => "cline",
            Self::CommandCode => "commandcode",
            Self::Zed => "zed",
            Self::Kiro => "kiro",
            Self::LiveAgent => "liveagent",
        }
    }

    pub fn supports_remote(self) -> bool {
        !matches!(self, Self::Desktop)
    }

    pub fn supports_sse(self) -> bool {
        !matches!(
            self,
            Self::Desktop
                | Self::Codex
                | Self::PiNative
                | Self::Goose
                | Self::Dsh
                | Self::Grok
                | Self::CommandCode
                | Self::Zed
                | Self::Kiro
                | Self::LiveAgent
        )
    }
}

/// Canonical managed external MCP server definition in `data/mcp/library.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub name: String,
    #[serde(default)]
    pub transport: Transport,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, alias = "hidden", skip_serializing_if = "is_false")]
    pub paused: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_timeout_sec: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// Library-side ownership ledger per agent so deletes/disables only touch entries we wrote.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedAgentLedger {
    /// Server names written by SkillStar to this agent's config file.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub mcp: BTreeSet<String>,
    /// Disabled tool names written by SkillStar per server name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools_off: BTreeMap<String, BTreeSet<String>>,
}

/// Root file persisted at `~/.skillstar/data/mcp/library.json`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub servers: Vec<McpServer>,
    #[serde(default)]
    pub applied: BTreeMap<String, AppliedAgentLedger>,
}

fn default_version() -> u32 {
    1
}

/// Live detection state and capabilities for a canonical MCP agent target.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAgentTarget {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub shared_here: bool,
    pub config_path: String,
    pub format: McpFormat,
    pub no_remote: bool,
    pub no_sse: bool,
    pub supports_tools_off: bool,
    pub supports_startup_timeout: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Unmanaged MCP server discovered in an agent's existing config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundServer {
    pub agent_id: String,
    pub agent_name: String,
    pub config_path: String,
    pub format: McpFormat,
    pub server: McpServer,
}

/// Conflict action when saving or importing a server whose name collides with the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConflictStrategy {
    #[default]
    Rename,
    Replace,
    Keep,
}

/// Non-fatal sync or read diagnostic (e.g., Zed foreign extension guard, no-remote skip).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncWarning {
    pub agent_id: String,
    pub path: String,
    pub code: String,
    pub message: String,
}

/// Summary returned to CLI and GUI after library reads or sync mutations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub servers: Vec<McpServer>,
    pub agents: Vec<McpAgentTarget>,
    pub found: Vec<FoundServer>,
    pub warnings: Vec<SyncWarning>,
}

/// Lightweight mutation summary when agent targets and unmanaged discoveries are unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDelta {
    pub servers: Vec<McpServer>,
    pub warnings: Vec<SyncWarning>,
}

/// Result returned when inspecting a single server's live MCP capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server_version: String,
    #[serde(default)]
    pub tools: Vec<DiscoveredTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Project-level config snippet generated for one of the 10 project formats.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnippet {
    pub agent_id: String,
    pub agent_name: String,
    pub rel_path: String,
    pub format: McpFormat,
    pub content: String,
}

/// User-supplied field required by a store server template before installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreInputSpec {
    pub key: String,
    pub where_field: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub placeholder: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub format: String,
}

/// Store catalog item returned from featured list, snapshot, or live MCP Registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreItem {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub publisher: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    pub transport: Transport,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub runs: String,
    #[serde(default)]
    pub sign_in: bool,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub match_by_addr: bool,
    #[serde(default)]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stars: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub have: Option<String>,
    #[serde(default)]
    pub inputs: Vec<StoreInputSpec>,
    pub template: McpServer,
}

/// Response returned by built-in store search.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreSearchResponse {
    pub items: Vec<StoreItem>,
    pub online_used: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online_error: Option<String>,
}

/// Request payload for installing an item from the built-in store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreInstallRequest {
    pub item: StoreItem,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    #[serde(default)]
    pub agents: Vec<String>,
}

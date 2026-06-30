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
    "grok",
    "opencode",
    "zcode",
    "kiro",
];

/// Human-readable label for a tool id.
pub fn mcp_tool_label(tool_id: &str) -> &'static str {
    match tool_id {
        "claude-code" => "Claude Code",
        "claude-desktop" => "Claude Desktop",
        "codex" => "Codex",
        "gemini" => "Gemini CLI",
        "grok" => "Grok",
        "opencode" => "OpenCode",
        "zcode" => "ZCode",
        "kiro" => "Kiro",
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

/// Build a stdio MCP preset (command + args + optional env).
///
/// `env` is a list of `(name, default, required)` tuples: `required` ones are
/// also pushed into `required_env` so the UI can highlight blanks the user must
/// fill before the server works.
fn stdio_preset(
    id: &str,
    description: &str,
    homepage: &str,
    command: &str,
    args: &[&str],
    env: &[(&str, &str, bool)],
    tags: &[&str],
) -> McpPreset {
    let mut env_map = BTreeMap::new();
    let mut required_env = Vec::new();
    for (name, default, required) in env {
        env_map.insert((*name).to_string(), (*default).to_string());
        if *required {
            required_env.push((*name).to_string());
        }
    }
    McpPreset {
        id: id.to_string(),
        name: id.to_string(),
        description: description.to_string(),
        homepage: homepage.to_string(),
        transport: "stdio".to_string(),
        command: Some(command.to_string()),
        args: args.iter().map(|s| (*s).to_string()).collect(),
        env: env_map,
        url: None,
        headers: BTreeMap::new(),
        tags: tags.iter().map(|s| (*s).to_string()).collect(),
        required_env,
    }
}

/// Build a remote (http/sse) MCP preset (url + optional headers). Each header is
/// pre-seeded with an empty value so the UI shows the field for the user to fill.
fn remote_preset(
    id: &str,
    description: &str,
    homepage: &str,
    transport: &str,
    url: &str,
    headers: &[&str],
    tags: &[&str],
) -> McpPreset {
    McpPreset {
        id: id.to_string(),
        name: id.to_string(),
        description: description.to_string(),
        homepage: homepage.to_string(),
        transport: transport.to_string(),
        command: None,
        args: Vec::new(),
        env: BTreeMap::new(),
        url: Some(url.to_string()),
        headers: headers
            .iter()
            .map(|h| ((*h).to_string(), String::new()))
            .collect(),
        tags: tags.iter().map(|s| (*s).to_string()).collect(),
        required_env: Vec::new(),
    }
}

/// Built-in recommended MCP presets — single source of truth for the UI's
/// one-click "add a recommended server" list. Kept accurate to each server's
/// real runtime (npx / uvx) and official package / endpoint.
pub fn get_mcp_presets() -> Vec<McpPreset> {
    let mcp_servers_repo = "https://github.com/modelcontextprotocol/servers";
    vec![
        // ── Browser / automation ────────────────────────────────────────
        stdio_preset(
            "adspower-local-api",
            "AdsPower 浏览器 Local API — 通过 MCP 控制指纹浏览器 / 自动化。",
            "https://github.com/AdsPower/adspower-browser",
            "npx",
            &["-y", "local-api-mcp-typescript"],
            &[("PORT", "50325", false), ("API_KEY", "", true)],
            &["browser", "automation"],
        ),
        stdio_preset(
            "playwright",
            "微软官方 Playwright MCP — 浏览器自动化，AI 可打开网页、点击、填表、截图。",
            "https://github.com/microsoft/playwright-mcp",
            "npx",
            &["-y", "@playwright/mcp@latest"],
            &[],
            &["browser", "automation", "testing"],
        ),
        stdio_preset(
            "chrome-devtools",
            "Chrome 官方 DevTools MCP — 驱动 Chrome 调试、抓取性能/网络、检查 DOM 与控制台。",
            "https://github.com/ChromeDevTools/chrome-devtools-mcp",
            "npx",
            &["-y", "chrome-devtools-mcp@latest"],
            &[],
            &["browser", "debug"],
        ),
        // ── Official reference servers (Anthropic / modelcontextprotocol) ─
        stdio_preset(
            "filesystem",
            "官方文件系统 MCP — 读写本地文件与目录（需在 args 末尾追加允许访问的目录）。",
            mcp_servers_repo,
            "npx",
            &["-y", "@modelcontextprotocol/server-filesystem"],
            &[],
            &["files", "official"],
        ),
        stdio_preset(
            "memory",
            "官方记忆 MCP — 基于知识图谱的持久化记忆，跨会话存取实体与关系。",
            mcp_servers_repo,
            "npx",
            &["-y", "@modelcontextprotocol/server-memory"],
            &[],
            &["memory", "official"],
        ),
        stdio_preset(
            "sequential-thinking",
            "官方思维链 MCP — 通过结构化、可回溯的思维序列进行动态反思式问题求解。",
            mcp_servers_repo,
            "npx",
            &["-y", "@modelcontextprotocol/server-sequential-thinking"],
            &[],
            &["reasoning", "official"],
        ),
        stdio_preset(
            "fetch",
            "官方抓取 MCP — 获取 URL 内容并转为 Markdown，供模型读取网页/文档（uvx 运行）。",
            mcp_servers_repo,
            "uvx",
            &["mcp-server-fetch"],
            &[],
            &["web", "official"],
        ),
        stdio_preset(
            "git",
            "官方 Git MCP — status / diff / log / commit 等本地仓库操作（uvx 运行）。",
            mcp_servers_repo,
            "uvx",
            &["mcp-server-git"],
            &[],
            &["git", "official"],
        ),
        stdio_preset(
            "time",
            "官方时间 MCP — 当前时间查询与时区转换（uvx 运行）。",
            mcp_servers_repo,
            "uvx",
            &["mcp-server-time"],
            &[],
            &["time", "official"],
        ),
        // ── Docs / search / crawl ───────────────────────────────────────
        stdio_preset(
            "context7",
            "Context7 MCP — 为 AI 提供最新版库/框架文档上下文，避免使用过时 API。",
            "https://github.com/upstash/context7",
            "npx",
            &["-y", "@upstash/context7-mcp"],
            &[],
            &["docs", "context"],
        ),
        stdio_preset(
            "brave-search",
            "Brave 官方搜索 MCP — 通过 Brave Search API 提供 Web 搜索与本地商户搜索能力。",
            "https://github.com/brave/brave-search-mcp-server",
            "npx",
            &["-y", "@brave/brave-search-mcp-server"],
            &[("BRAVE_API_KEY", "", true)],
            &["search", "web"],
        ),
        stdio_preset(
            "firecrawl",
            "Firecrawl MCP — 抓取/爬取任意网站转为干净的 Markdown，供 AI 读取与分析。",
            "https://github.com/firecrawl/firecrawl-mcp-server",
            "npx",
            &["-y", "firecrawl-mcp"],
            &[("FIRECRAWL_API_KEY", "", true)],
            &["web", "crawl"],
        ),
        // ── Data / SaaS ─────────────────────────────────────────────────
        stdio_preset(
            "supabase",
            "Supabase 官方 MCP — 管理 Postgres 数据库、表结构、RLS 策略、Auth 用户与存储。",
            "https://github.com/supabase/mcp-server-supabase",
            "npx",
            &["-y", "@supabase/mcp-server-supabase"],
            &[("SUPABASE_ACCESS_TOKEN", "", true)],
            &["database", "supabase"],
        ),
        // ── X (Twitter) ─────────────────────────────────────────────────
        stdio_preset(
            "xapi",
            "X 官方 MCP — 通过 xurl 桥接 X API：发帖、搜索、用户/时间线、书签、趋势等（首次需浏览器 OAuth 登录，建议启动超时 ≥300s）。",
            "https://docs.x.com/tools/mcp",
            "npx",
            &["-y", "@xdevplatform/xurl", "mcp", "https://api.x.com/mcp"],
            &[("CLIENT_ID", "", true), ("CLIENT_SECRET", "", true)],
            &["x", "social", "official"],
        ),
        remote_preset(
            "x-docs",
            "X 官方文档 MCP — search_x / get_page_x 工具，检索 X API 指南与示例（无需鉴权）。",
            "https://docs.x.com/tools/mcp",
            "http",
            "https://docs.x.com/mcp",
            &[],
            &["x", "docs", "official"],
        ),
        // ── Remote (http) servers ───────────────────────────────────────
        remote_preset(
            "github",
            "GitHub 官方远程 MCP — 仓库、issue、PR、代码搜索等（Authorization 填 Bearer <PAT>）。",
            "https://github.com/github/github-mcp-server",
            "http",
            "https://api.githubcopilot.com/mcp/",
            &["Authorization"],
            &["git", "github"],
        ),
        remote_preset(
            "notion",
            "Notion 官方远程 MCP — 管理笔记、数据库、页面，AI 可读写你的 Notion 工作区。",
            "https://github.com/makenotion/notion-mcp-server",
            "http",
            "https://mcp.notion.com/mcp",
            &["Authorization"],
            &["saas", "notion"],
        ),
    ]
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

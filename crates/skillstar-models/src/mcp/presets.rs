//! Built-in MCP preset catalog — the recommended-servers list the UI offers
//! for one-click creation, plus the merge rule that folds marketplace-curated
//! rows in front of it.
//!
//! Split out of `types.rs`: this is a large, slow-churning data table whose
//! edits (a new recommended server) have nothing to do with the store schema
//! that file otherwise owns.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

// ---------------------------------------------------------------------------
// Built-in / recommended MCP presets
// ---------------------------------------------------------------------------

/// A built-in, recommended-to-install MCP server template.
///
/// Mirrors the `ProviderPresetFlat` pattern: the registry below is the single
/// source of truth, exposed to the UI via the `get_mcp_presets` command.
///
/// A preset leads to one of two install paths, decided by [`Self::catalog_id`]:
/// a curated row opens the install wizard on that row, and a built-in pre-fills
/// the create form (leaving any `required_env` keys blank for the user to fill
/// in) which then creates a normal [`McpServerEntry`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpPreset.ts")]
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
    /// Catalog row this preset was derived from, when it has one.
    ///
    /// This is the routing marker for the chip: `Some` means the install wizard
    /// can resolve the row (a curated preset's id *is* its catalog row id), so
    /// the chip opens the wizard and the user gets the runtime-shape picker,
    /// masked secret fields and the command confirmation. Built-in presets have
    /// no catalog row, carry `None`, and keep the plain create form. Routing on
    /// an explicit marker rather than on "try to resolve, fall back on miss"
    /// keeps a transient catalog read from silently retiring an entry point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
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
        catalog_id: None,
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
        catalog_id: None,
    }
}

/// Built-in recommended MCP presets — the always-available floor the UI shows
/// when the marketplace snapshot is missing or unreadable.
///
/// Scope: byte-for-byte the same ids the marketplace's curated catalog
/// carries — the curated list *is* the recommended shortlist, so the add
/// dialog offers one consistent set whether or not the snapshot DB resolved.
/// Keep an id here byte-identical to its curated twin: the merge dedupes on
/// id and name, so a drifted id shows the server twice.
///
/// Kept accurate to each server's real runtime: `git` ships on PyPI and runs
/// under `uvx`, the TypeScript ones under `npx`.
pub fn get_mcp_presets() -> Vec<McpPreset> {
    let mcp_servers_repo = "https://github.com/modelcontextprotocol/servers";
    vec![
        // ── Core: files, version control, code hosting ──────────────────
        stdio_preset(
            "filesystem",
            "官方文件系统 MCP — 读写本地文件与目录（需在 args 末尾追加允许访问的目录）。",
            mcp_servers_repo,
            "npx",
            &["-y", "@modelcontextprotocol/server-filesystem"],
            &[],
            &["files", "core", "recommended"],
        ),
        stdio_preset(
            "git",
            "官方 Git MCP — status / diff / log / commit 等本地仓库操作（uvx 运行）。",
            mcp_servers_repo,
            "uvx",
            &["mcp-server-git"],
            &[],
            &["git", "core", "recommended"],
        ),
        remote_preset(
            "github",
            "GitHub 官方远程 MCP — 仓库、issue、PR、代码搜索等（Authorization 填 Bearer <PAT>）。",
            "https://github.com/github/github-mcp-server",
            "http",
            "https://api.githubcopilot.com/mcp/",
            &["Authorization"],
            &["git", "github", "core", "recommended"],
        ),
        // ── Context: docs and code grounding ────────────────────────────
        stdio_preset(
            "context7",
            "Context7 MCP — 为 AI 提供最新版库/框架文档上下文，避免使用过时 API。",
            "https://github.com/upstash/context7",
            "npx",
            &["-y", "@upstash/context7-mcp"],
            &[],
            &["docs", "context", "recommended"],
        ),
        remote_preset(
            "deepwiki",
            "DeepWiki MCP — 读取任意公开 GitHub 仓库的 AI 文档并回答仓库内问题（无需鉴权）。",
            "https://docs.devin.ai/work-with-devin/deepwiki-mcp",
            "http",
            "https://mcp.deepwiki.com/mcp",
            &[],
            &["docs", "repo", "context", "recommended"],
        ),
        // Official MCP wire-up is `codegraph serve --mcp`. The npm bin is the
        // same CLI, so npx is the one-click path (no prior global install).
        // Each project still needs `codegraph init` before the graph has data.
        stdio_preset(
            "codegraph",
            "CodeGraph MCP — 本地代码知识图谱，一次调用返回符号源码、调用链与影响范围（项目需先 codegraph init）。",
            "https://github.com/colbymchenry/codegraph",
            "npx",
            &["-y", "@colbymchenry/codegraph", "serve", "--mcp"],
            &[],
            &["code", "graph", "local", "context", "recommended"],
        ),
        // Serena's executable (`serena`) is not its package name, so uvx needs
        // `--from <source>` before it; `--project-from-cwd` binds the server to
        // whichever directory the launching tool runs it from.
        stdio_preset(
            "serena",
            "Serena — 基于 LSP 的语义代码工具：符号查找、引用分析与精准编辑，适合在大型仓库里做「改代码」而非「读代码」。",
            "https://github.com/oraios/serena",
            "uvx",
            &[
                "--from",
                "git+https://github.com/oraios/serena",
                "serena",
                "start-mcp-server",
                "--project-from-cwd",
            ],
            &[],
            &["code", "lsp", "semantic", "context", "recommended"],
        ),
        // ── Browser: web verification & debugging ───────────────────────
        stdio_preset(
            "playwright",
            "微软官方 Playwright MCP — 浏览器自动化，AI 可打开网页、点击、填表、截图。",
            "https://github.com/microsoft/playwright-mcp",
            "npx",
            &["-y", "@playwright/mcp@latest"],
            &[],
            &["browser", "automation", "testing", "recommended"],
        ),
        stdio_preset(
            "chrome-devtools",
            "Chrome 官方 DevTools MCP — 驱动 Chrome 调试、抓取性能/网络、检查 DOM 与控制台。",
            "https://github.com/ChromeDevTools/chrome-devtools-mcp",
            "npx",
            &["-y", "chrome-devtools-mcp@latest"],
            &[],
            &["browser", "debug", "recommended"],
        ),
        // ── Creative: design & content tools ────────────────────────────
        remote_preset(
            "figma",
            "Figma 官方 MCP — 设计稿结构、组件与变量交给 agent，实现设计到代码（OAuth）。",
            "https://www.figma.com/",
            "http",
            "https://mcp.figma.com/mcp",
            &[],
            &["design", "figma", "creative", "recommended"],
        ),
        // blender-mcp runs under uvx; the Blender side needs the bundled
        // addon installed and the app running for the socket to answer.
        stdio_preset(
            "blender",
            "Blender MCP — 建模、材质、渲染与场景脚本驱动（需 Blender 运行 + 配套插件）。",
            "https://github.com/ahujasid/blender-mcp",
            "uvx",
            &["blender-mcp"],
            &[],
            &["3d", "blender", "creative", "recommended"],
        ),
        stdio_preset(
            "photoshop",
            "Photoshop MCP — 文档操作、生成式填充与批处理配方，带独立 Web 面板。",
            "https://github.com/alisaitteke/photoshop-mcp",
            "npx",
            &["-y", "@alisaitteke/photoshop-mcp"],
            &[],
            &["image", "photoshop", "creative", "recommended"],
        ),
        remote_preset(
            "after-effects",
            "After Effects 连接器（Oneprism）— 读取与改动真实合成：图层、关键帧、表达式（OAuth）。",
            "https://oneprism.io",
            "http",
            "https://live.oneprism.io/after-effects",
            &[],
            &["video", "after-effects", "creative", "recommended"],
        ),
    ]
}

/// Merge marketplace-curated presets with the built-in catalog, first one wins.
///
/// The two sources are *complementary*, not alternatives: curated rows track
/// what the marketplace promotes right now, the built-in catalog is the
/// always-available floor that survives an empty or unreadable snapshot DB.
/// Curated entries lead so a freshly promoted server outranks its built-in
/// twin; a preset already seen under the same id or (case-insensitive) name is
/// dropped so the UI never shows the same server twice.
pub fn merge_mcp_presets(curated: Vec<McpPreset>, builtin: Vec<McpPreset>) -> Vec<McpPreset> {
    let mut seen_ids = std::collections::HashSet::new();
    let mut seen_names = std::collections::HashSet::new();
    let mut merged = Vec::with_capacity(curated.len() + builtin.len());
    for preset in curated.into_iter().chain(builtin) {
        let name_key = preset.name.trim().to_lowercase();
        if !seen_ids.insert(preset.id.clone()) || !seen_names.insert(name_key) {
            continue;
        }
        merged.push(preset);
    }
    merged
}

//! The curated MCP catalog — the servers the store's 精选 list shows.
//!
//! Scope rule for this table: **the curated list is the recommended
//! shortlist, not a survey of what exists.** Every row is a commonly used
//! programming MCP and carries `recommended: true`, which is also what lands
//! it on the add dialog's preset chips. On 2026-09-12 the catalog was cut from
//! the broader programming whitelist (29 rows across eight publisher buckets)
//! to this shortlist — databases, cloud/infra, issue trackers, scraping and
//! doc-conversion rows left the curated shelf; their discovery path is the
//! store's 完整目录 scope, which queries the remote registries directly.
//!
//! `source` is the shelf a row sits on — `core` (the daily dev loop: files,
//! Git, hosting), `context` (docs and code grounding), `browser` (web
//! verification and debugging) and `creative` (design & content tools like
//! Figma, Blender, Photoshop, After Effects). The store groups cards by it, so
//! rows of one shelf stay contiguous in this list (the seed index is the
//! store's sort key).
//!
//! Accuracy rule: every package identifier, launcher and endpoint below was
//! read off the official MCP registry or the publisher's own PyPI/npm record,
//! not off a blog post. `git` ships on PyPI only — seeding it as an npm
//! package is what made the old row uninstallable.

use super::helpers::{CuratedSpec, Launch, Registry};

const OFFICIAL_REPO: &str = "https://github.com/modelcontextprotocol/servers";

/// Every curated row, in shelf order (the seed index is the store's sort key).
pub(super) fn catalog() -> Vec<CuratedSpec> {
    vec![
        // ── Core (source = "core") — files, version control, code hosting ──
        CuratedSpec {
            id: "filesystem",
            name: "filesystem",
            source: "core",
            description: "官方文件系统 MCP — 读写本地文件与目录。安装后在命令确认里把允许访问的目录追加到参数末尾，未列出的路径一律拒绝。",
            homepage: OFFICIAL_REPO,
            launch: Launch::Stdio {
                registry: Registry::Npm,
                identifier: "@modelcontextprotocol/server-filesystem",
                runtime_args: &[],
                args: &[],
            },
            env: &[],
            recommended: true,
        },
        CuratedSpec {
            id: "git",
            name: "git",
            source: "core",
            description: "官方 Git MCP — 读取、搜索与改写本地仓库：status / diff / log / commit / branch。",
            homepage: OFFICIAL_REPO,
            launch: Launch::Stdio {
                registry: Registry::Pypi,
                identifier: "mcp-server-git",
                runtime_args: &[],
                args: &[],
            },
            env: &[],
            recommended: true,
        },
        CuratedSpec {
            id: "github",
            name: "github",
            source: "core",
            description: "GitHub 官方远程 MCP — 仓库、issue、PR、Actions 日志与代码搜索。Authorization 填 `Bearer <PAT>`，建议只给 repo 读权限。",
            homepage: "https://github.com/github/github-mcp-server",
            launch: Launch::Remote {
                url: "https://api.githubcopilot.com/mcp/",
                auth_header: "Authorization",
            },
            env: &[],
            recommended: true,
        },
        // ── Context (source = "context") — docs and code grounding ───────
        CuratedSpec {
            id: "context7",
            name: "context7",
            source: "context",
            description: "Context7 MCP — 按版本注入库与框架的真实文档，避免模型凭记忆写出过时 API。",
            homepage: "https://github.com/upstash/context7",
            launch: Launch::Stdio {
                registry: Registry::Npm,
                identifier: "@upstash/context7-mcp",
                runtime_args: &[],
                args: &[],
            },
            env: &[],
            recommended: true,
        },
        CuratedSpec {
            id: "deepwiki",
            name: "deepwiki",
            source: "context",
            description: "DeepWiki MCP — 读取任意公开 GitHub 仓库的 AI 文档并回答仓库内问题，无需密钥，适合接手陌生依赖。",
            homepage: "https://docs.devin.ai/work-with-devin/deepwiki-mcp",
            launch: Launch::Remote {
                url: "https://mcp.deepwiki.com/mcp",
                auth_header: "",
            },
            env: &[],
            recommended: true,
        },
        CuratedSpec {
            id: "codegraph",
            name: "codegraph",
            source: "context",
            description: "CodeGraph MCP — 本地代码知识图谱，一次调用返回符号源码、调用链与影响范围（项目需先 codegraph init）。",
            homepage: "https://github.com/colbymchenry/codegraph",
            launch: Launch::Stdio {
                registry: Registry::Npm,
                identifier: "@colbymchenry/codegraph",
                runtime_args: &[],
                args: &["serve", "--mcp"],
            },
            env: &[],
            recommended: true,
        },
        CuratedSpec {
            id: "serena",
            name: "serena",
            source: "context",
            description: "Serena — 基于 LSP 的语义代码工具：符号查找、引用分析与精准编辑，适合在大型仓库里做「改代码」而非「读代码」。",
            homepage: "https://github.com/oraios/serena",
            launch: Launch::Stdio {
                registry: Registry::Pypi,
                identifier: "serena",
                runtime_args: &[("--from", "git+https://github.com/oraios/serena")],
                args: &["start-mcp-server", "--project-from-cwd"],
            },
            env: &[],
            recommended: true,
        },
        // ── Browser (source = "browser") — web verification & debugging ──
        CuratedSpec {
            id: "playwright",
            name: "playwright",
            source: "browser",
            description: "微软官方 Playwright MCP — 用真实 Chromium 打开网页、点击、填表、截图、跑 JS，做端到端验证与抓取。",
            homepage: "https://github.com/microsoft/playwright-mcp",
            launch: Launch::Stdio {
                registry: Registry::Npm,
                identifier: "@playwright/mcp",
                runtime_args: &[],
                args: &[],
            },
            env: &[],
            recommended: true,
        },
        CuratedSpec {
            id: "chrome-devtools",
            name: "chrome-devtools",
            source: "browser",
            description: "Chrome 官方 DevTools MCP — 驱动 Chrome 调试：抓性能轨迹、网络请求、控制台报错，检查 DOM。",
            homepage: "https://github.com/ChromeDevTools/chrome-devtools-mcp",
            launch: Launch::Stdio {
                registry: Registry::Npm,
                identifier: "chrome-devtools-mcp",
                runtime_args: &[],
                args: &[],
            },
            env: &[],
            recommended: true,
        },
        // ── Creative (source = "creative") — design & content tools ─────
        CuratedSpec {
            id: "figma",
            name: "figma",
            source: "creative",
            description: "Figma 官方 MCP — 把设计稿结构、组件与变量交给 agent，实现设计到代码。OAuth 登录 Figma 账号。",
            homepage: "https://www.figma.com/",
            launch: Launch::Remote {
                url: "https://mcp.figma.com/mcp",
                auth_header: "",
            },
            env: &[],
            recommended: true,
        },
        CuratedSpec {
            id: "blender",
            name: "blender",
            source: "creative",
            description: "Blender MCP — 建模、材质、渲染与场景脚本驱动。需 Blender 运行中并装配套插件，MCP 经 socket 连接。",
            homepage: "https://github.com/ahujasid/blender-mcp",
            launch: Launch::Stdio {
                registry: Registry::Pypi,
                identifier: "blender-mcp",
                runtime_args: &[],
                args: &[],
            },
            env: &[],
            recommended: true,
        },
        CuratedSpec {
            id: "photoshop",
            name: "photoshop",
            source: "creative",
            description: "Photoshop MCP — 118 个工具：文档操作、生成式填充与批处理配方，带独立 Web 面板。",
            homepage: "https://github.com/alisaitteke/photoshop-mcp",
            launch: Launch::Stdio {
                registry: Registry::Npm,
                identifier: "@alisaitteke/photoshop-mcp",
                runtime_args: &[],
                args: &[],
            },
            env: &[],
            recommended: true,
        },
        CuratedSpec {
            id: "after-effects",
            name: "after-effects",
            source: "creative",
            description: "After Effects 连接器（Oneprism）— 读取与改动真实合成：图层、关键帧、表达式。OAuth 授权。",
            homepage: "https://oneprism.io",
            launch: Launch::Remote {
                url: "https://live.oneprism.io/after-effects",
                auth_header: "",
            },
            env: &[],
            recommended: true,
        },
    ]
}

//! Curated MCP seeds for additional official publishers.

use crate::mcp_models::{
    McpRegistryPackageSummary, McpRegistryRemoteSummary, McpRegistryServer, McpServerKind,
};

use super::helpers::{make_remote_curated, make_stdio_curated, remote_http_raw, stdio_npx_raw};

/// Anthropic official reference MCP servers (modelcontextprotocol/servers).
/// All four are stdio/npx under the `@modelcontextprotocol/` npm scope.
pub(super) fn anthropic_curated_servers() -> Vec<McpRegistryServer> {
    let source = "anthropic";
    let repo = "https://github.com/modelcontextprotocol/servers";
    let fs_raw = stdio_npx_raw(
        "anthropic-filesystem",
        "filesystem",
        "Anthropic 官方文件系统 MCP — 让 AI 读写本地文件与目录，支持受限路径访问。",
        "@modelcontextprotocol/server-filesystem",
        repo,
        &[],
    );
    let git_raw = stdio_npx_raw(
        "anthropic-git",
        "git",
        "Anthropic 官方 Git MCP — status / diff / log / commit 等本地仓库操作。",
        "@modelcontextprotocol/server-git",
        repo,
        &[],
    );
    let fetch_raw = stdio_npx_raw(
        "anthropic-fetch",
        "fetch",
        "Anthropic 官方抓取 MCP — 获取 URL 内容并转为 Markdown，供模型读取网页/文档。",
        "@modelcontextprotocol/server-fetch",
        repo,
        &[],
    );
    let think_raw = stdio_npx_raw(
        "anthropic-sequential-thinking",
        "sequential-thinking",
        "Anthropic 官方思维链 MCP — 通过结构化、可回溯的思维序列进行动态反思式问题求解。",
        "@modelcontextprotocol/server-sequential-thinking",
        repo,
        &[],
    );
    vec![
        make_stdio_curated(
            "anthropic-filesystem",
            "filesystem",
            "Anthropic 官方文件系统 MCP — 让 AI 读写本地文件与目录，支持受限路径访问。",
            &fs_raw,
            source,
            repo,
            "@modelcontextprotocol/server-filesystem",
            &[],
        ),
        make_stdio_curated(
            "anthropic-git",
            "git",
            "Anthropic 官方 Git MCP — status / diff / log / commit 等本地仓库操作。",
            &git_raw,
            source,
            repo,
            "@modelcontextprotocol/server-git",
            &[],
        ),
        make_stdio_curated(
            "anthropic-fetch",
            "fetch",
            "Anthropic 官方抓取 MCP — 获取 URL 内容并转为 Markdown，供模型读取网页/文档。",
            &fetch_raw,
            source,
            repo,
            "@modelcontextprotocol/server-fetch",
            &[],
        ),
        make_stdio_curated(
            "anthropic-sequential-thinking",
            "sequential-thinking",
            "Anthropic 官方思维链 MCP — 通过结构化、可回溯的思维序列进行动态反思式问题求解。",
            &think_raw,
            source,
            repo,
            "@modelcontextprotocol/server-sequential-thinking",
            &[],
        ),
    ]
}

/// Microsoft official MCP servers.
pub(super) fn microsoft_curated_servers() -> Vec<McpRegistryServer> {
    let source = "microsoft";
    let pw_repo = "https://github.com/microsoft/playwright-mcp";
    let pw_raw = stdio_npx_raw(
        "microsoft-playwright",
        "playwright",
        "微软官方 Playwright MCP — 通过 Playwright 提供浏览器自动化能力，AI 可与网页交互。",
        "@executeautomation/playwright-mcp-server",
        pw_repo,
        &[],
    );
    let md_repo = "https://github.com/microsoft/markitdown";
    let md_raw = stdio_npx_raw(
        "microsoft-markitdown",
        "markitdown",
        "微软官方 MarkItDown MCP — 将 PDF / Word / Excel / 图片等文件转为 Markdown。",
        "markitdown-mcp",
        md_repo,
        &[],
    );
    vec![
        make_stdio_curated(
            "microsoft-playwright",
            "playwright",
            "微软官方 Playwright MCP — 通过 Playwright 提供浏览器自动化能力，AI 可与网页交互。",
            &pw_raw,
            source,
            pw_repo,
            "@executeautomation/playwright-mcp-server",
            &[],
        ),
        make_stdio_curated(
            "microsoft-markitdown",
            "markitdown",
            "微软官方 MarkItDown MCP — 将 PDF / Word / Excel / 图片等文件转为 Markdown。",
            &md_raw,
            source,
            md_repo,
            "markitdown-mcp",
            &[],
        ),
    ]
}

/// Mainstream SaaS brand remote MCP servers (Notion / Figma / Stripe).
pub(super) fn saas_curated_servers() -> Vec<McpRegistryServer> {
    let source = "saas";
    let notion_raw = remote_http_raw(
        "saas-notion",
        "notion",
        "Notion 官方远程 MCP — 管理笔记、数据库、页面，AI 可读写你的 Notion 工作区。",
        "https://mcp.notion.com/mcp",
        "https://github.com/makenotion/notion-mcp-server",
        "Authorization",
        "NOTION_API_KEY",
    );
    let figma_raw = remote_http_raw(
        "saas-figma",
        "figma",
        "Figma 官方远程 MCP — 读取设计文件、组件、图层，让 AI 理解并生成设计代码。",
        "https://mcp.figma.com/mcp",
        "https://github.com/figma/community-figma-mcp-server",
        "Authorization",
        "FIGMA_API_KEY",
    );
    let stripe_raw = remote_http_raw(
        "saas-stripe",
        "stripe",
        "Stripe 官方远程 MCP — 客户、支付、订阅、退款、发票等 Stripe API 工具。",
        "https://mcp.stripe.com/mcp",
        "https://docs.stripe.com/mcp",
        "Authorization",
        "STRIPE_SECRET_KEY",
    );
    vec![
        make_remote_curated(
            "saas-notion",
            "notion",
            "Notion 官方远程 MCP — 管理笔记、数据库、页面，AI 可读写你的 Notion 工作区。",
            &notion_raw,
            source,
            "https://github.com/makenotion/notion-mcp-server",
            "https://mcp.notion.com/mcp",
            "Authorization",
        ),
        make_remote_curated(
            "saas-figma",
            "figma",
            "Figma 官方远程 MCP — 读取设计文件、组件、图层，让 AI 理解并生成设计代码。",
            &figma_raw,
            source,
            "https://github.com/figma/community-figma-mcp-server",
            "https://mcp.figma.com/mcp",
            "Authorization",
        ),
        make_remote_curated(
            "saas-stripe",
            "stripe",
            "Stripe 官方远程 MCP — 客户、支付、订阅、退款、发票等 Stripe API 工具。",
            &stripe_raw,
            source,
            "https://docs.stripe.com/mcp",
            "https://mcp.stripe.com/mcp",
            "Authorization",
        ),
    ]
}

/// Extra commonly-used developer MCP servers (context7 / firecrawl).
pub(super) fn extra_curated_servers() -> Vec<McpRegistryServer> {
    let source = "cn-ai";
    let c7_repo = "https://github.com/upstash/context7";
    let c7_raw = stdio_npx_raw(
        "extra-context7",
        "context7",
        "Context7 MCP — 为 AI 提供最新版库/框架文档上下文，避免使用过时 API。",
        "@upstash/context7-mcp",
        c7_repo,
        // UPSTASH_API_KEY is optional for the free tier.
        &[],
    );
    let fc_repo = "https://github.com/firecrawl/firecrawl-mcp-server";
    let fc_raw = stdio_npx_raw(
        "extra-firecrawl",
        "firecrawl",
        "Firecrawl MCP — 抓取/爬取任意网站转为干净的 Markdown，供 AI 读取与分析。",
        "firecrawl-mcp-server",
        fc_repo,
        &[("FIRECRAWL_API_KEY", None, true)],
    );
    vec![
        make_stdio_curated(
            "extra-context7",
            "context7",
            "Context7 MCP — 为 AI 提供最新版库/框架文档上下文，避免使用过时 API。",
            &c7_raw,
            source,
            c7_repo,
            "@upstash/context7-mcp",
            &[],
        ),
        make_stdio_curated(
            "extra-firecrawl",
            "firecrawl",
            "Firecrawl MCP — 抓取/爬取任意网站转为干净的 Markdown，供 AI 读取与分析。",
            &fc_raw,
            source,
            fc_repo,
            "firecrawl-mcp-server",
            &["FIRECRAWL_API_KEY"],
        ),
    ]
}

/// Cloudflare remote MCP servers — official hosted endpoints.
pub(super) fn cloudflare_curated_servers() -> Vec<McpRegistryServer> {
    let source = "cloudflare";
    let repo = "https://github.com/cloudflare/mcp-server-cloudflare";
    let docs_raw = remote_http_raw(
        "cloudflare-docs",
        "Cloudflare Docs",
        "Cloudflare 官方文档 MCP — 查询 Cloudflare 全产品文档，获取配置示例与最佳实践。",
        "https://docs.mcp.cloudflare.com/sse",
        repo,
        "Authorization",
        "CLOUDFLARE_API_KEY",
    );
    let workers_raw = remote_http_raw(
        "cloudflare-workers",
        "Cloudflare Workers",
        "Cloudflare Workers MCP — 管理 Workers 部署、KV、D1、R2 等无服务器资源。",
        "https://bindings.mcp.cloudflare.com/sse",
        repo,
        "Authorization",
        "CLOUDFLARE_API_KEY",
    );
    let radar_raw = remote_http_raw(
        "cloudflare-radar",
        "Cloudflare Radar",
        "Cloudflare Radar MCP — 全球互联网流量分析、安全趋势与攻击洞察。",
        "https://radar.mcp.cloudflare.com/sse",
        repo,
        "Authorization",
        "CLOUDFLARE_API_KEY",
    );
    vec![
        make_remote_curated(
            "cloudflare-docs",
            "Cloudflare Docs",
            "Cloudflare 官方文档 MCP — 查询 Cloudflare 全产品文档，获取配置示例与最佳实践。",
            &docs_raw,
            source,
            repo,
            "https://docs.mcp.cloudflare.com/sse",
            "Authorization",
        ),
        make_remote_curated(
            "cloudflare-workers",
            "Cloudflare Workers",
            "Cloudflare Workers MCP — 管理 Workers 部署、KV、D1、R2 等无服务器资源。",
            &workers_raw,
            source,
            repo,
            "https://bindings.mcp.cloudflare.com/sse",
            "Authorization",
        ),
        make_remote_curated(
            "cloudflare-radar",
            "Cloudflare Radar",
            "Cloudflare Radar MCP — 全球互联网流量分析、安全趋势与攻击洞察。",
            &radar_raw,
            source,
            repo,
            "https://radar.mcp.cloudflare.com/sse",
            "Authorization",
        ),
    ]
}

/// Brave Search MCP server — official stdio/npx.
pub(super) fn brave_curated_servers() -> Vec<McpRegistryServer> {
    let source = "brave";
    let repo = "https://github.com/brave/brave-search-mcp";
    let raw = stdio_npx_raw(
        "brave-search",
        "Brave Search",
        "Brave 官方搜索 MCP — 通过 Brave Search API 提供 Web 搜索与本地商户搜索能力。",
        "@modelcontextprotocol/server-brave-search",
        repo,
        &[("BRAVE_API_KEY", None, true)],
    );
    vec![make_stdio_curated(
        "brave-search",
        "Brave Search",
        "Brave 官方搜索 MCP — 通过 Brave Search API 提供 Web 搜索与本地商户搜索能力。",
        &raw,
        source,
        repo,
        "@modelcontextprotocol/server-brave-search",
        &["BRAVE_API_KEY"],
    )]
}

/// Google official MCP servers — Drive / Maps remote endpoints.
pub(super) fn google_curated_servers() -> Vec<McpRegistryServer> {
    let source = "google";
    let drive_repo = "https://github.com/modelcontextprotocol/servers";
    let drive_raw = remote_http_raw(
        "google-drive",
        "Google Drive",
        "Google Drive 官方 MCP — 搜索、读取、创建 Google Drive 文件与文件夹。",
        "https://mcp.drive.google.com/sse",
        drive_repo,
        "Authorization",
        "GOOGLE_ACCESS_TOKEN",
    );
    let maps_repo = "https://github.com/modelcontextprotocol/servers";
    let maps_raw = stdio_npx_raw(
        "google-maps",
        "Google Maps",
        "Google Maps 官方 MCP — 地点搜索、路线规划、距离计算、地理编码等地图能力。",
        "@modelcontextprotocol/server-google-maps",
        maps_repo,
        &[("GOOGLE_MAPS_API_KEY", None, true)],
    );
    vec![
        make_remote_curated(
            "google-drive",
            "Google Drive",
            "Google Drive 官方 MCP — 搜索、读取、创建 Google Drive 文件与文件夹。",
            &drive_raw,
            source,
            drive_repo,
            "https://mcp.drive.google.com/sse",
            "Authorization",
        ),
        make_stdio_curated(
            "google-maps",
            "Google Maps",
            "Google Maps 官方 MCP — 地点搜索、路线规划、距离计算、地理编码等地图能力。",
            &maps_raw,
            source,
            maps_repo,
            "@modelcontextprotocol/server-google-maps",
            &["GOOGLE_MAPS_API_KEY"],
        ),
    ]
}

/// Supabase MCP server — official stdio/npx for database & auth management.
pub(super) fn supabase_curated_servers() -> Vec<McpRegistryServer> {
    let source = "supabase";
    let repo = "https://github.com/supabase/mcp-server-supabase";
    let raw = stdio_npx_raw(
        "supabase-mcp",
        "Supabase",
        "Supabase 官方 MCP — 管理 Postgres 数据库、表结构、RLS 策略、Auth 用户与存储。",
        "@supabase/mcp-server-supabase",
        repo,
        &[("SUPABASE_ACCESS_TOKEN", None, true)],
    );
    vec![make_stdio_curated(
        "supabase-mcp",
        "Supabase",
        "Supabase 官方 MCP — 管理 Postgres 数据库、表结构、RLS 策略、Auth 用户与存储。",
        &raw,
        source,
        repo,
        "@supabase/mcp-server-supabase",
        &["SUPABASE_ACCESS_TOKEN"],
    )]
}

/// X (Twitter) official MCP servers — the `xurl` stdio bridge to the X API
/// plus the no-auth remote docs server. `xapi` is hand-built rather than via
/// `make_stdio_curated` because it needs `package_arguments` (`mcp <url>`)
/// appended after the npm identifier, which the generic helper can't express.
pub(super) fn x_curated_servers() -> Vec<McpRegistryServer> {
    let source = "x";
    let repo = "https://docs.x.com/tools/mcp";

    let xapi_desc = "X 官方 MCP — 通过 xurl 桥接 X API：发帖、搜索、用户/时间线、书签、趋势等（首次需浏览器 OAuth 登录）。";
    let xapi_raw = r##"{
        "id": "x-api",
        "name": "xapi",
        "description": "X 官方 MCP — 通过 xurl 桥接 X API：发帖、搜索、用户/时间线、书签、趋势等（首次需浏览器 OAuth 登录）。",
        "packages": [
            {
                "registry_type": "npm",
                "identifier": "@xdevplatform/xurl",
                "runtime_hint": "npx",
                "package_arguments": ["mcp", "https://api.x.com/mcp"],
                "environment_variables": [
                    { "name": "CLIENT_ID", "is_secret": true, "is_required": true },
                    { "name": "CLIENT_SECRET", "is_secret": true, "is_required": true }
                ]
            }
        ],
        "remotes": [],
        "repository": { "url": "https://docs.x.com/tools/mcp", "source": "github" }
    }"##;

    let xapi = McpRegistryServer {
        id: "x-api".to_string(),
        name: "xapi".to_string(),
        namespace: "x-api".to_string(),
        description: xapi_desc.to_string(),
        repo_url: repo.to_string(),
        stars: 0,
        license: None,
        version: None,
        kind: McpServerKind::Stdio,
        runtimes: vec!["npx".to_string()],
        readme: Some(format!("# xapi\n\n{xapi_desc}")),
        updated_at: None,
        packages: vec![McpRegistryPackageSummary {
            runtime: "npx".to_string(),
            identifier: "@xdevplatform/xurl".to_string(),
            version: None,
            required_env: vec!["CLIENT_ID".to_string(), "CLIENT_SECRET".to_string()],
            registry_type: Some("npm".to_string()),
            runtime_hint: Some("npx".to_string()),
            ..Default::default()
        }],
        remotes: Vec::new(),
        raw_server_json: xapi_raw.to_string(),
        recommended: false,
        source: Some(source.to_string()),
        ..Default::default()
    };

    let docs_desc =
        "X 官方文档 MCP — search_x / get_page_x 工具，检索 X API 指南与示例（无需鉴权）。";
    let docs_raw = r##"{
        "id": "x-docs",
        "name": "x-docs",
        "description": "X 官方文档 MCP — search_x / get_page_x 工具，检索 X API 指南与示例（无需鉴权）。",
        "packages": [],
        "remotes": [
            { "transport_type": "streamable-http", "url": "https://docs.x.com/mcp" }
        ],
        "repository": { "url": "https://docs.x.com/tools/mcp", "source": "github" }
    }"##;

    let docs = McpRegistryServer {
        id: "x-docs".to_string(),
        name: "x-docs".to_string(),
        namespace: "x-docs".to_string(),
        description: docs_desc.to_string(),
        repo_url: repo.to_string(),
        stars: 0,
        license: None,
        version: None,
        kind: McpServerKind::Remote,
        runtimes: Vec::new(),
        readme: Some(format!("# x-docs\n\n{docs_desc}")),
        updated_at: None,
        packages: Vec::new(),
        remotes: vec![McpRegistryRemoteSummary {
            transport: "http".to_string(),
            url: "https://docs.x.com/mcp".to_string(),
            required_headers: Vec::new(),
            transport_type: Some("streamable-http".to_string()),
            ..Default::default()
        }],
        raw_server_json: docs_raw.to_string(),
        recommended: false,
        source: Some(source.to_string()),
        ..Default::default()
    };

    vec![xapi, docs]
}

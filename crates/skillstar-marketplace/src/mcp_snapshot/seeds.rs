//! Curated MCP server seed data, split out of `mcp_snapshot` to keep the
//! snapshot module focused on schema + DB ops.
//!
//! Each `*_curated_servers()` fn returns the official MCP servers for one
//! publisher; `default_curated_mcp_servers()` aggregates them into the
//! priority-ordered seed list that `seed_default_curated_mcp_servers` writes
//! into `mcp_curated_server`.
//!
//! `raw_server_json` is hand-built to mirror the GitHub registry server.json
//! shape so the existing `registry_to_entry` install path works unchanged;
//! the top-level `McpRegistryServer` fields mirror the same content for
//! card display + detail rendering.

use crate::mcp_models::{
    McpRegistryPackageSummary, McpRegistryRemoteSummary, McpRegistryServer, McpServerKind,
};

pub(super) struct CuratedMcpSeed {
    pub(super) priority: i64,
    pub(super) server: McpRegistryServer,
}

pub(super) fn default_curated_mcp_servers() -> Vec<CuratedMcpSeed> {
    // The `source` column doubles as the MCP official-publisher id
    // ("adspower" / "bigmodel") so the publishers grid can GROUP BY it.
    // GitHub registry rows carry no `source` and form their own publisher.
    let mut seeds: Vec<CuratedMcpSeed> = Vec::new();

    // ── AdsPower (source = "adspower") ──────────────────────────────────
    let adspower_description =
        "AdsPower 浏览器 Local API — 通过 MCP 控制指纹浏览器 / 自动化".to_string();
    let adspower_repo_url = "https://github.com/AdsPower/adspower-browser".to_string();
    let adspower_raw = r##"{
        "id": "adspower-local-api",
        "name": "adspower-local-api",
        "description": "AdsPower 浏览器 Local API — 通过 MCP 控制指纹浏览器 / 自动化",
        "packages": [
            {
                "registry_type": "npm",
                "identifier": "local-api-mcp-typescript",
                "runtime_hint": "npx",
                "environment_variables": [
                    { "name": "PORT", "default": "50325" },
                    { "name": "API_KEY", "is_secret": true, "is_required": true }
                ]
            }
        ],
        "remotes": [],
        "repository": {
            "url": "https://github.com/AdsPower/adspower-browser",
            "source": "github",
            "readme": "# adspower-local-api\n\nAdsPower 浏览器 Local API — 通过 MCP 控制指纹浏览器 / 自动化。"
        }
    }"##
    .to_string();
    seeds.push(CuratedMcpSeed {
        priority: 0,
        server: McpRegistryServer {
            id: "adspower-local-api".to_string(),
            name: "adspower-local-api".to_string(),
            namespace: "adspower-local-api".to_string(),
            description: adspower_description,
            repo_url: adspower_repo_url,
            stars: 0,
            license: None,
            version: None,
            kind: McpServerKind::Stdio,
            runtimes: vec!["npx".to_string()],
            readme: Some(
                "# adspower-local-api\n\nAdsPower 浏览器 Local API — 通过 MCP 控制指纹浏览器 / 自动化。"
                    .to_string(),
            ),
            updated_at: None,
            packages: vec![McpRegistryPackageSummary {
                runtime: "npx".to_string(),
                identifier: "local-api-mcp-typescript".to_string(),
                version: None,
                required_env: vec!["API_KEY".to_string()],
            }],
            remotes: Vec::new(),
            raw_server_json: adspower_raw,
            recommended: true,
            source: Some("adspower".to_string()),
        },
    });

    // ── BigModel / 智谱 (source = "bigmodel") ───────────────────────────
    // Four official MCP servers from https://docs.bigmodel.cn/cn/coding-plan/mcp/.
    // The `raw_server_json` mirrors the GitHub registry server.json shape so the
    // existing `registry_to_entry` install path works unchanged.
    let bigmodel_repo_url = "https://docs.bigmodel.cn/cn/coding-plan/mcp/".to_string();
    for (idx, bigmodel) in bigmodel_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: bigmodel,
        });
    }

    // ── Anthropic (source = "anthropic") ────────────────────────────────
    for (idx, srv) in anthropic_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: srv,
        });
    }

    // ── Microsoft (source = "microsoft") ────────────────────────────────
    for (idx, srv) in microsoft_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: srv,
        });
    }

    // ── SaaS brands: Notion / Figma / Stripe (source = "saas") ──────────
    for (idx, srv) in saas_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: srv,
        });
    }

    // ── Extra dev tools: Context7 / Firecrawl (source = "cn-ai") ────────
    for (idx, srv) in extra_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: srv,
        });
    }

    // ── Cloudflare (source = "cloudflare") ──────────────────────────────
    for (idx, srv) in cloudflare_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: srv,
        });
    }

    // ── Brave (source = "brave") ────────────────────────────────────────
    for (idx, srv) in brave_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: srv,
        });
    }

    // ── Google (source = "google") ──────────────────────────────────────
    for (idx, srv) in google_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: srv,
        });
    }

    // ── Supabase (source = "supabase") ──────────────────────────────────
    for (idx, srv) in supabase_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: srv,
        });
    }

    // Helper-free closure to build BigModel servers lives below; keep
    // `bigmodel_repo_url` referenced via the loop so a later tweak to the
    // repo url only touches one place.
    let _ = &bigmodel_repo_url;
    seeds
}

/// The four BigModel (智谱) official MCP servers. Each entry is authored to
/// feed straight into `registry_to_entry`:
/// - `raw_server_json` carries the `packages`/`remotes`/`environment_variables`
///   shape the registry parser already understands.
/// - top-level `McpRegistryServer` fields mirror the same content for card
///   display + detail rendering.
fn bigmodel_curated_servers() -> Vec<McpRegistryServer> {
    let bigmodel_source = "bigmodel".to_string();
    let repo_url = "https://docs.bigmodel.cn/cn/coding-plan/mcp/".to_string();
    let make = |id: &str, name: &str, description: &str, raw: &str, kind: McpServerKind| {
        McpRegistryServer {
            id: id.to_string(),
            name: name.to_string(),
            namespace: id.to_string(),
            description: description.to_string(),
            repo_url: repo_url.clone(),
            stars: 0,
            license: None,
            version: None,
            kind,
            runtimes: if kind == McpServerKind::Stdio {
                vec!["npx".to_string()]
            } else {
                Vec::new()
            },
            readme: Some(format!("# {name}\n\n{description}")),
            updated_at: None,
            packages: if kind == McpServerKind::Stdio {
                vec![McpRegistryPackageSummary {
                    runtime: "npx".to_string(),
                    identifier: "@z_ai/mcp-server".to_string(),
                    version: None,
                    required_env: vec!["Z_AI_API_KEY".to_string()],
                }]
            } else {
                Vec::new()
            },
            remotes: if kind == McpServerKind::Remote {
                vec![McpRegistryRemoteSummary {
                    transport: "http".to_string(),
                    url: bigmodel_remote_url(id),
                    required_headers: vec!["Authorization".to_string()],
                }]
            } else {
                Vec::new()
            },
            raw_server_json: raw.to_string(),
            recommended: false,
            source: Some(bigmodel_source.clone()),
        }
    };

    // bigmodel-vision — stdio / npx @z_ai/mcp-server
    let vision_raw = r##"{
        "id": "bigmodel-vision",
        "name": "bigmodel-vision",
        "description": "智谱视觉理解 MCP — 让模型看懂图片/截图/界面，OCR、图表、UI 理解与提取。",
        "packages": [
            {
                "registry_type": "npm",
                "identifier": "@z_ai/mcp-server",
                "runtime_hint": "npx",
                "environment_variables": [
                    { "name": "Z_AI_API_KEY", "is_secret": true, "is_required": true }
                ]
            }
        ],
        "remotes": [],
        "repository": { "url": "https://docs.bigmodel.cn/cn/coding-plan/mcp/vision-mcp-server", "source": "github" }
    }"##
    .to_string();

    // bigmodel-search / reader / zread — remote http endpoints, header auth.
    let search_remote_url = "https://open.bigmodel.cn/api/mcp/web_search_prime/mcp";
    let reader_remote_url = "https://open.bigmodel.cn/api/mcp/web_reader/mcp";
    let zread_remote_url = "https://open.bigmodel.cn/api/mcp/zread/mcp";
    let remote_raw = |id: &str, name: &str, description: &str, url: &str| -> String {
        format!(
            r##"{{
            "id": "{id}",
            "name": "{name}",
            "description": "{description}",
            "packages": [],
            "remotes": [
                {{
                    "transport_type": "streamable-http",
                    "url": "{url}",
                    "headers": [
                        {{ "name": "Authorization", "value": "Bearer {{Z_AI_API_KEY}}", "is_secret": true, "is_required": true }}
                    ]
                }}
            ],
            "repository": {{ "url": "https://docs.bigmodel.cn/cn/coding-plan/mcp/", "source": "github" }}
        }}"##
        )
    };

    vec![
        make(
            "bigmodel-vision",
            "bigmodel-vision",
            "智谱视觉理解 MCP — 让模型看懂图片/截图/界面，OCR、图表、UI 理解与提取。",
            &vision_raw,
            McpServerKind::Stdio,
        ),
        make(
            "bigmodel-search",
            "bigmodel-search",
            "智谱联网搜索 MCP — webSearchPrime 工具，返回网页标题、URL、摘要、来源等结构化结果。",
            &remote_raw(
                "bigmodel-search",
                "bigmodel-search",
                "智谱联网搜索 MCP — webSearchPrime 工具，返回网页标题、URL、摘要、来源等结构化结果。",
                search_remote_url,
            ),
            McpServerKind::Remote,
        ),
        make(
            "bigmodel-reader",
            "bigmodel-reader",
            "智谱网页读取 MCP — webReader 工具，抓取 URL 页面，返回标题、正文、元数据、链接等。",
            &remote_raw(
                "bigmodel-reader",
                "bigmodel-reader",
                "智谱网页读取 MCP — webReader 工具，抓取 URL 页面，返回标题、正文、元数据、链接等。",
                reader_remote_url,
            ),
            McpServerKind::Remote,
        ),
        make(
            "bigmodel-zread",
            "bigmodel-zread",
            "智谱开源仓库 MCP — 搜索 GitHub 仓库知识文档，快速了解 README、issue、PR 与贡献者。",
            &remote_raw(
                "bigmodel-zread",
                "bigmodel-zread",
                "智谱开源仓库 MCP — 搜索 GitHub 仓库知识文档，快速了解 README、issue、PR 与贡献者。",
                zread_remote_url,
            ),
            McpServerKind::Remote,
        ),
    ]
}

/// Map a BigModel curated server id to its remote MCP endpoint URL.
fn bigmodel_remote_url(id: &str) -> String {
    match id {
        "bigmodel-search" => "https://open.bigmodel.cn/api/mcp/web_search_prime/mcp".to_string(),
        "bigmodel-reader" => "https://open.bigmodel.cn/api/mcp/web_reader/mcp".to_string(),
        "bigmodel-zread" => "https://open.bigmodel.cn/api/mcp/zread/mcp".to_string(),
        _ => String::new(),
    }
}

// ── Additional curated publishers ───────────────────────────────────────
// Each factory mirrors the BigModel pattern: a `make` closure builds a
// `McpRegistryServer` with a `raw_server_json` in the GitHub registry
// server.json shape, so `registry_to_entry` installs them unchanged.

/// Build a stdio (npx) curated server. The `raw` must carry a `packages`
/// entry with `registry_type: "npm"` + `runtime_hint: "npx"`.
fn make_stdio_curated(
    id: &str,
    name: &str,
    description: &str,
    raw: &str,
    source: &str,
    repo_url: &str,
    npm_identifier: &str,
    required_env: &[&str],
) -> McpRegistryServer {
    McpRegistryServer {
        id: id.to_string(),
        name: name.to_string(),
        namespace: id.to_string(),
        description: description.to_string(),
        repo_url: repo_url.to_string(),
        stars: 0,
        license: None,
        version: None,
        kind: McpServerKind::Stdio,
        runtimes: vec!["npx".to_string()],
        readme: Some(format!("# {name}\n\n{description}")),
        updated_at: None,
        packages: vec![McpRegistryPackageSummary {
            runtime: "npx".to_string(),
            identifier: npm_identifier.to_string(),
            version: None,
            required_env: required_env.iter().map(|s| s.to_string()).collect(),
        }],
        remotes: Vec::new(),
        raw_server_json: raw.to_string(),
        recommended: false,
        source: Some(source.to_string()),
    }
}

/// Build a remote (streamable-http) curated server. The `raw` must carry a
/// `remotes` entry with `transport_type: "streamable-http"`.
fn make_remote_curated(
    id: &str,
    name: &str,
    description: &str,
    raw: &str,
    source: &str,
    repo_url: &str,
    url: &str,
    auth_header: &str,
) -> McpRegistryServer {
    McpRegistryServer {
        id: id.to_string(),
        name: name.to_string(),
        namespace: id.to_string(),
        description: description.to_string(),
        repo_url: repo_url.to_string(),
        stars: 0,
        license: None,
        version: None,
        kind: McpServerKind::Remote,
        runtimes: Vec::new(),
        readme: Some(format!("# {name}\n\n{description}")),
        updated_at: None,
        packages: Vec::new(),
        remotes: vec![McpRegistryRemoteSummary {
            transport: "http".to_string(),
            url: url.to_string(),
            required_headers: vec![auth_header.to_string()],
        }],
        raw_server_json: raw.to_string(),
        recommended: false,
        source: Some(source.to_string()),
    }
}

/// Raw JSON template for a stdio npx server with optional env vars.
fn stdio_npx_raw(
    id: &str,
    name: &str,
    description: &str,
    npm_pkg: &str,
    repo_url: &str,
    env_vars: &[( &str, Option<&str>, bool )], // (name, default, is_required_secret)
) -> String {
    let envs: Vec<String> = env_vars
        .iter()
        .map(|(n, def, secret)| {
            let def_clause = def
                .map(|d| format!(", \"default\": \"{d}\""))
                .unwrap_or_default();
            format!(
                "{{ \"name\": \"{n}\", \"is_secret\": {secret}, \"is_required\": {secret}{def_clause} }}",
                secret = secret,
            )
        })
        .collect();
    format!(
        r##"{{
        "id": "{id}",
        "name": "{name}",
        "description": "{description}",
        "packages": [
            {{
                "registry_type": "npm",
                "identifier": "{npm_pkg}",
                "runtime_hint": "npx",
                "environment_variables": [{envs}]
            }}
        ],
        "remotes": [],
        "repository": {{ "url": "{repo_url}", "source": "github" }}
    }}"##,
        envs = envs.join(", "),
    )
}

/// Raw JSON template for a remote streamable-http server with a bearer header.
fn remote_http_raw(
    id: &str,
    name: &str,
    description: &str,
    url: &str,
    repo_url: &str,
    header_name: &str,
    token_env: &str,
) -> String {
    format!(
        r##"{{
        "id": "{id}",
        "name": "{name}",
        "description": "{description}",
        "packages": [],
        "remotes": [
            {{
                "transport_type": "streamable-http",
                "url": "{url}",
                "headers": [
                    {{ "name": "{header_name}", "value": "Bearer {{{token_env}}}", "is_secret": true, "is_required": true }}
                ]
            }}
        ],
        "repository": {{ "url": "{repo_url}", "source": "github" }}
    }}"##
    )
}

/// Anthropic official reference MCP servers (modelcontextprotocol/servers).
/// All four are stdio/npx under the `@modelcontextprotocol/` npm scope.
fn anthropic_curated_servers() -> Vec<McpRegistryServer> {
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
fn microsoft_curated_servers() -> Vec<McpRegistryServer> {
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
fn saas_curated_servers() -> Vec<McpRegistryServer> {
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
fn extra_curated_servers() -> Vec<McpRegistryServer> {
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
fn cloudflare_curated_servers() -> Vec<McpRegistryServer> {
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
fn brave_curated_servers() -> Vec<McpRegistryServer> {
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
fn google_curated_servers() -> Vec<McpRegistryServer> {
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
fn supabase_curated_servers() -> Vec<McpRegistryServer> {
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


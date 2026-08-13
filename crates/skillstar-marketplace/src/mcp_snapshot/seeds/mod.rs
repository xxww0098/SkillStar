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

mod bigmodel;
mod helpers;
mod publishers;

use crate::mcp_models::{McpRegistryPackageSummary, McpRegistryServer, McpServerKind};

use bigmodel::bigmodel_curated_servers;
use publishers::{
    anthropic_curated_servers, brave_curated_servers, cloudflare_curated_servers,
    extra_curated_servers, google_curated_servers, microsoft_curated_servers, saas_curated_servers,
    supabase_curated_servers, x_curated_servers,
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
                registry_type: Some("npm".to_string()),
                runtime_hint: Some("npx".to_string()),
                ..Default::default()
            }],
            remotes: Vec::new(),
            raw_server_json: adspower_raw,
            recommended: true,
            source: Some("adspower".to_string()),
            ..Default::default()
        },
    });

    // ── BigModel / 智谱 (source = "bigmodel") ───────────────────────────
    // Four official MCP servers from https://docs.bigmodel.cn/cn/coding-plan/mcp/.
    // The `raw_server_json` mirrors the GitHub registry server.json shape so the
    // existing `registry_to_entry` install path works unchanged.
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

    // ── X / Twitter (source = "x") ──────────────────────────────────────
    for (idx, srv) in x_curated_servers().into_iter().enumerate() {
        seeds.push(CuratedMcpSeed {
            priority: idx as i64,
            server: srv,
        });
    }

    seeds
}

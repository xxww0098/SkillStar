//! BigModel (智谱) curated MCP server seeds.

use crate::mcp_models::{
    McpRegistryPackageSummary, McpRegistryRemoteSummary, McpRegistryServer, McpServerKind,
};

/// The four BigModel (智谱) official MCP servers. Each entry is authored to
/// feed straight into `registry_to_entry`:
/// - `raw_server_json` carries the `packages`/`remotes`/`environment_variables`
///   shape the registry parser already understands.
/// - top-level `McpRegistryServer` fields mirror the same content for card
///   display + detail rendering.
pub(super) fn bigmodel_curated_servers() -> Vec<McpRegistryServer> {
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
                    registry_type: Some("npm".to_string()),
                    runtime_hint: Some("npx".to_string()),
                    ..Default::default()
                }]
            } else {
                Vec::new()
            },
            remotes: if kind == McpServerKind::Remote {
                vec![McpRegistryRemoteSummary {
                    transport: "http".to_string(),
                    url: bigmodel_remote_url(id),
                    required_headers: vec!["Authorization".to_string()],
                    transport_type: Some("streamable-http".to_string()),
                    ..Default::default()
                }]
            } else {
                Vec::new()
            },
            raw_server_json: raw.to_string(),
            recommended: false,
            source: Some(bigmodel_source.clone()),
            ..Default::default()
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

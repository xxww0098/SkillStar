use crate::formats::encode_project_snippet;
use crate::model::{McpFormat, McpServer, ProjectSnippet};
use std::path::Path;

/// Specification of one project-level MCP file format (matching magpie `project_mcp.go`).
#[derive(Debug, Clone)]
pub struct ProjectTargetSpec {
    pub agent_id: &'static str,
    pub agent_name: &'static str,
    pub rel_path: &'static str,
    pub alt_rel_path: Option<&'static str>,
    pub format: McpFormat,
    pub no_cmd_refs: bool,
}

static PROJECT_TARGET_LIST: &[ProjectTargetSpec] = &[
    ProjectTargetSpec {
        agent_id: "claude",
        agent_name: "Claude Code",
        rel_path: ".mcp.json",
        alt_rel_path: None,
        format: McpFormat::Claude,
        no_cmd_refs: false,
    },
    ProjectTargetSpec {
        agent_id: "codex",
        agent_name: "Codex",
        rel_path: ".codex/config.toml",
        alt_rel_path: None,
        format: McpFormat::Codex,
        no_cmd_refs: false,
    },
    ProjectTargetSpec {
        agent_id: "cursor",
        agent_name: "Cursor",
        rel_path: ".cursor/mcp.json",
        alt_rel_path: None,
        format: McpFormat::Cursor,
        no_cmd_refs: false,
    },
    ProjectTargetSpec {
        agent_id: "opencode",
        agent_name: "OpenCode",
        rel_path: "opencode.json",
        alt_rel_path: Some("opencode.jsonc"),
        format: McpFormat::OpenCode,
        no_cmd_refs: false,
    },
    ProjectTargetSpec {
        agent_id: "zcode",
        agent_name: "ZCode",
        rel_path: ".zcode/config.json",
        alt_rel_path: None,
        format: McpFormat::ZCode,
        no_cmd_refs: false,
    },
    ProjectTargetSpec {
        agent_id: "pi",
        agent_name: "Pi",
        rel_path: ".pi/mcp.json",
        alt_rel_path: None,
        format: McpFormat::PiNative,
        no_cmd_refs: false,
    },
    ProjectTargetSpec {
        agent_id: "qoder",
        agent_name: "Qoder",
        rel_path: ".qoder/settings.json",
        alt_rel_path: None,
        format: McpFormat::Omp,
        no_cmd_refs: false,
    },
    ProjectTargetSpec {
        agent_id: "qoder-cn",
        agent_name: "Qoder CN",
        rel_path: ".qoder/settings.json",
        alt_rel_path: None,
        format: McpFormat::Omp,
        no_cmd_refs: false,
    },
    ProjectTargetSpec {
        agent_id: "codebuddy",
        agent_name: "CodeBuddy",
        rel_path: ".mcp.json",
        alt_rel_path: Some("mcp.json"),
        format: McpFormat::Claude,
        no_cmd_refs: false,
    },
    ProjectTargetSpec {
        agent_id: "droid",
        agent_name: "Droid",
        rel_path: ".factory/mcp.json",
        alt_rel_path: None,
        format: McpFormat::Claude,
        no_cmd_refs: true,
    },
];

/// Return all 10 project-level MCP snippet specs.
pub fn project_targets() -> &'static [ProjectTargetSpec] {
    PROJECT_TARGET_LIST
}

/// Generate project-level MCP configuration previews for selected agents (or all 10 if empty).
pub fn build_project_snippets(
    project_root: Option<&Path>,
    servers: &[McpServer],
    agent_filter: &[String],
) -> Vec<ProjectSnippet> {
    if !servers.iter().any(|s| !s.paused) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for spec in PROJECT_TARGET_LIST {
        if !agent_filter.is_empty() && !agent_filter.iter().any(|a| a == spec.agent_id) {
            continue;
        }
        let chosen_rel = match (project_root, spec.alt_rel_path) {
            (Some(root), Some(alt)) if !root.join(spec.rel_path).exists() && root.join(alt).exists() => alt,
            _ => spec.rel_path,
        };
        let active: Vec<&McpServer> = servers
            .iter()
            .filter(|s| {
                !s.paused
                    && (s.agents.is_empty() || s.agents.iter().any(|a| a == spec.agent_id))
            })
            .collect();
        let content = encode_project_snippet(spec.format, spec.no_cmd_refs, &active);
        out.push(ProjectSnippet {
            agent_id: spec.agent_id.to_string(),
            agent_name: spec.agent_name.to_string(),
            rel_path: chosen_rel.to_string(),
            format: spec.format,
            content,
        });
    }
    out
}

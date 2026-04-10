use std::path::PathBuf;

use super::types::AgentCliInfo;

/// Supported CLI agents (desktop apps are explicitly excluded).
pub(crate) const AGENT_CLIS: &[(&str, &str, &str)] = &[
    ("claude", "Claude Code", "claude"),
    ("codex", "Codex CLI", "codex"),
    ("opencode", "OpenCode", "opencode"),
    ("gemini", "Gemini CLI", "gemini"),
];

pub(crate) fn binary_name_for_agent(agent_id: &str) -> Option<&'static str> {
    AGENT_CLIS
        .iter()
        .find(|(id, _, _)| *id == agent_id)
        .map(|(_, _, binary)| *binary)
}

/// Find the binary path for a given agent id.
pub(crate) fn find_cli_binary(agent_id: &str) -> Option<PathBuf> {
    let binary_name = binary_name_for_agent(agent_id)?;
    which::which(binary_name).ok()
}

/// List all known agent CLIs with their installation status.
pub(crate) fn list_available_clis() -> Vec<AgentCliInfo> {
    AGENT_CLIS
        .iter()
        .map(|(id, name, binary)| {
            let path = which::which(binary).ok();
            AgentCliInfo {
                id: id.to_string(),
                name: name.to_string(),
                binary: binary.to_string(),
                installed: path.is_some(),
                path: path.map(|p| p.to_string_lossy().to_string()),
            }
        })
        .collect()
}

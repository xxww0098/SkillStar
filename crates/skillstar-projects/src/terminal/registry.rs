use std::path::PathBuf;

use crate::projects::agents::{binary_name_for_builtin, builtin_cli_entries};

use super::types::AgentCliInfo;

/// A terminal-launchable CLI agent entry: `(id, display_name, binary)`.
///
/// Derived from the builtin agent table — any builtin agent whose `binary` is
/// set is considered a terminal-launchable CLI. This keeps the builtin table
/// the single source of truth for binary names (so it can't drift out of sync
/// with install detection in `agents::detect`). Desktop-app-only agents
/// (Antigravity, Cursor, …) are excluded because they have no terminal binary.
pub fn agent_cli_entries() -> Vec<(&'static str, &'static str, &'static str)> {
    builtin_cli_entries()
}

/// Resolve the CLI binary name for a given agent id, if it has one.
pub fn binary_name_for_agent(agent_id: &str) -> Option<&'static str> {
    binary_name_for_builtin(agent_id)
}

/// Find the binary path for a given agent id by searching PATH.
pub fn find_cli_binary(agent_id: &str) -> Option<PathBuf> {
    let binary_name = binary_name_for_agent(agent_id)?;
    which::which(binary_name).ok()
}

/// List all terminal-launchable CLI agents with their installation status.
pub fn list_available_clis() -> Vec<AgentCliInfo> {
    agent_cli_entries()
        .into_iter()
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

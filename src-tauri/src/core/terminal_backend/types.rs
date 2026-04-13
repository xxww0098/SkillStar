use serde::{Deserialize, Serialize};

/// Metadata about an agent CLI binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCliInfo {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub installed: bool,
    pub path: Option<String>,
}

/// tmux availability status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmuxStatus {
    pub installed: bool,
    pub version: Option<String>,
}

/// Launch script runtime type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LaunchScriptKind {
    Bash,
    PowerShell,
}

/// Result of a deploy operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub success: bool,
    pub message: String,
    pub script_path: Option<String>,
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCliInfo {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub installed: bool,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LaunchScriptKind {
    Bash,
    PowerShell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub success: bool,
    pub message: String,
    pub script_path: Option<String>,
}

use skillstar_infra::paths::acp_config_path;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AcpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_agent_command")]
    pub agent_command: String,
    #[serde(default = "default_agent_label")]
    pub agent_label: String,
}

fn default_agent_command() -> String {
    "npx -y @agentclientprotocol/claude-agent-acp".to_string()
}

fn default_agent_label() -> String {
    "Claude Code".to_string()
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            agent_command: default_agent_command(),
            agent_label: default_agent_label(),
        }
    }
}

fn config_path() -> PathBuf {
    acp_config_path()
}

pub fn load_config() -> AcpConfig {
    let p = config_path();
    match std::fs::read_to_string(&p) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AcpConfig::default(),
    }
}

pub fn save_config(config: &AcpConfig) -> anyhow::Result<()> {
    let p = config_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&p, content)?;
    Ok(())
}

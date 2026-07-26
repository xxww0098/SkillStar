use crate::infra::paths::acp_config_path;
use std::path::PathBuf;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TutorialStyle {
    /// Progressive explanations optimized for first-time users.
    #[default]
    Guided,
    /// Dense, lookup-oriented technical documentation.
    Reference,
    /// Task-driven exercises with checkpoints and recovery paths.
    Workshop,
}

impl TutorialStyle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Guided => "guided",
            Self::Reference => "reference",
            Self::Workshop => "workshop",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AcpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_agent_command")]
    pub agent_command: String,
    #[serde(default = "default_agent_label")]
    pub agent_label: String,
    #[serde(default)]
    pub tutorial_style: TutorialStyle,
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
            tutorial_style: TutorialStyle::default(),
        }
    }
}

fn config_path() -> PathBuf {
    acp_config_path()
}

pub fn load_config() -> AcpConfig {
    let p = config_path();
    let mut config = match std::fs::read_to_string(&p) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AcpConfig::default(),
    };
    normalize_builtin_label(&mut config);
    config
}

fn normalize_builtin_label(config: &mut AcpConfig) {
    if config.agent_command == default_agent_command() && config.agent_label == "Claude" {
        config.agent_label = default_agent_label();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_legacy_claude_label_only_for_builtin_command() {
        let mut builtin = AcpConfig {
            enabled: true,
            agent_command: default_agent_command(),
            agent_label: "Claude".to_string(),
            tutorial_style: TutorialStyle::Guided,
        };
        normalize_builtin_label(&mut builtin);
        assert_eq!(builtin.agent_label, "Claude Code");

        let mut custom = AcpConfig {
            enabled: true,
            agent_command: "custom-acp".to_string(),
            agent_label: "Claude".to_string(),
            tutorial_style: TutorialStyle::Guided,
        };
        normalize_builtin_label(&mut custom);
        assert_eq!(custom.agent_label, "Claude");
    }

    #[test]
    fn legacy_config_defaults_to_guided_tutorial_style() {
        let config: AcpConfig = serde_json::from_str(
            r#"{"enabled":true,"agent_command":"custom-acp","agent_label":"Custom"}"#,
        )
        .unwrap();

        assert_eq!(config.tutorial_style, TutorialStyle::Guided);
    }

    #[test]
    fn tutorial_style_uses_stable_snake_case_ids() {
        assert_eq!(
            serde_json::to_string(&TutorialStyle::Reference).unwrap(),
            r#""reference""#
        );
        assert_eq!(
            serde_json::from_str::<TutorialStyle>(r#""workshop""#).unwrap(),
            TutorialStyle::Workshop
        );
    }
}

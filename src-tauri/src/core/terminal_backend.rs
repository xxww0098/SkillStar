//! Terminal backend: CLI detection, script generation, terminal launch, and deploy orchestration.

use anyhow::{Context, Result};
use std::path::PathBuf;

use super::terminal::config::{LaunchConfig, LayoutNode, deployable_layout};

mod pane_command;
mod provider_env;
mod registry;
mod script_builder;
mod session;
mod terminal_launcher;
mod types;

pub use types::{AgentCliInfo, DeployResult, LaunchScriptKind};

/// Find the binary path for a given agent id.
pub fn find_cli_binary(agent_id: &str) -> Option<PathBuf> {
    registry::find_cli_binary(agent_id)
}

/// List all known agent CLIs with installation status.
pub fn list_available_clis() -> Vec<AgentCliInfo> {
    registry::list_available_clis()
}

/// Open a launch script in the user's preferred terminal emulator.
pub fn open_script_in_terminal_with_kind(
    script_path: &std::path::Path,
    script_kind: LaunchScriptKind,
) -> Result<()> {
    terminal_launcher::open_script_in_terminal_with_kind(script_path, script_kind)
}

/// Generate a shell script for single-terminal mode.
#[allow(dead_code)]
pub fn generate_single_script(layout: &LayoutNode, project_path: &str) -> String {
    script_builder::generate_single_script(layout, project_path)
}

pub(crate) fn generate_single_script_for_current_os(
    layout: &LayoutNode,
    project_path: &str,
) -> (String, &'static str, LaunchScriptKind) {
    script_builder::generate_single_script_for_current_os(layout, project_path)
}

/// Deploy a launch config: validate, generate script, execute in terminal.
pub fn deploy(config: &LaunchConfig, project_path: &str) -> Result<DeployResult> {
    if let Err(errors) = super::terminal::config::validate(config) {
        return Ok(DeployResult {
            success: false,
            message: errors.join("; "),
            script_path: None,
        });
    }

    let (script, extension, script_kind) =
        generate_single_script_for_current_os(deployable_layout(config), project_path);

    let script_path = std::env::temp_dir().join(format!(
        "ss-launch-{}.{}",
        session::session_name(&config.project_name),
        extension
    ));
    std::fs::write(&script_path, &script)
        .with_context(|| format!("Failed to write launch script to {}", script_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;
    }

    open_script_in_terminal_with_kind(&script_path, script_kind)?;

    Ok(DeployResult {
        success: true,
        message: format!("Launched '{}'", config.project_name),
        script_path: Some(script_path.to_string_lossy().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::terminal::config::LaunchMode;
    use std::collections::HashMap;

    fn pane(id: &str, agent: &str) -> LayoutNode {
        LayoutNode::Pane {
            id: id.to_string(),
            agent_id: agent.to_string(),
            provider_id: None,
            provider_name: None,
            model_id: None,
            safe_mode: false,
            extra_args: vec![],
        }
    }

    #[test]
    fn test_session_name_deterministic() {
        let name = session::session_name("my project");
        assert!(name.starts_with("ss-"));
        assert!(name.contains("my-project"));
        assert_eq!(name, session::session_name("my project"));
    }

    #[test]
    fn test_single_script_generation() {
        let layout = pane("1", "claude");
        let script = generate_single_script(&layout, "/home/user/project");
        assert!(script.contains("cd"));
        assert!(script.contains("/home/user/project"));
        assert!(script.contains("claude"));
        assert!(script.contains("rm -f \"$0\""));
    }

    #[test]
    fn test_platform_single_script_variant() {
        let layout = pane("1", "claude");
        let (script, extension, kind) =
            generate_single_script_for_current_os(&layout, "/tmp/project");

        #[cfg(target_os = "windows")]
        {
            assert_eq!(extension, "ps1");
            assert_eq!(kind, LaunchScriptKind::PowerShell);
            assert!(script.contains("Set-Location -LiteralPath"));
            assert!(script.contains("& 'claude'"));
        }

        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(extension, "sh");
            assert_eq!(kind, LaunchScriptKind::Bash);
            assert!(script.starts_with("#!/bin/bash"));
        }
    }

    #[test]
    fn test_normalize_claude_auth_keys_keeps_auth_token_only() {
        let mut env = HashMap::new();
        env.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            "token-from-provider".to_string(),
        );

        provider_env::normalize_claude_auth_keys(&mut env);

        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN"),
            Some(&"token-from-provider".to_string())
        );
        assert!(!env.contains_key("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn test_normalize_claude_auth_keys_custom_base_keeps_auth_token_mode() {
        let mut env = HashMap::new();
        env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), "token".to_string());
        env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://api.minimaxi.com/anthropic".to_string(),
        );

        provider_env::normalize_claude_auth_keys(&mut env);

        assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN"), Some(&"token".to_string()));
        assert!(!env.contains_key("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn test_normalize_claude_auth_keys_keeps_api_key_only() {
        let mut env = HashMap::new();
        env.insert(
            "ANTHROPIC_API_KEY".to_string(),
            "api-key-from-provider".to_string(),
        );

        provider_env::normalize_claude_auth_keys(&mut env);

        assert_eq!(
            env.get("ANTHROPIC_API_KEY"),
            Some(&"api-key-from-provider".to_string())
        );
        assert!(!env.contains_key("ANTHROPIC_AUTH_TOKEN"));
    }

    #[test]
    fn test_normalize_claude_auth_keys_prefers_api_key_when_both_present() {
        let mut env = HashMap::new();
        env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), "token".to_string());
        env.insert("ANTHROPIC_API_KEY".to_string(), "key".to_string());

        provider_env::normalize_claude_auth_keys(&mut env);

        assert_eq!(env.get("ANTHROPIC_API_KEY"), Some(&"key".to_string()));
        assert!(!env.contains_key("ANTHROPIC_AUTH_TOKEN"));
    }

    #[test]
    fn test_normalize_claude_model_env_maps_anthropic_model_to_claude_code_model() {
        let mut env = HashMap::new();
        env.insert(
            "ANTHROPIC_MODEL".to_string(),
            "MiniMax-M2.7-highspeed".to_string(),
        );
        provider_env::normalize_claude_model_env(&mut env);
        assert_eq!(
            env.get("CLAUDE_CODE_MODEL"),
            Some(&"MiniMax-M2.7-highspeed".to_string())
        );
    }

    #[test]
    fn test_list_available_clis() {
        let clis = list_available_clis();
        assert_eq!(clis.len(), 4);
        assert_eq!(clis[0].id, "claude");
        assert_eq!(clis[1].id, "codex");
    }

    #[test]
    fn test_preferred_layout_falls_back_to_legacy_multi_layout() {
        let config = LaunchConfig {
            project_name: "legacy".to_string(),
            mode: LaunchMode::Multi,
            single_layout: pane("a", ""),
            multi_layout: pane("b", "codex"),
            updated_at: 0,
        };

        match deployable_layout(&config) {
            LayoutNode::Pane { id, agent_id, .. } => {
                assert_eq!(id, "b");
                assert_eq!(agent_id, "codex");
            }
            LayoutNode::Split { .. } => panic!("expected pane layout"),
        }
    }
}

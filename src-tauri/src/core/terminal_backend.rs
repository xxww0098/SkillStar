//! Terminal backend: CLI detection, script generation, terminal launch, and deploy orchestration.

use anyhow::{Context, Result};
use std::path::PathBuf;

use super::launch_deck::{LaunchConfig, LaunchMode, LayoutNode};

mod pane_command;
mod provider_env;
mod registry;
mod script_builder;
mod session;
mod terminal_launcher;
mod tmux_support;
mod types;

pub use types::{AgentCliInfo, DeployResult, LaunchScriptKind, TmuxStatus};

/// Find the binary path for a given agent id.
pub fn find_cli_binary(agent_id: &str) -> Option<PathBuf> {
    registry::find_cli_binary(agent_id)
}

/// List all known agent CLIs with installation status.
pub fn list_available_clis() -> Vec<AgentCliInfo> {
    registry::list_available_clis()
}

/// Check if tmux is installed and get version information.
pub fn check_tmux() -> TmuxStatus {
    tmux_support::check_tmux()
}

/// Open a launch script in the user's preferred terminal emulator.
pub fn open_script_in_terminal_with_kind(
    script_path: &std::path::Path,
    script_kind: LaunchScriptKind,
) -> Result<()> {
    terminal_launcher::open_script_in_terminal_with_kind(script_path, script_kind)
}

/// Generate a shell script for single-terminal mode (no tmux).
pub fn generate_single_script(layout: &LayoutNode, project_path: &str) -> String {
    script_builder::generate_single_script(layout, project_path)
}

pub(crate) fn generate_single_script_for_current_os(
    layout: &LayoutNode,
    project_path: &str,
) -> (String, &'static str, LaunchScriptKind) {
    script_builder::generate_single_script_for_current_os(layout, project_path)
}

/// Generate a tmux script for multi-terminal mode.
pub fn generate_multi_script(config: &LaunchConfig, project_path: &str) -> String {
    script_builder::generate_multi_script(config, project_path)
}

/// Deploy a launch config: validate, generate script, execute in terminal.
pub fn deploy(config: &LaunchConfig, project_path: &str) -> Result<DeployResult> {
    #[cfg(target_os = "windows")]
    if config.mode == LaunchMode::Multi {
        return Ok(DeployResult {
            success: false,
            message: "Multi mode (tmux) is disabled on Windows. Please use single mode."
                .to_string(),
            script_path: None,
        });
    }

    if let Err(errors) = super::launch_deck::validate(config) {
        return Ok(DeployResult {
            success: false,
            message: errors.join("; "),
            script_path: None,
        });
    }

    if config.mode == LaunchMode::Multi {
        let status = check_tmux();
        if !status.installed {
            return Ok(DeployResult {
                success: false,
                message:
                    "tmux is not available in a bash runtime. Install tmux in Git Bash/MSYS2/WSL and verify `bash --login -c \"tmux -V\"`."
                        .to_string(),
                script_path: None,
            });
        }
    }

    let (script, extension, script_kind) = match config.mode {
        LaunchMode::Single => {
            generate_single_script_for_current_os(&config.single_layout, project_path)
        }
        LaunchMode::Multi => (
            generate_multi_script(config, project_path),
            "sh",
            LaunchScriptKind::Bash,
        ),
    };

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
        message: format!(
            "Launched {} mode for '{}'",
            match config.mode {
                LaunchMode::Single => "single",
                LaunchMode::Multi => "multi",
            },
            config.project_name
        ),
        script_path: Some(script_path.to_string_lossy().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::launch_deck::SplitDirection;
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

    fn split(dir: SplitDirection, ratio: f64, a: LayoutNode, b: LayoutNode) -> LayoutNode {
        LayoutNode::Split {
            direction: dir,
            ratio,
            children: Box::new([a, b]),
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
    fn test_multi_script_2_panes() {
        let config = LaunchConfig {
            project_name: "test".to_string(),
            mode: LaunchMode::Multi,
            single_layout: pane("a", "claude"),
            multi_layout: split(
                SplitDirection::H,
                0.5,
                pane("a", "claude"),
                pane("b", "codex"),
            ),
            updated_at: 0,
        };
        let script = generate_multi_script(&config, "/tmp/project");

        assert!(script.contains("split-window -h"));
        assert!(script.contains("claude"));
        assert!(script.contains("codex"));
        assert!(script.contains("Phase 1"));
        assert!(script.contains("Phase 2"));
        assert!(script.contains("Phase 3"));
    }

    #[test]
    fn test_multi_script_5_panes_allocation() {
        let tree = split(
            SplitDirection::H,
            0.6,
            split(
                SplitDirection::V,
                0.5,
                pane("a", "claude"),
                pane("b", "codex"),
            ),
            split(
                SplitDirection::V,
                0.33,
                pane("c", "gemini"),
                split(
                    SplitDirection::H,
                    0.5,
                    pane("d", "claude"),
                    pane("e", "opencode"),
                ),
            ),
        );

        let mut split_commands = vec![];
        let mut pane_commands = vec![];
        let mut next_pane = 1usize;
        script_builder::collect_commands(
            &tree,
            0,
            &mut next_pane,
            &mut split_commands,
            &mut pane_commands,
        );

        assert_eq!(split_commands.len(), 4);
        assert_eq!(pane_commands.len(), 5);

        let command_map: HashMap<usize, &str> = pane_commands
            .iter()
            .map(|(id, command)| (*id, command.as_str()))
            .collect();
        assert!(command_map[&0].contains("claude"));
        assert!(command_map[&2].contains("codex"));
        assert!(command_map[&1].contains("gemini"));
        assert!(command_map[&3].contains("claude"));
        assert!(command_map[&4].contains("opencode"));
    }

    #[test]
    fn test_list_available_clis() {
        let clis = list_available_clis();
        assert_eq!(clis.len(), 4);
        assert_eq!(clis[0].id, "claude");
        assert_eq!(clis[1].id, "codex");
    }

    #[test]
    fn test_check_tmux() {
        let _status = check_tmux();
    }

    #[test]
    fn test_3pane_vsplit_hsplit_layout() {
        let tree = split(
            SplitDirection::V,
            0.5,
            split(
                SplitDirection::H,
                0.5,
                pane("1", "claude"),
                pane("5", "opencode"),
            ),
            pane("2", "gemini"),
        );

        let mut split_commands = vec![];
        let mut pane_commands = vec![];
        let mut next_pane = 1usize;
        script_builder::collect_commands(
            &tree,
            0,
            &mut next_pane,
            &mut split_commands,
            &mut pane_commands,
        );

        assert_eq!(split_commands.len(), 2);
        assert_eq!(pane_commands.len(), 3);

        let command_map: HashMap<usize, &str> = pane_commands
            .iter()
            .map(|(id, command)| (*id, command.as_str()))
            .collect();

        assert!(command_map[&0].contains("claude"));
        assert!(command_map[&2].contains("opencode"));
        assert!(command_map[&1].contains("gemini"));
        assert!(split_commands[0].contains("-v"));
        assert!(split_commands[0].contains("0.0"));
        assert!(split_commands[1].contains("-h"));
        assert!(split_commands[1].contains("0.0"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_multi_script_normalizes_windows_project_path() {
        let config = LaunchConfig {
            project_name: "win".to_string(),
            mode: LaunchMode::Multi,
            single_layout: pane("a", "claude"),
            multi_layout: pane("a", "claude"),
            updated_at: 0,
        };
        let script = generate_multi_script(&config, r"D:\code\SkillStar");
        assert!(script.contains("D=D:/code/SkillStar"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_multi_script_includes_dynamic_windows_path_bootstrap() {
        let config = LaunchConfig {
            project_name: "win".to_string(),
            mode: LaunchMode::Multi,
            single_layout: pane("a", "claude"),
            multi_layout: pane("a", "claude"),
            updated_at: 0,
        };
        let script = generate_multi_script(&config, r"D:\code\SkillStar");
        assert!(script.contains("# Phase 0: import Windows PATH into bash PATH"));
        assert!(script.contains("PATH=\"$PATH:"));
        assert!(script.contains("export PATH"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_deploy_rejects_multi_mode_on_windows() {
        let config = LaunchConfig {
            project_name: "win".to_string(),
            mode: LaunchMode::Multi,
            single_layout: pane("a", ""),
            multi_layout: pane("a", ""),
            updated_at: 0,
        };

        let result = deploy(&config, r"D:\code\SkillStar").expect("deploy should return a result");
        assert!(!result.success);
        assert!(result.message.contains("disabled on Windows"));
        assert!(result.script_path.is_none());
    }
}

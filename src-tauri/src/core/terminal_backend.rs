//! Terminal backend — CLI binary detection, tmux management, script generation, and deploy.
//!
//! Supports two deployment modes:
//! - **Single**: generates a simple shell script that runs one CLI directly.
//! - **Multi**: generates a two-phase tmux script (split all panes, then send commands).
//!
//! Uses a correct **allocation-based** pane-ID algorithm to avoid tmux index errors.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

use super::launch_deck::{LaunchConfig, LaunchMode, LayoutNode, SplitDirection};

// ── Agent CLI Registry ──────────────────────────────────────────────

/// Metadata about an agent CLI binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCliInfo {
    pub id: String,
    pub name: String,
    pub binary: String,
    pub installed: bool,
    pub path: Option<String>,
}

/// The supported CLI agents (desktop apps are explicitly excluded).
const AGENT_CLIS: &[(&str, &str, &str)] = &[
    ("claude", "Claude Code", "claude"),
    ("codex", "Codex CLI", "codex"),
    ("opencode", "OpenCode", "opencode"),
    ("gemini", "Gemini CLI", "gemini"),
];

/// Find the binary path for a given agent id.
pub fn find_cli_binary(agent_id: &str) -> Option<PathBuf> {
    let binary_name = AGENT_CLIS
        .iter()
        .find(|(id, _, _)| *id == agent_id)
        .map(|(_, _, bin)| *bin)?;
    which::which(binary_name).ok()
}

/// List all known agent CLIs with their installation status.
pub fn list_available_clis() -> Vec<AgentCliInfo> {
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

// ── tmux Detection ──────────────────────────────────────────────────

/// tmux availability status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmuxStatus {
    pub installed: bool,
    pub version: Option<String>,
}

/// Check if tmux is installed and get its version.
pub fn check_tmux() -> TmuxStatus {
    match which::which("tmux") {
        Ok(_) => {
            let version = std::process::Command::new("tmux")
                .arg("-V")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|v| v.trim().to_string());
            TmuxStatus {
                installed: true,
                version,
            }
        }
        Err(_) => TmuxStatus {
            installed: false,
            version: None,
        },
    }
}

// ── Session Naming ──────────────────────────────────────────────────

/// Generate a deterministic tmux session name for a project.
fn session_name(project_name: &str) -> String {
    let sanitized: String = project_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    let mut hasher = Sha256::new();
    hasher.update(project_name.as_bytes());
    let hash_bytes = hasher.finalize();
    let hash: String = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect();
    let short_hash = &hash[..6];
    format!("ss-{}-{}", sanitized, short_hash)
}

// ── Environment Variable Extraction ─────────────────────────────────

/// Extract environment variables for a pane's agent from the model provider store.
fn extract_env_for_pane(pane: &LayoutNode) -> HashMap<String, String> {
    let (agent_id, provider_id) = match pane {
        LayoutNode::Pane {
            agent_id,
            provider_id,
            ..
        } => (agent_id.as_str(), provider_id.as_deref()),
        _ => return HashMap::new(),
    };

    if agent_id.is_empty() {
        return HashMap::new();
    }

    let store = crate::core::model_config::providers::read_store().unwrap_or_default();
    let provider_id = provider_id.unwrap_or("");

    let app_providers = match agent_id {
        "claude" => &store.claude,
        "codex" => &store.codex,
        _ => return HashMap::new(),
    };

    let provider = if provider_id.is_empty() {
        // Use current provider
        app_providers
            .current
            .as_deref()
            .and_then(|id| app_providers.providers.get(id))
    } else {
        app_providers.providers.get(provider_id)
    };

    let Some(provider) = provider else {
        return HashMap::new();
    };

    let mut env = HashMap::new();

    // Extract from settingsConfig.env object
    if let Some(env_obj) = provider.settings_config.get("env").and_then(|v| v.as_object()) {
        for (key, value) in env_obj {
            if let Some(val) = value.as_str() {
                env.insert(key.clone(), val.to_string());
            }
        }
    }

    env
}

// ── Command Building ────────────────────────────────────────────────

/// Build the CLI command string for a pane.
fn build_pane_command(pane: &LayoutNode) -> String {
    let (agent_id, safe_mode, extra_args, model_id) = match pane {
        LayoutNode::Pane {
            agent_id,
            safe_mode,
            extra_args,
            model_id,
            ..
        } => (agent_id.as_str(), *safe_mode, extra_args, model_id.as_deref()),
        _ => return String::new(),
    };

    let binary = AGENT_CLIS
        .iter()
        .find(|(id, _, _)| *id == agent_id)
        .map(|(_, _, bin)| *bin)
        .unwrap_or(agent_id);

    // Build env prefix
    let env_vars = extract_env_for_pane(pane);
    let env_prefix: String = env_vars
        .iter()
        .map(|(k, v)| format!("{}={}", k, shell_escape(v)))
        .collect::<Vec<_>>()
        .join(" ");

    let mut parts = vec![];
    if !env_prefix.is_empty() {
        parts.push(env_prefix);
    }
    parts.push(binary.to_string());

    // Agent-specific safe mode flags
    if safe_mode {
        match agent_id {
            "claude" => parts.push("--dangerously-skip-permissions".to_string()),
            _ => {} // Other agents don't have a standardized safe-mode flag
        }
    }

    // OpenCode model override
    if agent_id == "opencode" {
        if let Some(model) = model_id {
            if !model.is_empty() {
                parts.push("--model".to_string());
                parts.push(model.to_string());
            }
        }
    }

    for arg in extra_args {
        parts.push(arg.clone());
    }

    parts.join(" ")
}

fn shell_escape(s: &str) -> String {
    if s.contains(' ') || s.contains('"') || s.contains('\'') || s.contains('$') {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

// ── Terminal Emulator Detection & Launch ─────────────────────────────

/// Detected terminal emulator on macOS.
#[cfg(target_os = "macos")]
#[derive(Debug)]
enum MacTerminal {
    Ghostty,
    ITerm2,
    TerminalApp,
}

/// Detect the user's preferred terminal emulator on macOS.
///
/// Checks installed terminals in preference order:
/// 1. Ghostty (needs CLI integration installed via Ghostty menu)
/// 2. iTerm2 (checks /Applications/)
/// 3. Terminal.app (always available)
#[cfg(target_os = "macos")]
fn detect_macos_terminal() -> MacTerminal {
    if which::which("ghostty").is_ok() {
        return MacTerminal::Ghostty;
    }
    if std::path::Path::new("/Applications/iTerm.app").exists() {
        return MacTerminal::ITerm2;
    }
    MacTerminal::TerminalApp
}

/// Open a shell script in the user's preferred terminal emulator.
///
/// **macOS**: Ghostty → iTerm2 → Terminal.app
/// **Linux**: Ghostty → Kitty → WezTerm → Alacritty → gnome-terminal → konsole → xfce4-terminal → foot → xterm
/// **Windows**: Windows Terminal (wt) → cmd start bash
pub fn open_script_in_terminal(script_path: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let terminal = detect_macos_terminal();
        tracing::info!("Detected macOS terminal: {:?}", terminal);
        match terminal {
            MacTerminal::Ghostty => {
                std::process::Command::new("ghostty")
                    .arg("-e")
                    .arg("bash")
                    .arg(script_path)
                    .spawn()
                    .context("Failed to launch Ghostty")?;
            }
            MacTerminal::ITerm2 => {
                std::process::Command::new("open")
                    .arg("-a")
                    .arg("iTerm")
                    .arg(script_path)
                    .spawn()
                    .context("Failed to launch iTerm2")?;
            }
            MacTerminal::TerminalApp => {
                std::process::Command::new("open")
                    .arg("-a")
                    .arg("Terminal")
                    .arg(script_path)
                    .spawn()
                    .context("Failed to launch Terminal.app")?;
            }
        }
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        // Terminal emulators in preference order, with their launch args.
        // Each entry: (binary, args_builder) where args_builder returns the args
        // needed to execute a bash script.
        let candidates: Vec<(&str, Vec<String>)> = vec![
            ("ghostty", vec!["-e".into(), "bash".into(), script_path.to_string_lossy().into()]),
            ("kitty", vec!["bash".into(), script_path.to_string_lossy().into()]),
            ("wezterm", vec!["start".into(), "--".into(), "bash".into(), script_path.to_string_lossy().into()]),
            ("alacritty", vec!["-e".into(), "bash".into(), script_path.to_string_lossy().into()]),
            ("gnome-terminal", vec!["--".into(), "bash".into(), script_path.to_string_lossy().into()]),
            ("konsole", vec!["-e".into(), "bash".into(), script_path.to_string_lossy().into()]),
            ("xfce4-terminal", vec!["-e".into(), format!("bash {}", script_path.display())]),
            ("foot", vec!["bash".into(), script_path.to_string_lossy().into()]),
            ("xterm", vec!["-e".into(), "bash".into(), script_path.to_string_lossy().into()]),
        ];

        for (bin, args) in &candidates {
            if which::which(bin).is_ok() {
                tracing::info!("Detected Linux terminal: {}", bin);
                if std::process::Command::new(bin).args(args).spawn().is_ok() {
                    return Ok(());
                }
            }
        }
        anyhow::bail!("No supported terminal emulator found. Install one of: ghostty, kitty, wezterm, alacritty, gnome-terminal, konsole, xterm");
    }

    #[cfg(target_os = "windows")]
    {
        // Try Windows Terminal first, then fall back to cmd
        if which::which("wt").is_ok() {
            tracing::info!("Detected Windows Terminal");
            std::process::Command::new("wt")
                .args(["new-tab", "bash", &script_path.to_string_lossy()])
                .spawn()
                .context("Failed to launch Windows Terminal")?;
        } else {
            tracing::info!("Falling back to cmd.exe");
            std::process::Command::new("cmd")
                .args(["/C", "start", "bash", &script_path.to_string_lossy()])
                .spawn()
                .context("Failed to launch terminal via cmd")?;
        }
        return Ok(());
    }
}

// ── Single Terminal Script ──────────────────────────────────────────

/// Generate a shell script for single-terminal mode (no tmux).
pub fn generate_single_script(layout: &LayoutNode, project_path: &str) -> String {
    let cmd = build_pane_command(layout);
    format!(
        "#!/bin/bash\ncd {}\n{}\nrm -f \"$0\"\n",
        shell_escape(project_path),
        cmd,
    )
}

// ── Multi Terminal Script (tmux) ─────────────────────────────────────

/// Two-phase tmux script using the allocation-based pane-ID algorithm.
pub fn generate_multi_script(config: &LaunchConfig, project_path: &str) -> String {
    let session = session_name(&config.project_name);
    let mut splits: Vec<String> = vec![];
    let mut cmds: Vec<(usize, String)> = vec![];
    let mut next_pane: usize = 1;

    collect_commands(
        &config.multi_layout,
        0,
        &mut next_pane,
        &mut splits,
        &mut cmds,
        project_path,
    );

    let mut script = format!(
        "#!/bin/bash\nS=\"{session}\"; D={path}\ntmux kill-session -t \"$S\" 2>/dev/null\n\n# ── Phase 1: Create all panes ──\ntmux new-session -d -s \"$S\" -c \"$D\"\n",
        session = session,
        path = shell_escape(project_path),
    );

    for split_cmd in &splits {
        script.push_str(split_cmd);
        script.push('\n');
    }

    script.push_str("\n# ── Phase 2: Send commands ──\n");
    for (pane_id, cmd) in &cmds {
        script.push_str(&format!(
            "tmux send-keys -t \"$S:0.{}\" '{}' Enter\n",
            pane_id,
            cmd.replace('\'', "'\\''"),
        ));
    }

    script.push_str(&format!(
        "\n# ── Phase 3: Attach + cleanup ──\ntmux attach -t \"$S\"\nrm -f \"$0\"\n"
    ));

    script
}

/// Allocation-based recursive algorithm for tmux pane commands.
///
/// Each split creates a new pane; the left/top child inherits the original pane ID,
/// and the right/bottom child gets the newly created pane ID.
fn collect_commands(
    node: &LayoutNode,
    allocated_pane: usize,
    next_pane: &mut usize,
    splits: &mut Vec<String>,
    cmds: &mut Vec<(usize, String)>,
    _project_path: &str,
) {
    match node {
        LayoutNode::Pane { .. } => {
            let cmd = build_pane_command(node);
            if !cmd.is_empty() {
                cmds.push((allocated_pane, cmd));
            }
        }
        LayoutNode::Split {
            direction,
            ratio,
            children,
        } => {
            let new_pane = *next_pane;
            *next_pane += 1;

            let flag = match direction {
                SplitDirection::H => "-h",
                SplitDirection::V => "-v",
            };
            // The percentage represents the size of the NEW pane (right/bottom)
            let percent = ((1.0 - ratio) * 100.0) as u32;
            splits.push(format!(
                "tmux split-window {} -t \"$S:0.{}\" -p {} -c \"$D\"",
                flag, allocated_pane, percent
            ));

            // Left/top child → original pane (keeps the same ID after split)
            collect_commands(&children[0], allocated_pane, next_pane, splits, cmds, _project_path);
            // Right/bottom child → newly created pane
            collect_commands(&children[1], new_pane, next_pane, splits, cmds, _project_path);
        }
    }
}

// ── Deploy ──────────────────────────────────────────────────────────

/// Result of a deploy operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub success: bool,
    pub message: String,
    pub script_path: Option<String>,
}

/// Deploy a launch config: validate, generate script, execute in detected terminal.
pub fn deploy(config: &LaunchConfig, project_path: &str) -> Result<DeployResult> {
    // Validate first
    if let Err(errors) = super::launch_deck::validate(config) {
        return Ok(DeployResult {
            success: false,
            message: errors.join("; "),
            script_path: None,
        });
    }

    // Check tmux for multi mode
    if config.mode == LaunchMode::Multi {
        let status = check_tmux();
        if !status.installed {
            return Ok(DeployResult {
                success: false,
                message: "tmux is not installed. Install with: brew install tmux".to_string(),
                script_path: None,
            });
        }
    }

    // Generate script
    let script = match config.mode {
        LaunchMode::Single => generate_single_script(&config.single_layout, project_path),
        LaunchMode::Multi => generate_multi_script(config, project_path),
    };

    // Write to temp file
    let script_path = std::env::temp_dir().join(format!(
        "ss-launch-{}.sh",
        &session_name(&config.project_name)
    ));
    std::fs::write(&script_path, &script)
        .with_context(|| format!("Failed to write launch script to {}", script_path.display()))?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // Open in detected terminal emulator (cross-platform)
    open_script_in_terminal(&script_path)?;

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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::launch_deck::{LaunchConfig, LaunchMode, LayoutNode, SplitDirection};

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
    fn test_session_name() {
        let name = session_name("my project");
        assert!(name.starts_with("ss-"));
        assert!(name.contains("my-project"));
        // Deterministic
        assert_eq!(name, session_name("my project"));
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
    fn test_multi_script_2_panes() {
        let config = LaunchConfig {
            project_name: "test".to_string(),
            mode: LaunchMode::Multi,
            single_layout: pane("a", "claude"),
            multi_layout: split(SplitDirection::H, 0.5, pane("a", "claude"), pane("b", "codex")),
            updated_at: 0,
        };
        let script = generate_multi_script(&config, "/tmp/project");

        // Should have 1 split command
        assert!(script.contains("split-window -h"));
        // Both agents should appear in send-keys
        assert!(script.contains("claude"));
        assert!(script.contains("codex"));
        // Phase markers
        assert!(script.contains("Phase 1"));
        assert!(script.contains("Phase 2"));
        assert!(script.contains("Phase 3"));
    }

    #[test]
    fn test_multi_script_5_panes_allocation() {
        // Tree: HSplit(0.6, [VSplit(0.5, [A, B]), VSplit(0.33, [C, HSplit(0.5, [D, E])])])
        let tree = split(
            SplitDirection::H,
            0.6,
            split(SplitDirection::V, 0.5, pane("a", "claude"), pane("b", "codex")),
            split(
                SplitDirection::V,
                0.33,
                pane("c", "gemini"),
                split(SplitDirection::H, 0.5, pane("d", "claude"), pane("e", "opencode")),
            ),
        );
        let config = LaunchConfig {
            project_name: "big".to_string(),
            mode: LaunchMode::Multi,
            single_layout: pane("a", "claude"),
            multi_layout: tree.clone(),
            updated_at: 0,
        };

        let mut splits: Vec<String> = vec![];
        let mut cmds: Vec<(usize, String)> = vec![];
        let mut next_pane: usize = 1;
        collect_commands(&tree, 0, &mut next_pane, &mut splits, &mut cmds, "/tmp");

        // 4 splits for 5 panes
        assert_eq!(splits.len(), 4);
        // 5 commands
        assert_eq!(cmds.len(), 5);

        // Verify pane assignments:
        // Pane 0 → A (claude), Pane 2 → B (codex),
        // Pane 1 → C (gemini), Pane 3 → D (claude), Pane 4 → E (opencode)
        let cmd_map: HashMap<usize, &str> = cmds.iter().map(|(id, cmd)| (*id, cmd.as_str())).collect();
        assert!(cmd_map[&0].contains("claude"));
        assert!(cmd_map[&2].contains("codex"));
        assert!(cmd_map[&1].contains("gemini"));
        assert!(cmd_map[&3].contains("claude"));
        assert!(cmd_map[&4].contains("opencode"));
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
        // Just verify it doesn't panic
        let _status = check_tmux();
    }

    #[test]
    fn test_3pane_vsplit_hsplit_layout() {
        // User's layout: VSplit(HSplit(Claude, OpenCode), Gemini)
        // Expected tmux result:
        //   ┌──────┬──────┐
        //   │  0   │  2   │  (Claude, OpenCode)
        //   ├──────┴──────┤
        //   │      1      │  (Gemini)
        //   └─────────────┘
        let tree = split(
            SplitDirection::V,
            0.5,
            split(SplitDirection::H, 0.5, pane("1", "claude"), pane("5", "opencode")),
            pane("2", "gemini"),
        );

        let mut splits: Vec<String> = vec![];
        let mut cmds: Vec<(usize, String)> = vec![];
        let mut next_pane: usize = 1;
        collect_commands(&tree, 0, &mut next_pane, &mut splits, &mut cmds, "/tmp");

        // 2 splits for 3 panes
        assert_eq!(splits.len(), 2);
        assert_eq!(cmds.len(), 3);

        let cmd_map: HashMap<usize, &str> = cmds.iter().map(|(id, cmd)| (*id, cmd.as_str())).collect();

        // Pane assignments must match the visual layout
        assert!(cmd_map[&0].contains("claude"), "pane 0 (top-left) should be claude");
        assert!(cmd_map[&2].contains("opencode"), "pane 2 (top-right) should be opencode");
        assert!(cmd_map[&1].contains("gemini"), "pane 1 (bottom) should be gemini");

        // Split order: first vertical (top/bottom), then horizontal (left/right in top)
        assert!(splits[0].contains("-v"), "first split should be vertical");
        assert!(splits[0].contains("0.0"), "first split should target pane 0");
        assert!(splits[1].contains("-h"), "second split should be horizontal");
        assert!(splits[1].contains("0.0"), "second split should target pane 0");
    }
}

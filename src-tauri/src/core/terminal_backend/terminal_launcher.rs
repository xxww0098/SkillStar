use anyhow::{Context, Result};

#[cfg(target_os = "windows")]
use super::tmux_support::resolve_windows_bash_with_tmux;
use super::types::LaunchScriptKind;

#[cfg(target_os = "macos")]
#[derive(Debug)]
enum MacTerminal {
    Ghostty,
    ITerm2,
    TerminalApp,
}

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

/// Open a launch script in the user's preferred terminal emulator.
pub(crate) fn open_script_in_terminal_with_kind(
    script_path: &std::path::Path,
    script_kind: LaunchScriptKind,
) -> Result<()> {
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
        let candidates: Vec<(&str, Vec<String>)> = vec![
            (
                "ghostty",
                vec![
                    "-e".into(),
                    "bash".into(),
                    script_path.to_string_lossy().into(),
                ],
            ),
            (
                "kitty",
                vec!["bash".into(), script_path.to_string_lossy().into()],
            ),
            (
                "wezterm",
                vec![
                    "start".into(),
                    "--".into(),
                    "bash".into(),
                    script_path.to_string_lossy().into(),
                ],
            ),
            (
                "alacritty",
                vec![
                    "-e".into(),
                    "bash".into(),
                    script_path.to_string_lossy().into(),
                ],
            ),
            (
                "gnome-terminal",
                vec![
                    "--".into(),
                    "bash".into(),
                    script_path.to_string_lossy().into(),
                ],
            ),
            (
                "konsole",
                vec![
                    "-e".into(),
                    "bash".into(),
                    script_path.to_string_lossy().into(),
                ],
            ),
            (
                "xfce4-terminal",
                vec!["-e".into(), format!("bash {}", script_path.display())],
            ),
            (
                "foot",
                vec!["bash".into(), script_path.to_string_lossy().into()],
            ),
            (
                "xterm",
                vec![
                    "-e".into(),
                    "bash".into(),
                    script_path.to_string_lossy().into(),
                ],
            ),
        ];

        for (binary, args) in &candidates {
            if which::which(binary).is_ok() {
                tracing::info!("Detected Linux terminal: {}", binary);
                if std::process::Command::new(binary)
                    .args(args)
                    .spawn()
                    .is_ok()
                {
                    return Ok(());
                }
            }
        }

        anyhow::bail!(
            "No supported terminal emulator found. Install one of: ghostty, kitty, wezterm, alacritty, gnome-terminal, konsole, xterm"
        );
    }

    #[cfg(target_os = "windows")]
    {
        let script = script_path.to_string_lossy().to_string();

        match script_kind {
            LaunchScriptKind::PowerShell => {
                let shell = if which::which("pwsh").is_ok() {
                    "pwsh"
                } else {
                    "powershell"
                };

                if which::which("wt").is_ok() {
                    tracing::info!("Detected Windows Terminal");
                    std::process::Command::new("wt")
                        .arg("new-tab")
                        .arg(shell)
                        .args(["-NoExit", "-ExecutionPolicy", "Bypass", "-File"])
                        .arg(&script)
                        .spawn()
                        .context("Failed to launch Windows Terminal")?;
                } else {
                    tracing::info!("Falling back to cmd.exe");
                    std::process::Command::new("cmd")
                        .arg("/C")
                        .arg("start")
                        .arg("")
                        .arg(shell)
                        .args(["-NoExit", "-ExecutionPolicy", "Bypass", "-File"])
                        .arg(&script)
                        .spawn()
                        .context("Failed to launch terminal via cmd")?;
                }
            }
            LaunchScriptKind::Bash => {
                let (bash_path, _) = resolve_windows_bash_with_tmux().ok_or_else(|| {
                    anyhow::anyhow!(
                        "No tmux-capable bash runtime found. Install Git Bash/MSYS2/WSL with tmux, then ensure `bash --login -c \"tmux -V\"` works."
                    )
                })?;

                if which::which("wt").is_ok() {
                    tracing::info!("Detected Windows Terminal");
                    std::process::Command::new("wt")
                        .arg("new-tab")
                        .arg(&bash_path)
                        .arg("--login")
                        .arg(&script)
                        .spawn()
                        .context("Failed to launch Windows Terminal")?;
                } else {
                    tracing::info!("Falling back to cmd.exe");
                    std::process::Command::new("cmd")
                        .arg("/C")
                        .arg("start")
                        .arg("")
                        .arg(&bash_path)
                        .arg("--login")
                        .arg(&script)
                        .spawn()
                        .context("Failed to launch terminal via cmd")?;
                }
            }
        }

        return Ok(());
    }
}

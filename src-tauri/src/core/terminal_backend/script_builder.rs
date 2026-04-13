use crate::core::terminal::config::{LaunchConfig, LayoutNode, SplitDirection};
#[cfg(target_os = "windows")]
use std::collections::HashSet;

use super::pane_command::{build_posix_pane_command, shell_escape};
#[cfg(target_os = "windows")]
use super::pane_command::pane_command_spec;
use super::session::session_name;
use super::types::LaunchScriptKind;

fn normalize_project_path_for_bash(project_path: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        return project_path.replace('\\', "/");
    }

    #[cfg(not(target_os = "windows"))]
    {
        project_path.to_string()
    }
}

#[cfg(target_os = "windows")]
fn windows_path_segment_to_bash(segment: &str) -> Option<String> {
    let trimmed = segment.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }

    // Drive path: C:\foo\bar => /c/foo/bar
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 3 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/') {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = trimmed[3..].replace('\\', "/");
        if rest.is_empty() {
            return Some(format!("/{}", drive));
        }
        return Some(format!("/{}/{}", drive, rest.trim_start_matches('/')));
    }

    // UNC path: \\server\share => //server/share
    if let Some(unc) = trimmed.strip_prefix("\\\\") {
        return Some(format!("//{}", unc.replace('\\', "/")));
    }

    if trimmed.starts_with('/') {
        return Some(trimmed.to_string());
    }

    None
}

#[cfg(target_os = "windows")]
fn collect_windows_bash_path_segments() -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let path = std::env::var("PATH").unwrap_or_default();
    for segment in path.split(';') {
        let Some(bash_path) = windows_path_segment_to_bash(segment) else {
            continue;
        };
        if seen.insert(bash_path.clone()) {
            out.push(bash_path);
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn windows_bash_path_bootstrap_script() -> String {
    let joined = collect_windows_bash_path_segments().join(":");
    if joined.is_empty() {
        return String::new();
    }

    format!(
        "# Phase 0: import Windows PATH into bash PATH (dynamic, no hardcoded user dirs)\nPATH=\"$PATH:{joined}\"\nexport PATH\n\n"
    )
}

#[cfg(not(target_os = "windows"))]
fn windows_bash_path_bootstrap_script() -> String {
    String::new()
}

/// Generate a shell script for single-terminal mode (no tmux).
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub(crate) fn generate_single_script(layout: &LayoutNode, project_path: &str) -> String {
    let command = build_posix_pane_command(layout);
    format!(
        "#!/bin/bash\ncd {}\n{}\nrm -f \"$0\"\n",
        shell_escape(project_path),
        command,
    )
}

#[cfg(target_os = "windows")]
fn powershell_quote_single(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(target_os = "windows")]
fn generate_single_powershell_script(layout: &LayoutNode, project_path: &str) -> String {
    let mut script = String::new();
    script.push_str(&format!(
        "Set-Location -LiteralPath {}\n",
        powershell_quote_single(project_path)
    ));

    let Some(spec) = pane_command_spec(layout) else {
        script.push_str(
            "Write-Host 'No command configured for this pane.' -ForegroundColor Yellow\n",
        );
        script.push_str(
            "Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue\n",
        );
        return script;
    };

    let mut env_entries: Vec<_> = spec.env_vars.iter().collect();
    env_entries.sort_by(|a, b| a.0.cmp(b.0));
    for key in &spec.unset_env_vars {
        script.push_str(&format!(
            "Remove-Item -LiteralPath Env:{} -ErrorAction SilentlyContinue\n",
            key
        ));
    }
    for (key, value) in env_entries {
        script.push_str(&format!(
            "$env:{} = {}\n",
            key,
            powershell_quote_single(value)
        ));
    }

    script.push_str("& ");
    script.push_str(&powershell_quote_single(&spec.binary));
    for arg in &spec.args {
        script.push(' ');
        script.push_str(&powershell_quote_single(arg));
    }
    script.push('\n');
    script
        .push_str("Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue\n");
    script
}

pub(crate) fn generate_single_script_for_current_os(
    layout: &LayoutNode,
    project_path: &str,
) -> (String, &'static str, LaunchScriptKind) {
    #[cfg(target_os = "windows")]
    {
        return (
            generate_single_powershell_script(layout, project_path),
            "ps1",
            LaunchScriptKind::PowerShell,
        );
    }

    #[cfg(not(target_os = "windows"))]
    {
        (
            generate_single_script(layout, project_path),
            "sh",
            LaunchScriptKind::Bash,
        )
    }
}

/// Two-phase tmux script using allocation-based pane IDs.
pub(crate) fn generate_multi_script(config: &LaunchConfig, project_path: &str) -> String {
    let session = session_name(&config.project_name);
    let bash_project_path = normalize_project_path_for_bash(project_path);
    let path_bootstrap = windows_bash_path_bootstrap_script();
    let mut split_commands = vec![];
    let mut pane_commands = vec![];
    let mut next_pane = 1usize;

    collect_commands(
        &config.multi_layout,
        0,
        &mut next_pane,
        &mut split_commands,
        &mut pane_commands,
    );

    let mut script = format!(
        "#!/bin/bash\nS=\"{session}\"; D={path}\ntmux kill-session -t \"$S\" 2>/dev/null\n\n{path_bootstrap}# Phase 1: create panes\ntmux new-session -d -s \"$S\" -c \"$D\"\n",
        session = session,
        path = shell_escape(&bash_project_path),
        path_bootstrap = path_bootstrap,
    );

    for command in &split_commands {
        script.push_str(command);
        script.push('\n');
    }

    script.push_str("\n# Phase 2: send commands\n");
    for (pane_id, command) in &pane_commands {
        script.push_str(&format!(
            "tmux send-keys -t \"$S:0.{}\" '{}' Enter\n",
            pane_id,
            command.replace('\'', "'\\''"),
        ));
    }

    script.push_str("\n# Phase 3: attach + cleanup\ntmux attach -t \"$S\"\nrm -f \"$0\"\n");
    script
}

pub(crate) fn collect_commands(
    node: &LayoutNode,
    allocated_pane: usize,
    next_pane: &mut usize,
    split_commands: &mut Vec<String>,
    pane_commands: &mut Vec<(usize, String)>,
) {
    match node {
        LayoutNode::Pane { .. } => {
            let command = build_posix_pane_command(node);
            if !command.is_empty() {
                pane_commands.push((allocated_pane, command));
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
            let percent = ((1.0 - ratio) * 100.0) as u32;
            split_commands.push(format!(
                "tmux split-window {} -t \"$S:0.{}\" -p {} -c \"$D\"",
                flag, allocated_pane, percent
            ));

            collect_commands(
                &children[0],
                allocated_pane,
                next_pane,
                split_commands,
                pane_commands,
            );
            collect_commands(
                &children[1],
                new_pane,
                next_pane,
                split_commands,
                pane_commands,
            );
        }
    }
}

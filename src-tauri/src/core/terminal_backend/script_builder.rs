use crate::core::terminal::config::LayoutNode;

#[cfg(target_os = "windows")]
use super::pane_command::pane_command_spec;
use super::pane_command::{build_posix_pane_command, shell_escape};
use super::types::LaunchScriptKind;

/// Generate a shell script for single-terminal mode.
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

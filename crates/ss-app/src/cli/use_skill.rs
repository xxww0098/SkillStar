//! skillstar use: print a skill prompt without installing it.
//!
//! The canonical store, skill lock, and Agent directories are not written.
//! --agent launches a local agent with the prompt; it still does not install.

use std::process::Command;

use ss_skills::skill_use::prepare_use;

pub fn cmd_use(source: &str, skill: Option<&str>, agent: Option<&str>, full_depth: bool) {
    let prepared = match prepare_use(source, skill, full_depth) {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("use: {error:#}");
            std::process::exit(1);
        }
    };
    let Some(agent) = agent else {
        print!("{}", prepared.prompt);
        return;
    };
    let command = match agent_command(agent) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("use: {error}");
            std::process::exit(1);
        }
    };
    let status = Command::new(command).arg(&prepared.prompt).status();
    match status {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("use: failed to launch {command}: {error}");
            std::process::exit(1);
        }
    }
}

fn agent_command(agent: &str) -> Result<&'static str, String> {
    match agent {
        "claude-code" => Ok("claude"),
        "codex" => Ok("codex"),
        "sarvam-code" => Ok("sarvam-code"),
        other => Err(format!(
            "Unsupported agent '{other}'. Supported: claude-code, codex, sarvam-code"
        )),
    }
}

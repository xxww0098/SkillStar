//! `skillstar mcp approve <plan_id>`.

use chrono::Utc;
use std::io::{self, IsTerminal};

pub fn cmd_approve(plan_id: &str) {
    let stdin = io::stdin();
    let is_terminal = stdin.is_terminal();
    let mut input = stdin.lock();
    let mut output = io::stdout().lock();
    if let Err(err) = crate::project_skills_mcp::run_approve(
        plan_id,
        &mut input,
        is_terminal,
        &mut output,
        Utc::now(),
    ) {
        eprintln!("✗ {err}");
        std::process::exit(1);
    }
}

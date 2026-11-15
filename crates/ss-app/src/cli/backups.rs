//! `skillstar backups`: inspect, restore, and prune skill backups under `data/skills/backups/`.

use std::io::{self, IsTerminal, Write};

use clap::Subcommand;
use ss_skills::backup::BackupReason;

#[derive(Subcommand)]
pub enum BackupsCommand {
    /// List stored skill backups
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Restore a backup snapshot into data/skills/installed/
    Restore {
        /// Backup snapshot id (from `skillstar backups list`)
        id: String,
    },
    /// Remove all stored skill backups
    Clear {
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
}

fn reason_label(reason: BackupReason) -> &'static str {
    match reason {
        BackupReason::Uninstall => "uninstall",
        BackupReason::UpdateOverwrite => "update-overwrite",
        BackupReason::CopyConflict => "copy-conflict",
    }
}

pub fn cmd_backups(command: BackupsCommand) {
    match command {
        BackupsCommand::List { json } => cmd_list(json),
        BackupsCommand::Restore { id } => cmd_restore(&id),
        BackupsCommand::Clear { yes } => cmd_clear(yes),
    }
}

fn cmd_list(json: bool) {
    let entries = match ss_skills::backup::list_backups() {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("✗ Failed to read backups: {err:#}");
            std::process::exit(1);
        }
    };
    if json {
        match serde_json::to_string_pretty(&entries) {
            Ok(text) => println!("{text}"),
            Err(err) => {
                eprintln!("✗ Failed to serialize backups: {err}");
                std::process::exit(1);
            }
        }
        return;
    }
    if entries.is_empty() {
        println!("No backups stored in ~/.skillstar/data/skills/backups/.");
        return;
    }

    println!(
        "{:<30} {:<24} {:<18} {:<10} CREATED",
        "ID", "SKILL", "REASON", "BYTES"
    );
    println!("{}", "-".repeat(104));
    for entry in &entries {
        println!(
            "{:<30} {:<24} {:<18} {:<10} {}",
            truncate(&entry.id, 30),
            truncate(&entry.skill_name, 24),
            reason_label(entry.reason),
            entry.size_bytes,
            entry.created_at
        );
    }
    let (count, total_bytes) = ss_skills::backup::backups_summary();
    println!("\n{count} backup(s), {total_bytes} byte(s) total.");
}

fn cmd_restore(id: &str) {
    match ss_skills::backup::restore_backup(id) {
        Ok(name) => println!("✓ Restored backup '{id}' as '{name}'."),
        Err(err) => {
            eprintln!("✗ Failed to restore backup '{id}': {err:#}");
            std::process::exit(1);
        }
    }
}

fn cmd_clear(yes: bool) {
    let (count, bytes) = ss_skills::backup::backups_summary();
    if count == 0 {
        println!("No backups to clear.");
        return;
    }
    if !yes {
        if !io::stdin().is_terminal() {
            eprintln!("✗ Refusing to clear backups without --yes in non-interactive mode.");
            std::process::exit(2);
        }
        print!("Clear all {count} backup(s) ({bytes} bytes)? [y/N]: ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            eprintln!("✗ Failed to read confirmation.");
            std::process::exit(1);
        }
        if !matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Cancelled.");
            return;
        }
    }
    match ss_skills::backup::clear_all_backups() {
        Ok((removed, freed)) => {
            println!("✓ Removed {removed} backup(s), freed {freed} byte(s).");
        }
        Err(err) => {
            eprintln!("✗ Failed to clear backups: {err:#}");
            std::process::exit(1);
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max > 1 {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    } else {
        "…".to_string()
    }
}

//! Non-install CLI commands: remove and publish. Thin handlers over `crate::core` + the domain crates.

use super::RemoveOpts;

use ss_skills::git::gh_manager;
use ss_skills::local_skill;
use ss_skills::skill_install;
use ss_skills::skill_lock;
use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};

fn is_user_hub_entry(name: &str) -> bool {
    !name.starts_with('.')
}

pub fn cmd_remove(opts: RemoveOpts<'_>) {
    let targets: Vec<String> = if opts.all {
        let lock = skill_lock::load();
        let hub_dir = ss_core::infra::paths::hub_skills_dir();
        let mut names: BTreeSet<String> = lock.skills.keys().cloned().collect();
        if let Ok(dir_entries) = std::fs::read_dir(&hub_dir) {
            for entry in dir_entries.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && is_user_hub_entry(name)
                {
                    names.insert(name.to_string());
                }
            }
        }
        names.into_iter().collect()
    } else {
        // Names resolve to installed skills first. A name that is not one
        // falls back to matching the lock's `source` field, so one argument
        // uninstalls every skill from that source repo.
        let lock = skill_lock::load();
        let hub_dir = ss_core::infra::paths::hub_skills_dir();
        let mut expanded: BTreeSet<String> = BTreeSet::new();
        for name in opts.names {
            let is_skill = local_skill::is_local_skill(name)
                || hub_dir.join(name).exists()
                || lock.skills.contains_key(name);
            if is_skill {
                expanded.insert(name.to_string());
                continue;
            }
            let from_source: Vec<String> = lock
                .skills
                .iter()
                .filter(|(_, entry)| entry.source == *name)
                .map(|(key, _)| key.clone())
                .collect();
            if from_source.is_empty() {
                expanded.insert(name.to_string());
            } else {
                expanded.extend(from_source);
            }
        }
        expanded.into_iter().collect()
    };

    if targets.is_empty() {
        println!("Nothing to remove.");
        return;
    }

    if !opts.yes {
        print!(
            "About to remove {} skill(s): {}. Continue? [y/N] ",
            targets.len(),
            targets.join(", ")
        );
        let _ = io::stdout().flush();
        if io::stdin().is_terminal() {
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                eprintln!("✗ Failed to read confirmation");
                std::process::exit(1);
            }
            let trimmed = input.trim().to_lowercase();
            if !matches!(trimmed.as_str(), "y" | "yes") {
                println!("Cancelled.");
                return;
            }
        } else {
            eprintln!("✗ Refusing to remove in non-interactive mode without --yes");
            std::process::exit(1);
        }
    }

    let mut failed: Vec<(String, String)> = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    let mut not_found: Vec<String> = Vec::new();
    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    for name in &targets {
        // Distinguish "nothing to remove" from a real uninstall so typos and
        // stale names surface as feedback instead of a misleading "Removed".
        let exists = local_skill::is_local_skill(name)
            || hub_dir.join(name).exists()
            || skill_lock::load().skills.contains_key(name);
        if !exists {
            not_found.push(name.clone());
            continue;
        }
        match skill_install::uninstall_skill(name) {
            Ok(_) => removed.push(name.clone()),
            Err(err) => failed.push((name.clone(), err)),
        }
    }

    if !removed.is_empty() {
        println!(
            "✓ Removed {} skill(s): {}",
            removed.len(),
            removed.join(", ")
        );
    }
    for name in &not_found {
        eprintln!("• '{}' is not installed; nothing to remove.", name);
    }
    for (name, err) in &failed {
        eprintln!("✗ Failed to remove '{}': {}", name, err);
    }
    if !failed.is_empty() {
        std::process::exit(1);
    }
}

pub fn cmd_publish() {
    let status = gh_manager::check_status();
    match status {
        gh_manager::GhStatus::NotInstalled => {
            eprintln!("✗ Git is required to publish. Install from: https://git-scm.com/downloads");
            std::process::exit(1);
        }
        gh_manager::GhStatus::NotAuthenticated => {
            // Publishing uses the SkillStar GitHub App identity, not a global
            // `gh` login; the device flow lives in the desktop app.
            eprintln!(
                "✗ SkillStar is not signed in to GitHub. Sign in from the SkillStar app (Settings → GitHub), then retry."
            );
            std::process::exit(1);
        }
        gh_manager::GhStatus::Ready { .. } => {}
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    let name = cwd
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "my-skill".to_string());

    println!("Publishing '{}' to GitHub...", name);

    match gh_manager::publish_skill(
        &name,
        "my-skills",
        "SkillStar skills collection",
        true,
        None,
        &name,
        &ss_skills::skill_lock::lock_path(),
    ) {
        Ok(result) => println!("✓ Published to: {}", result.url),
        Err(e) => {
            eprintln!("✗ {}", e);
            std::process::exit(1);
        }
    }
}

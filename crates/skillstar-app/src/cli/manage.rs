//! Non-install CLI commands: update, remove, and publish. Thin handlers over `crate::core` + the domain crates.

use super::RemoveOpts;

use skillstar_skills::git::gh_manager;
use skillstar_skills::git_skill::GitSkillFacade;
use skillstar_skills::local_skill;
use std::collections::BTreeSet;
use skillstar_skills::skill_lock;
use skillstar_skills::skill_install;
use skillstar_skills::skill_update;
use std::io::{self, IsTerminal, Write};

fn is_user_hub_entry(name: &str) -> bool {
    !name.starts_with('.')
}

pub fn cmd_update(name: Option<&str>) {
    let lock = skill_lock::load();

    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let names: Vec<String> = match name {
        Some(name)
            if is_user_hub_entry(name)
                && (lock.skills.contains_key(name)
                    || (hub_dir.join(name).symlink_metadata().is_ok()
                        && !local_skill::is_local_skill(name))) =>
        {
            vec![name.to_string()]
        }
        Some(_) => Vec::new(),
        None => {
            let mut names: BTreeSet<String> =
                lock.skills.keys().cloned().collect();
            if let Ok(entries) = std::fs::read_dir(&hub_dir) {
                for entry in entries.flatten() {
                    let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                        continue;
                    };
                    if is_user_hub_entry(&name) && !local_skill::is_local_skill(&name) {
                        names.insert(name);
                    }
                }
            }
            names.into_iter().collect()
        }
    };

    if names.is_empty() {
        println!("No skills to update.");
        return;
    }

    let git = GitSkillFacade::from_file_store();

    // D-081: overwrite-style update. There are no local-divergence stops and
    // no rename migration; upstream removals are reported for manual action.
    let report = git.update_skills(&names);
    for result in &report.updated {
        print_update_result(result);
    }
    for failure in &report.failed {
        eprintln!("✗ Failed to update '{}': {}", failure.name, failure.error);
    }
    for name in &report.skipped {
        println!(
            "- '{}' is no longer available upstream; remove it or convert it to a local copy.",
            name
        );
    }
    // Declined by design, so it goes to stdout and never affects the exit code:
    // `skillstar update` with no name sweeps every hub entry, which includes
    // every Skill a shared channel owns.
    for managed in &report.channel_managed {
        println!(
            "- Skipped '{}': a shared channel manages it (repository {}); update it from the shared channel view.",
            managed.name, managed.repository_id
        );
    }
    if !report.failed.is_empty() {
        std::process::exit(1);
    }
}

fn print_update_result(result: &skill_update::UpdateResult) {
    println!("✓ Updated '{}'", result.skill.name);
    for sibling in &result.siblings_cleared {
        println!("  ↳ {} refreshed (same checkout)", sibling);
    }
    for failure in &result.agent_link_failures {
        eprintln!("  ! agent relink failed: {}", failure);
    }
}

pub fn cmd_remove(opts: RemoveOpts<'_>) {
    let targets: Vec<String> = if opts.all {
        let lock = skill_lock::load();
        let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
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
        opts.names.to_vec()
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
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
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
        gh_manager::PublishLockfileMode::ValidateOnly(&skillstar_skills::skill_lock::lock_path()),
    ) {
        Ok(result) => println!("✓ Published to: {}", result.url),
        Err(e) => {
            eprintln!("✗ {}", e);
            std::process::exit(1);
        }
    }
}

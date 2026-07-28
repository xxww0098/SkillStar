//! Non-install CLI commands: update, remove, publish, doctor, and pack
//! management. Thin handlers over `crate::core` + the domain crates.

use super::RemoveOpts;

use skillstar_skills::git::gh_manager;
use skillstar_skills::local_skill;
use skillstar_skills::lockfile;
use skillstar_skills::skill_install;
use skillstar_skills::skill_pack;
use skillstar_skills::skill_update;
use std::io::{self, IsTerminal, Write};

pub fn cmd_update(name: Option<&str>) {
    let lock_path = lockfile::lockfile_path();
    let lockfile = match lockfile::Lockfile::load(&lock_path) {
        Ok(lf) => lf,
        Err(e) => {
            eprintln!("✗ Error reading lockfile: {}", e);
            std::process::exit(1);
        }
    };

    let names: Vec<String> = match name {
        Some(name) => lockfile
            .skills
            .iter()
            .filter(|entry| entry.name == name)
            .map(|entry| entry.name.clone())
            .collect(),
        None => lockfile
            .skills
            .iter()
            .map(|entry| entry.name.clone())
            .collect(),
    };

    if names.is_empty() {
        println!("No skills to update.");
        return;
    }

    // Goes through the same transaction the GUI uses. Pulling the checkout by
    // hand — as this used to — skips the lockfile hash write, the sibling
    // fan-out, the agent relink and the project cascade, so a CLI update left
    // the install in a different state than a GUI update of the same skill.
    for name in &names {
        print!("Updating '{}'... ", name);
        let _ = io::stdout().flush();
        match skill_update::update_skill(name) {
            Ok(result) => {
                println!("✓ updated");
                for sibling in &result.siblings_cleared {
                    println!("  ↳ {} refreshed (same repository)", sibling);
                }
                for failure in &result.agent_link_failures {
                    eprintln!("  ! agent relink failed: {}", failure);
                }
            }
            Err(e) => println!("✗ {:#}", e),
        }
    }
}

pub fn cmd_remove(opts: RemoveOpts<'_>) {
    let targets: Vec<String> = if opts.all {
        let lock_path = lockfile::lockfile_path();
        let lockfile = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
        let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
        let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in &lockfile.skills {
            names.insert(entry.name.clone());
        }
        if let Ok(dir_entries) = std::fs::read_dir(&hub_dir) {
            for entry in dir_entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
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
            || lockfile::Lockfile::load(&lockfile::lockfile_path())
                .map(|lf| lf.skills.iter().any(|s| s.name == *name))
                .unwrap_or(false);
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
            eprintln!("✗ GitHub CLI (gh) is required. Install from: https://cli.github.com/");
            std::process::exit(1);
        }
        gh_manager::GhStatus::NotAuthenticated => {
            eprintln!("✗ GitHub CLI is not authenticated. Run: gh auth login");
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
        &lockfile::lockfile_path(),
    ) {
        Ok(result) => println!("✓ Published to: {}", result.url),
        Err(e) => {
            eprintln!("✗ {}", e);
            std::process::exit(1);
        }
    }
}

pub fn cmd_doctor(name: Option<&str>) {
    if let Some(name) = name {
        match skill_pack::doctor_pack(name) {
            Ok(report) => {
                println!("Pack: {} v{}", report.pack_name, report.version);
                println!(
                    "Overall healthy: {}",
                    if report.overall_healthy { "YES" } else { "NO" }
                );
                println!();
                println!("{:<25} {:<8} DETAIL", "CHECK", "PASSED");
                println!("{}", "-".repeat(60));
                for check in &report.checks {
                    println!(
                        "{:<25} {:<8} {}",
                        check.name,
                        if check.passed { "YES" } else { "NO" },
                        check.message.as_deref().unwrap_or("-")
                    );
                }
            }
            Err(e) => {
                eprintln!("✗ Doctor failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        let reports = skill_pack::doctor_all();
        if reports.is_empty() {
            println!("No packs installed.");
            return;
        }
        for report in reports {
            println!(
                "{:<20} v{:<10} {}",
                report.pack_name,
                report.version,
                if report.overall_healthy {
                    "✓ healthy"
                } else {
                    "✗ issues"
                }
            );
        }
    }
}

pub fn cmd_pack_list() {
    let packs = skill_pack::list_packs();
    if packs.is_empty() {
        println!("No packs installed.");
        return;
    }
    println!("{:<25} {:<10} {:<15} SKILLS", "NAME", "VERSION", "STATUS");
    println!("{}", "-".repeat(70));
    for pack in &packs {
        let skill_names: Vec<&str> = pack.skills.iter().map(|s| s.name.as_str()).collect();
        println!(
            "{:<25} {:<10} {:<15} {}",
            pack.name,
            pack.version,
            format!("{:?}", pack.status),
            skill_names.join(", ")
        );
    }
    println!("\n{} pack(s) installed.", packs.len());
}

pub fn cmd_pack_remove(name: &str) {
    match skill_pack::remove_pack(name) {
        Ok(removed) => {
            if removed.is_empty() {
                println!("Pack '{}' had no skills to remove.", name);
            } else {
                println!(
                    "✓ Removed pack '{}' ({} skill(s): {})",
                    name,
                    removed.len(),
                    removed.join(", ")
                );
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to remove pack '{}': {}", name, e);
            std::process::exit(1);
        }
    }
}

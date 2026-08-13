//! Non-install CLI commands: update, remove, publish, doctor, and pack
//! management. Thin handlers over `crate::core` + the domain crates.

use super::RemoveOpts;

use skillstar_skills::git::gh_manager;
use skillstar_skills::git_skill::GitSkillFacade;
use skillstar_skills::local_skill;
use skillstar_skills::lockfile;
use skillstar_skills::skill_install;
use skillstar_skills::skill_pack;
use skillstar_skills::skill_update;
use std::collections::{BTreeSet, VecDeque};
use std::io::{self, IsTerminal, Write};

fn is_user_hub_entry(name: &str) -> bool {
    !name.starts_with('.')
}

pub fn cmd_update(name: Option<&str>) {
    let lock_path = lockfile::lockfile_path();
    let lockfile = match lockfile::Lockfile::load(&lock_path) {
        Ok(lf) => lf,
        Err(e) => {
            eprintln!("✗ Error reading lockfile: {}", e);
            std::process::exit(1);
        }
    };

    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let names: Vec<String> = match name {
        Some(name)
            if is_user_hub_entry(name)
                && (lockfile.skills.iter().any(|entry| entry.name == name)
                    || (hub_dir.join(name).symlink_metadata().is_ok()
                        && !local_skill::is_local_skill(name))) =>
        {
            vec![name.to_string()]
        }
        Some(_) => Vec::new(),
        None => {
            let mut names: BTreeSet<String> = lockfile
                .skills
                .iter()
                .map(|entry| entry.name.clone())
                .collect();
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

    let git = GitSkillFacade::from_keyring();

    // Goes through the same batch transaction and divergence contract as the
    // GUI. A terminal can resolve blocked Skills interactively; redirected
    // automation remains fail-closed and never guesses whether to discard.
    let report = git.update_skills(&names);
    let mut had_failure = !report.failed.is_empty();
    for result in &report.updated {
        print_update_result(result);
    }
    for failure in &report.failed {
        eprintln!("✗ Failed to update '{}': {}", failure.name, failure.error);
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

    if report.blocked.is_empty() {
        if had_failure {
            std::process::exit(1);
        }
        return;
    }
    if !io::stdin().is_terminal() {
        for blocked in &report.blocked {
            eprintln!(
                "✗ '{}' has local changes; rerun in an interactive terminal to preserve them as '{}' or explicitly discard them.",
                blocked.name, blocked.suggested_local_name
            );
            if let Some(error) = &blocked.error {
                eprintln!("  Cause: {error}");
            }
        }
        std::process::exit(1);
    }

    let mut pending: VecDeque<_> = report.blocked.into();
    let mut moved = BTreeSet::new();
    while let Some(original) = pending.pop_front() {
        if moved.contains(&original.name) {
            continue;
        }

        let current = git.update_skills(std::slice::from_ref(&original.name));
        if let Some(result) = current.updated.first() {
            record_update_result(result, &mut moved);
            continue;
        }
        if let Some(failure) = current.failed.first() {
            had_failure = true;
            eprintln!("✗ Failed to update '{}': {}", failure.name, failure.error);
            continue;
        }
        let Some(blocked) = current.blocked.first().cloned() else {
            continue;
        };
        let Some(resolution) = prompt_divergence_resolution(&blocked) else {
            println!("Cancelled update for '{}'.", blocked.name);
            had_failure = true;
            moved.extend(current.blocked.iter().map(|item| item.name.clone()));
            continue;
        };

        match git.resolve_skill_update(&blocked.name, resolution) {
            Ok(result) => {
                if let Some(local_copy) = result.local_copy {
                    println!("✓ Preserved local copy as '{}'", local_copy.name);
                }
                for removed in &result.uninstalled {
                    println!("✓ Removed '{removed}' — its source no longer ships it");
                }
                if let Some(update) = result.update {
                    record_update_result(&update, &mut moved);
                } else {
                    for remaining in result.remaining_blocked {
                        if !pending.iter().any(|item| item.name == remaining.name) {
                            pending.push_front(remaining);
                        }
                    }
                }
            }
            Err(error) => {
                had_failure = true;
                eprintln!("✗ Failed to resolve '{}': {error:#}", blocked.name);
            }
        }
    }
    if had_failure {
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

fn record_update_result(result: &skill_update::UpdateResult, moved: &mut BTreeSet<String>) {
    print_update_result(result);
    moved.insert(result.skill.name.clone());
    moved.extend(result.siblings_cleared.iter().cloned());
}

fn prompt_divergence_resolution(
    blocked: &skill_update::SkillUpdateBlocked,
) -> Option<skill_update::LocalDivergenceResolution> {
    if blocked.reason.is_source_gone() {
        return prompt_removed_source_resolution(blocked);
    }
    println!("'{}' has local changes; update is paused.", blocked.name);
    if let Some(error) = &blocked.error {
        println!("Cause: {error}");
    }
    loop {
        print!("Preserve as local copy, discard changes, or cancel? [p/d/c] ");
        let _ = io::stdout().flush();
        let mut choice = String::new();
        if io::stdin().read_line(&mut choice).ok()? == 0 {
            return None;
        }
        match choice.trim().to_ascii_lowercase().as_str() {
            "p" | "preserve" => {
                print!("Local copy name [{}]: ", blocked.suggested_local_name);
                let _ = io::stdout().flush();
                let mut name = String::new();
                io::stdin().read_line(&mut name).ok()?;
                let name = name.trim();
                return Some(skill_update::LocalDivergenceResolution::Preserve {
                    local_name: if name.is_empty() {
                        blocked.suggested_local_name.clone()
                    } else {
                        name.to_string()
                    },
                });
            }
            "d" | "discard" => {
                return Some(skill_update::LocalDivergenceResolution::Discard);
            }
            "c" | "cancel" => return None,
            _ => println!("Enter p, d, or c."),
        }
    }
}

/// A Skill its source dropped has no clean upstream state to go back to, so
/// "discard" is not on offer here — only keeping a copy of what is left, or
/// removing it.
fn prompt_removed_source_resolution(
    blocked: &skill_update::SkillUpdateBlocked,
) -> Option<skill_update::LocalDivergenceResolution> {
    let content_gone = blocked.reason == skill_update::LocalDivergenceReason::SourceMissing;
    println!(
        "'{}' is no longer shipped by its source; update is paused.",
        blocked.name
    );
    if content_gone {
        println!("Its content is already gone, so it can only be removed.");
    }
    if let Some(error) = &blocked.error {
        println!("Cause: {error}");
    }
    loop {
        if content_gone {
            print!("Remove it, or cancel? [r/c] ");
        } else {
            print!("Keep as local copy, remove it, or cancel? [k/r/c] ");
        }
        let _ = io::stdout().flush();
        let mut choice = String::new();
        if io::stdin().read_line(&mut choice).ok()? == 0 {
            return None;
        }
        match choice.trim().to_ascii_lowercase().as_str() {
            "k" | "keep" if !content_gone => {
                print!("Local copy name [{}]: ", blocked.suggested_local_name);
                let _ = io::stdout().flush();
                let mut name = String::new();
                io::stdin().read_line(&mut name).ok()?;
                let name = name.trim();
                return Some(skill_update::LocalDivergenceResolution::Preserve {
                    local_name: if name.is_empty() {
                        blocked.suggested_local_name.clone()
                    } else {
                        name.to_string()
                    },
                });
            }
            "r" | "remove" => return Some(skill_update::LocalDivergenceResolution::Uninstall),
            "c" | "cancel" => return None,
            _ if content_gone => println!("Enter r or c."),
            _ => println!("Enter k, r, or c."),
        }
    }
}

pub fn cmd_remove(opts: RemoveOpts<'_>) {
    let targets: Vec<String> = if opts.all {
        let lock_path = lockfile::lockfile_path();
        let lockfile = match lockfile::Lockfile::load(&lock_path) {
            Ok(lockfile) => lockfile,
            Err(error) => {
                eprintln!("✗ Error reading lockfile: {error}");
                std::process::exit(1);
            }
        };
        let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
        let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in &lockfile.skills {
            names.insert(entry.name.clone());
        }
        if let Ok(dir_entries) = std::fs::read_dir(&hub_dir) {
            for entry in dir_entries.flatten() {
                if let Some(name) = entry.file_name().to_str()
                    && is_user_hub_entry(name) {
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

    let lock_path = lockfile::lockfile_path();
    match gh_manager::publish_skill(
        &name,
        "my-skills",
        "SkillStar skills collection",
        true,
        None,
        &name,
        gh_manager::PublishLockfileMode::Commit(&lock_path),
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

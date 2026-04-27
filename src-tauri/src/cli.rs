use skillstar_cli::CliHandlers;
use skillstar_cli::{
    derive_name_hint, normalize_agent_ids, print_project_targets, prompt_for_agent_selection,
    resolve_auto_project_agents, resolve_installed_name, resolve_rel_dirs_for_agents,
    supported_project_agents, validate_agent_ids,
};

use crate::core::{
    ai_provider,
    git::{gh_manager, ops as git_ops},
    lockfile,
    projects::sync,
    repo_scanner, security_scan, skill_install, skill_pack,
};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

fn install_or_reuse_skill(
    url: &str,
    explicit_name: Option<&str>,
    skill_filter: &[String],
) -> Result<(Vec<String>, bool), String> {
    let name_hint = derive_name_hint(url, explicit_name);

    // If skill filter is provided, use batch install
    if !skill_filter.is_empty() {
        if let Some(name) = explicit_name {
            return Err(format!(
                "Cannot use both --name ({}) and --skill ({})",
                name,
                skill_filter.join(", ")
            ));
        }
        let installed = skill_install::install_skills_batch(url, skill_filter)?;
        Ok((installed.into_iter().map(|s| s.name).collect(), true))
    } else {
        // Single skill install (original behavior)
        if let Some(name) = resolve_installed_name(url, explicit_name, &name_hint)? {
            return Ok((vec![name], false));
        }

        match skill_install::install_skill(url.to_string(), explicit_name.map(str::to_string)) {
            Ok(skill) => Ok((vec![skill.name], true)),
            Err(err) => {
                if err.contains("already installed") {
                    if let Some(name) = resolve_installed_name(url, explicit_name, &name_hint)? {
                        return Ok((vec![name], false));
                    }
                }
                Err(err)
            }
        }
    }
}

fn preview_install(url: &str, explicit_name: Option<&str>, skill_filter: &[String]) {
    println!("Preview mode — no changes will be made.\n");

    let name_hint = derive_name_hint(url, explicit_name);
    let skills_dir = crate::core::infra::paths::hub_skills_dir();

    if !skill_filter.is_empty() {
        println!("  Mode: batch install with --skill filter");
        println!("  URL: {}", url);
        println!("  Skill filter: {}\n", skill_filter.join(", "));

        let Ok((_repo_url, _source, _, skills_found)) =
            skill_install::fetch_repo_scanned(url, false)
        else {
            eprintln!("✗ Failed to scan repository: check URL or network access");
            std::process::exit(1);
        };

        for name in skill_filter {
            let target = find_target_skill_preview(&skills_found, Some(name), name);
            let already_installed = target
                .map(|s| skills_dir.join(&s.id).exists())
                .unwrap_or(false);
            if already_installed {
                println!("  • {} (already installed in hub — would be skipped)", name);
            } else if target.is_some() {
                println!("  • {} (would be installed to hub)", name);
            } else {
                println!("  • {} (NOT FOUND in repository)", name);
            }
        }
        return;
    }

    println!("  Mode: single-skill install");
    println!("  URL: {}", url);
    println!("  Name hint: {}", name_hint);
    if let Some(n) = explicit_name {
        println!("  Explicit name: {}", n);
    }
    println!();

    let existing_in_hub = skills_dir.join(&name_hint).exists();
    let existing_in_lockfile = resolve_installed_name(url, explicit_name, &name_hint)
        .map(|n| n.is_some())
        .unwrap_or(false);

    if existing_in_hub || existing_in_lockfile {
        println!("  • {} (already installed — would be reused)", name_hint);
    } else {
        let Ok((_, _, _, skills_found)) = skill_install::fetch_repo_scanned(url, false) else {
            println!("  • {} (would be cloned and installed to hub)", name_hint);
            return;
        };

        let target = find_target_skill_preview(&skills_found, explicit_name, &name_hint);
        if let Some(skill) = target {
            println!("  • {} (skill found in repo, would be installed)", skill.id);
        } else {
            println!(
                "  • {} (no matching skill in repo — would attempt full clone)",
                name_hint
            );
        }
    }
}

/// Inline targeting logic matching skill_install::find_target_skill.
fn find_target_skill_preview<'a>(
    skills_found: &'a [repo_scanner::DiscoveredSkill],
    requested_name: Option<&str>,
    name_hint: &str,
) -> Option<&'a repo_scanner::DiscoveredSkill> {
    if skills_found.len() == 1 {
        return skills_found.first();
    }
    let search_key = requested_name.unwrap_or(name_hint);
    let search_key_lower = search_key.to_lowercase();
    skills_found
        .iter()
        .find(|s| s.id == search_key || s.id.to_lowercase() == search_key_lower)
}

fn cmd_install(
    url: &str,
    name: Option<&str>,
    skill: &[String],
    global: bool,
    project: Option<&str>,
    requested_agents: &[String],
    preview: bool,
) {
    if preview {
        preview_install(url, name, skill);
        return;
    }

    println!("Installing from {}...", url);

    let (skill_names, newly_installed) = match install_or_reuse_skill(url, name, skill) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("✗ Failed to install into hub: {}", err);
            std::process::exit(1);
        }
    };

    if newly_installed {
        println!("✓ Installed '{}' into hub.", skill_names.join(", "));
    } else {
        println!(
            "✓ Reusing existing hub install(s): {}.",
            skill_names.join(", ")
        );
    }

    if global {
        println!("Done (global install mode).");
        return;
    }

    let project_path = match project {
        Some(path) => PathBuf::from(path),
        None => match std::env::current_dir() {
            Ok(path) => path,
            Err(err) => {
                eprintln!("✗ Failed to read current directory: {}", err);
                std::process::exit(1);
            }
        },
    };

    if !project_path.is_dir() {
        eprintln!(
            "✗ Project path is not a directory: {}",
            project_path.display()
        );
        std::process::exit(1);
    }

    let auto_agent_ids = resolve_auto_project_agents(&project_path);
    let mut chosen_agents = normalize_agent_ids(requested_agents);
    if chosen_agents.is_empty() {
        chosen_agents = prompt_for_agent_selection(&auto_agent_ids);
    }
    let agent_ids = match validate_agent_ids(&chosen_agents) {
        Ok(agent_ids) => agent_ids,
        Err(err) => {
            eprintln!("✗ {}", err);
            std::process::exit(1);
        }
    };
    let rel_dirs = resolve_rel_dirs_for_agents(&agent_ids);
    let selected_skills = skill_names.clone();

    match sync::create_project_skills(&project_path, &selected_skills, &agent_ids) {
        Ok(linked_count) => {
            println!(
                "✓ Linked {} skill(s) into project {} ({} link(s)).",
                skill_names.len(),
                project_path.display(),
                linked_count
            );
            if agent_ids.is_empty() {
                println!("  Target mode: fallback path (.agents/skills)");
            } else {
                println!("  Target agents: {}", agent_ids.join(", "));
            }
            for skill_name in &skill_names {
                print_project_targets(&project_path, &rel_dirs, skill_name);
            }
        }
        Err(err) => {
            eprintln!(
                "✗ Installed to hub but failed to link into project: {}",
                err
            );
            std::process::exit(1);
        }
    }
}

fn cmd_update(name: Option<&str>) {
    let lock_path = lockfile::lockfile_path();
    let lockfile = match lockfile::Lockfile::load(&lock_path) {
        Ok(lf) => lf,
        Err(e) => {
            eprintln!("Error reading lockfile: {}", e);
            return;
        }
    };

    let skills_dir = crate::core::infra::paths::hub_skills_dir();
    let skills_to_update: Vec<_> = if let Some(name) = name {
        lockfile.skills.iter().filter(|s| s.name == name).collect()
    } else {
        lockfile.skills.iter().collect()
    };

    if skills_to_update.is_empty() {
        println!("No skills to update.");
        return;
    }

    for skill in skills_to_update {
        let path = skills_dir.join(&skill.name);
        print!("Checking '{}' for updates... ", skill.name);
        match git_ops::check_update(&path) {
            Ok(true) => {
                print!("update available, pulling... ");
                match git_ops::pull_repo(&path) {
                    Ok(_) => println!("✓ updated"),
                    Err(e) => println!("✗ {}", e),
                }
            }
            Ok(false) => println!("already up to date"),
            Err(e) => println!("✗ {}", e),
        }
    }
}

fn cmd_publish() {
    let status = gh_manager::check_status();
    match status {
        gh_manager::GhStatus::NotInstalled => {
            eprintln!("✗ GitHub CLI (gh) is required. Install from: https://cli.github.com/");
            return;
        }
        gh_manager::GhStatus::NotAuthenticated => {
            eprintln!("✗ GitHub CLI is not authenticated. Run: gh auth login");
            return;
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
    ) {
        Ok(result) => println!("✓ Published to: {}", result.url),
        Err(e) => eprintln!("✗ {}", e),
    }
}

fn cmd_scan(path: &str, static_only: bool) {
    let dir = std::path::Path::new(path);
    if !dir.is_dir() {
        eprintln!("✗ Not a valid directory: {}", path);
        std::process::exit(1);
    }

    let skill_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    eprintln!("Scanning '{}' at {}...", skill_name, path);

    let (files, _content_hash) = security_scan::collect_scannable_files(dir);
    eprintln!("  Found {} scannable file(s)", files.len());

    if files.is_empty() {
        let result = serde_json::json!({
            "skill_name": skill_name,
            "risk_level": "Safe",
            "static_findings": [],
            "ai_findings": [],
            "summary": "No scannable files found.",
            "files_scanned": 0
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        return;
    }

    let classifications = security_scan::classify_files(&files);
    for (role, idx) in &classifications {
        eprintln!("  [{}] {}", role, files[*idx].relative_path);
    }

    let static_findings = security_scan::static_pattern_scan(&files);
    eprintln!("  Static findings: {}", static_findings.len());

    if static_only {
        let max_severity = static_findings
            .iter()
            .fold(security_scan::RiskLevel::Safe, |acc, f| {
                security_scan::RiskLevel::max(acc, f.severity)
            });
        let result = serde_json::json!({
            "skill_name": skill_name,
            "risk_level": max_severity,
            "static_findings": static_findings,
            "ai_findings": [],
            "summary": format!("Static-only scan: {} finding(s)", static_findings.len()),
            "files_scanned": files.len(),
            "mode": "static-only"
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        return;
    }

    let config = ai_provider::load_config();
    if !ai_provider::ai_runtime_ready(&config) {
        eprintln!("⚠ AI provider not configured. Running static-only scan.");
        eprintln!("  Configure AI in SkillStar Settings, or use --static-only flag.");
        let max_severity = static_findings
            .iter()
            .fold(security_scan::RiskLevel::Safe, |acc, f| {
                security_scan::RiskLevel::max(acc, f.severity)
            });
        let result = serde_json::json!({
            "skill_name": skill_name,
            "risk_level": max_severity,
            "static_findings": static_findings,
            "ai_findings": [],
            "summary": format!("Static-only scan (AI not configured): {} finding(s)", static_findings.len()),
            "files_scanned": files.len(),
            "mode": "static-only"
        });
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
        return;
    }

    eprintln!("  Running AI analysis with chunk-batched sub-agents...");

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let resolved = ai_provider::resolve_scan_params(&config);
    let ai_semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(
        resolved.max_concurrent_requests.max(1) as usize,
    ));
    let scan_mode = security_scan::ScanMode::Smart;
    match rt.block_on(security_scan::scan_single_skill::<fn(&str, Option<&str>)>(
        &config,
        &skill_name,
        dir,
        scan_mode,
        ai_semaphore,
        None,
    )) {
        Ok(result) => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            eprintln!("\n✓ Scan complete: {:?}", result.risk_level);
        }
        Err(e) => {
            eprintln!("✗ Scan failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_doctor(name: Option<&str>) {
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

fn cmd_pack_list() {
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

fn cmd_pack_remove(name: &str) {
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

fn migrate_and_run() {
    crate::core::infra::migration::migrate_legacy_paths();
}

pub fn cli_handlers() -> CliHandlers {
    CliHandlers {
        migrate_and_run,
        install: cmd_install,
        update: cmd_update,
        publish: cmd_publish,
        scan: cmd_scan,
        doctor: cmd_doctor,
        pack_list: cmd_pack_list,
        pack_remove: cmd_pack_remove,
        gui: || {
            println!("Launching SkillStar GUI...");
        },
    }
}

pub fn run(args: Vec<String>) {
    skillstar_cli::run(args, cli_handlers());
}

use clap::{Parser, Subcommand};

use crate::core::{
    agent_profile, ai_provider, gh_manager, git_ops, launch_deck, lockfile, project_manifest,
    security_scan, skill_install, skill_pack, sync, terminal_backend,
};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "skillstar",
    about = "SkillStar — Skill management for AI agents"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List installed skills
    List,
    /// Install a skill from a Git URL
    Install {
        /// Install to hub only (do not link into current project)
        #[arg(long)]
        global: bool,
        /// Target project path for project-level install (defaults to current dir)
        #[arg(long, conflicts_with = "global")]
        project: Option<String>,
        /// Target agent id(s), repeatable or comma-separated, e.g. --agent codex,opencode
        #[arg(long = "agent", value_delimiter = ',')]
        agent: Vec<String>,
        /// Explicit skill name (useful when one repo contains multiple skills)
        #[arg(long)]
        name: Option<String>,
        /// Git URL of the skill repository
        url: String,
    },
    /// Update installed skills
    Update {
        /// Name of a specific skill to update (updates all if omitted)
        name: Option<String>,
    },
    /// Create a new skill template
    Create,
    /// Publish current directory as a skill to GitHub
    Publish,
    /// Scan a skill folder for security threats
    Scan {
        /// Path to the skill directory to scan
        path: String,
        /// Skip AI analysis, run static patterns only
        #[arg(long)]
        static_only: bool,
    },
    /// Run health checks on installed packs or all packs
    Doctor {
        /// Pack name to check (checks all packs if omitted)
        name: Option<String>,
    },
    /// Manage installed skill packs
    Pack {
        #[command(subcommand)]
        action: PackAction,
    },
    /// Force launch GUI mode
    Gui,
    /// Launch agent CLIs for a project
    Launch {
        #[command(subcommand)]
        action: LaunchAction,
    },
}

#[derive(Subcommand)]
pub enum PackAction {
    /// List installed skill packs
    List,
    /// Remove an installed skill pack by name
    Remove {
        /// Pack name to remove
        name: String,
    },
}

#[derive(Subcommand)]
pub enum LaunchAction {
    /// Deploy a project's saved launch configuration
    Deploy {
        /// Project name (as registered in SkillStar)
        project_name: String,
    },
    /// Directly launch a single agent CLI in the current directory
    Run {
        /// Agent CLI to launch: claude | codex | opencode | gemini
        agent: String,
        /// Provider profile to use (optional)
        #[arg(long)]
        provider: Option<String>,
        /// Enable safe/dangerously-skip-permissions mode
        #[arg(long)]
        safe: bool,
        /// Extra arguments passed through to the CLI
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

pub fn run(args: Vec<String>) {
    // Migrate v1 flat layout → v2 categorised layout (idempotent)
    crate::core::paths::migrate_legacy_paths();

    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::List => cmd_list(),
        Commands::Install {
            url,
            global,
            project,
            agent,
            name,
        } => cmd_install(&url, name.as_deref(), global, project.as_deref(), &agent),
        Commands::Update { name } => cmd_update(name.as_deref()),
        Commands::Create => cmd_create(),
        Commands::Publish => cmd_publish(),
        Commands::Scan { path, static_only } => cmd_scan(&path, static_only),
        Commands::Doctor { name } => cmd_doctor(name.as_deref()),
        Commands::Pack { action } => match action {
            PackAction::List => cmd_pack_list(),
            PackAction::Remove { name } => cmd_pack_remove(&name),
        },
        Commands::Gui => {
            // Will be handled by main.rs — restart in GUI mode
            println!("Launching SkillStar GUI...");
        }
        Commands::Launch { action } => match action {
            LaunchAction::Deploy { project_name } => cmd_launch_deploy(&project_name),
            LaunchAction::Run {
                agent,
                provider,
                safe,
                args,
            } => cmd_launch_run(&agent, provider.as_deref(), safe, &args),
        },
    }
}

fn cmd_list() {
    let lock_path = lockfile::lockfile_path();
    match lockfile::Lockfile::load(&lock_path) {
        Ok(lockfile) => {
            if lockfile.skills.is_empty() {
                println!("No skills installed. Use 'skillstar install <url>' to add one.");
                return;
            }
            println!("{:<25} {:<50} {}", "NAME", "GIT URL", "TREE HASH");
            println!("{}", "-".repeat(90));
            for skill in &lockfile.skills {
                println!(
                    "{:<25} {:<50} {}",
                    skill.name,
                    skill.git_url,
                    &skill.tree_hash[..8.min(skill.tree_hash.len())]
                );
            }
            println!("\n{} skill(s) installed.", lockfile.skills.len());
        }
        Err(e) => eprintln!("Error reading lockfile: {}", e),
    }
}

fn derive_name_hint(url: &str, explicit_name: Option<&str>) -> String {
    explicit_name.map(str::to_string).unwrap_or_else(|| {
        url.rsplit('/')
            .next()
            .unwrap_or("skill")
            .trim_end_matches(".git")
            .to_string()
    })
}

fn normalize_remote_url(url: &str) -> String {
    url.trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_lowercase()
}

fn same_remote_url(left: &str, right: &str) -> bool {
    normalize_remote_url(left) == normalize_remote_url(right)
}

fn resolve_installed_name(
    url: &str,
    explicit_name: Option<&str>,
    name_hint: &str,
) -> Result<Option<String>, String> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let lock_path = lockfile::lockfile_path();
    let lockfile = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
    let has_matching_lock = |name: &str| {
        lockfile
            .skills
            .iter()
            .any(|entry| entry.name == name && same_remote_url(&entry.git_url, url))
    };

    if let Some(name) = explicit_name {
        if skills_dir.join(name).exists() {
            if has_matching_lock(name) {
                return Ok(Some(name.to_string()));
            }
            return Err(format!(
                "Skill '{}' already exists but is linked to a different repository. Re-run with a different --name or uninstall the existing skill first.",
                name
            ));
        }
    } else if skills_dir.join(name_hint).exists() && has_matching_lock(name_hint) {
        return Ok(Some(name_hint.to_string()));
    }

    let mut matches: Vec<String> = lockfile
        .skills
        .iter()
        .filter(|entry| {
            same_remote_url(&entry.git_url, url) && skills_dir.join(&entry.name).exists()
        })
        .map(|entry| entry.name.clone())
        .collect();
    matches.sort();
    matches.dedup();

    if let Some(name) = explicit_name {
        if matches.iter().any(|candidate| candidate == name) {
            return Ok(Some(name.to_string()));
        }
        return Ok(None);
    }

    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(format!(
            "Repository '{}' maps to multiple installed skills ({}). Please re-run with --name <skill-name>.",
            url,
            matches.join(", ")
        )),
    }
}

fn install_or_reuse_skill(
    url: &str,
    explicit_name: Option<&str>,
) -> Result<(String, bool), String> {
    let name_hint = derive_name_hint(url, explicit_name);
    if let Some(name) = resolve_installed_name(url, explicit_name, &name_hint)? {
        return Ok((name, false));
    }

    match skill_install::install_skill(url.to_string(), explicit_name.map(str::to_string)) {
        Ok(skill) => Ok((skill.name, true)),
        Err(err) => {
            if err.contains("already installed") {
                if let Some(name) = resolve_installed_name(url, explicit_name, &name_hint)? {
                    return Ok((name, false));
                }
            }
            Err(err)
        }
    }
}

fn resolve_auto_project_agents(project_path: &Path) -> Vec<String> {
    let detection = project_manifest::detect_project_agents(&project_path.to_string_lossy());
    let mut agent_ids: Vec<String> = detection
        .detected
        .iter()
        .filter(|agent| agent.exists)
        .map(|agent| agent.agent_id.clone())
        .collect();
    agent_ids.sort();
    agent_ids.dedup();
    agent_ids
}

fn normalize_agent_ids(agent_ids: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = agent_ids
        .iter()
        .map(|id| id.trim().to_lowercase())
        .filter(|id| !id.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn supported_project_agents() -> Vec<(String, String)> {
    let mut supported: Vec<(String, String)> = agent_profile::list_profiles()
        .into_iter()
        .filter(|profile| profile.has_project_skills())
        .map(|profile| (profile.id, profile.project_skills_rel))
        .collect();
    supported.sort_by(|a, b| a.0.cmp(&b.0));
    supported.dedup_by(|a, b| a.0 == b.0);
    supported
}

fn prompt_for_agent_selection(auto_agent_ids: &[String]) -> Vec<String> {
    if !io::stdin().is_terminal() {
        return auto_agent_ids.to_vec();
    }

    let supported = supported_project_agents();
    if supported.is_empty() {
        return auto_agent_ids.to_vec();
    }

    let supported_ids: Vec<String> = supported.into_iter().map(|(id, _)| id).collect();
    let default_text = if auto_agent_ids.is_empty() {
        "auto fallback (.agents/skills)".to_string()
    } else {
        format!("auto detected ({})", auto_agent_ids.join(", "))
    };

    println!("Select target agent(s) for project link:");
    println!("  Available: {}", supported_ids.join(", "));
    println!(
        "  Press Enter for {} or input comma-separated agent ids.",
        default_text
    );
    print!("  Agent(s): ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return auto_agent_ids.to_vec();
    }

    let input = input.trim();
    if input.is_empty() {
        return auto_agent_ids.to_vec();
    }

    let parsed: Vec<String> = input
        .split(',')
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    normalize_agent_ids(&parsed)
}

fn validate_agent_ids(agent_ids: &[String]) -> Result<Vec<String>, String> {
    let normalized = normalize_agent_ids(agent_ids);
    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let supported = supported_project_agents();
    let supported_ids: Vec<String> = supported.iter().map(|(id, _)| id.clone()).collect();
    let mut invalid = Vec::new();
    for agent_id in &normalized {
        if !supported_ids.iter().any(|id| id == agent_id) {
            invalid.push(agent_id.clone());
        }
    }

    if invalid.is_empty() {
        Ok(normalized)
    } else {
        Err(format!(
            "Unknown agent id(s): {}. Supported agents: {}.",
            invalid.join(", "),
            supported_ids.join(", ")
        ))
    }
}

fn resolve_rel_dirs_for_agents(agent_ids: &[String]) -> Vec<String> {
    if agent_ids.is_empty() {
        return vec![".agents/skills".to_string()];
    }

    let supported = supported_project_agents();
    let mut rel_dirs = Vec::new();
    for agent_id in agent_ids {
        if let Some((_, rel_dir)) = supported.iter().find(|(id, _)| id == agent_id) {
            rel_dirs.push(rel_dir.clone());
        }
    }

    rel_dirs.sort();
    rel_dirs.dedup();
    if rel_dirs.is_empty() {
        rel_dirs.push(".agents/skills".to_string());
    }
    rel_dirs
}

fn print_project_targets(project_path: &Path, rel_dirs: &[String], skill_name: &str) {
    for rel_dir in rel_dirs {
        let linked_path = project_path.join(rel_dir).join(skill_name);
        println!("  ↳ {}", linked_path.display());
    }
}

fn cmd_install(
    url: &str,
    name: Option<&str>,
    global: bool,
    project: Option<&str>,
    requested_agents: &[String],
) {
    println!("Installing from {}...", url);

    let (skill_name, newly_installed) = match install_or_reuse_skill(url, name) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("✗ Failed to install into hub: {}", err);
            std::process::exit(1);
        }
    };

    if newly_installed {
        println!("✓ Installed '{}' into hub.", skill_name);
    } else {
        println!("✓ Reusing existing hub install '{}'.", skill_name);
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
    let selected_skills = vec![skill_name.clone()];

    match sync::create_project_skills(&project_path, &selected_skills, &agent_ids) {
        Ok(linked_count) => {
            println!(
                "✓ Linked '{}' into project {} ({} link(s)).",
                skill_name,
                project_path.display(),
                linked_count
            );
            if agent_ids.is_empty() {
                println!("  Target mode: fallback path (.agents/skills)");
            } else {
                println!("  Target agents: {}", agent_ids.join(", "));
            }
            print_project_targets(&project_path, &rel_dirs, &skill_name);
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

    let skills_dir = crate::core::paths::hub_skills_dir();
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

fn cmd_create() {
    let skill_name = "my-new-skill";
    let dir = std::env::current_dir().unwrap_or_default().join(skill_name);

    println!("Creating skill template at {:?}...", dir);

    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("Failed to create directory: {}", e);
        return;
    }

    let skill_md = r#"---
name: my-new-skill
description: A new SkillStar skill
---

# My New Skill

Add your skill instructions here.
"#;

    if let Err(e) = std::fs::write(dir.join("SKILL.md"), skill_md) {
        eprintln!("Failed to write SKILL.md: {}", e);
        return;
    }

    println!("✓ Skill template created at {:?}", dir);
    println!("  Edit SKILL.md, then run 'skillstar publish' to share it.");
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

    // Collect files
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

    // Classify
    let classifications = security_scan::classify_files(&files);
    for (role, idx) in &classifications {
        eprintln!("  [{}] {}", role, files[*idx].relative_path);
    }

    // Static scan
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

    // Full AI scan
    let config = ai_provider::load_config();
    if !config.enabled || config.api_key.trim().is_empty() {
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
                println!("{:<25} {:<8} {}", "CHECK", "PASSED", "DETAIL");
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
    println!(
        "{:<25} {:<10} {:<15} {}",
        "NAME", "VERSION", "STATUS", "SKILLS"
    );
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

fn cmd_launch_deploy(project_name: &str) {
    let config = match launch_deck::load_config(project_name) {
        Some(c) => c,
        None => {
            eprintln!("✗ No launch config found for project '{}'", project_name);
            eprintln!("  Configure a launch layout in SkillStar UI first.");
            std::process::exit(1);
        }
    };

    // Find project path from registered projects
    let projects_path = crate::core::paths::projects_manifest_path();
    let project_path = if projects_path.exists() {
        let data = std::fs::read_to_string(&projects_path).unwrap_or_default();
        let projects: Vec<serde_json::Value> = serde_json::from_str(&data).unwrap_or_default();
        projects
            .iter()
            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(project_name))
            .and_then(|p| p.get("path").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
    } else {
        None
    };

    let project_path = match project_path {
        Some(p) => p,
        None => {
            // Fallback to current directory
            match std::env::current_dir() {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(e) => {
                    eprintln!(
                        "✗ Project '{}' not found and cannot read current dir: {}",
                        project_name, e
                    );
                    std::process::exit(1);
                }
            }
        }
    };

    println!(
        "Deploying launch config for '{}' ({:?} mode)...",
        project_name, config.mode
    );

    match terminal_backend::deploy(&config, &project_path) {
        Ok(result) => {
            if result.success {
                println!("✓ {}", result.message);
            } else {
                eprintln!("✗ {}", result.message);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("✗ Deploy failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_launch_run(agent: &str, provider: Option<&str>, safe: bool, args: &[String]) {
    // Verify the agent is a known CLI
    if terminal_backend::find_cli_binary(agent).is_none() {
        eprintln!("✗ Agent CLI '{}' not found.", agent);
        eprintln!("  Available: claude, codex, opencode, gemini");
        std::process::exit(1);
    }

    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let pane = launch_deck::LayoutNode::Pane {
        id: "cli-run".to_string(),
        agent_id: agent.to_string(),
        provider_id: provider.map(|s| s.to_string()),
        provider_name: None,
        model_id: None,
        safe_mode: safe,
        extra_args: args.to_vec(),
    };

    let (script, extension, script_kind) =
        terminal_backend::generate_single_script_for_current_os(&pane, &cwd);

    // Write and execute directly
    let script_path = std::env::temp_dir().join(format!("ss-run-{}.{}", agent, extension));
    if let Err(e) = std::fs::write(&script_path, &script) {
        eprintln!("✗ Failed to write script: {}", e);
        std::process::exit(1);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
    }

    println!("Launching {} in {}...", agent, cwd);

    match terminal_backend::open_script_in_terminal_with_kind(&script_path, script_kind) {
        Ok(_) => println!("✓ Launched in terminal"),
        Err(e) => {
            eprintln!("✗ Failed to open terminal: {}", e);
            std::process::exit(1);
        }
    }
}

use clap::{Parser, Subcommand};

use crate::core::{ai_provider, gh_manager, git_ops, lockfile, security_scan};

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
    /// Force launch GUI mode
    Gui,
}

pub fn run(args: Vec<String>) {
    let cli = Cli::parse_from(args);

    match cli.command {
        Commands::List => cmd_list(),
        Commands::Install { url } => cmd_install(&url),
        Commands::Update { name } => cmd_update(name.as_deref()),
        Commands::Create => cmd_create(),
        Commands::Publish => cmd_publish(),
        Commands::Scan { path, static_only } => cmd_scan(&path, static_only),
        Commands::Gui => {
            // Will be handled by main.rs — restart in GUI mode
            println!("Launching SkillStar GUI...");
        }
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

fn cmd_install(url: &str) {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let name = url
        .rsplit('/')
        .next()
        .unwrap_or("skill")
        .trim_end_matches(".git");
    let dest = skills_dir.join(name);

    println!("Installing skill '{}' from {}...", name, url);

    if dest.exists() {
        eprintln!("Skill '{}' already installed at {:?}", name, dest);
        return;
    }

    match git_ops::clone_repo(url, &dest) {
        Ok(_) => {
            let tree_hash = git_ops::compute_tree_hash(&dest).unwrap_or_default();
            let lock_path = lockfile::lockfile_path();
            let mut lockfile = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
            lockfile.upsert(lockfile::LockEntry {
                name: name.to_string(),
                git_url: url.to_string(),
                tree_hash,
                installed_at: chrono::Utc::now().to_rfc3339(),
                source_folder: None,
            });
            if let Err(e) = lockfile.save(&lock_path) {
                eprintln!("Warning: Failed to save lockfile: {}", e);
            }
            println!("✓ Skill '{}' installed successfully.", name);
        }
        Err(e) => eprintln!("✗ Failed to install: {}", e),
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

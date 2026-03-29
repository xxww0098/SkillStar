use clap::{Parser, Subcommand};

use crate::core::{gh_manager, git_ops, lockfile, sync};

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
    let skills_dir = sync::get_hub_skills_dir();
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

    let skills_dir = sync::get_hub_skills_dir();
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

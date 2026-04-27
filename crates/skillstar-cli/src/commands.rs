use anyhow::Context;
use skillstar_core_types::lockfile::Lockfile;
use skillstar_infra::paths::{lockfile_path, projects_manifest_path};
use skillstar_terminal::config::{LaunchConfig, deployable_layout, validate};
use skillstar_terminal::script_builder::generate_single_script_for_current_os;
use skillstar_terminal::terminal_launcher::open_script_in_terminal_with_kind;
use skillstar_terminal::types::DeployResult;
use skillstar_terminal::{LayoutNode, find_cli_binary, load_config, session_name};

pub fn cmd_create() {
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

pub fn cmd_list() {
    let lock_path = lockfile_path();
    match Lockfile::load(&lock_path) {
        Ok(lockfile) => {
            if lockfile.skills.is_empty() {
                println!("No skills installed. Use 'skillstar install <url>' to add one.");
                return;
            }
            println!("{:<25} {:<50} TREE HASH", "NAME", "GIT URL");
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

pub fn cmd_launch_deploy(project_name: &str) {
    let config = match load_config(project_name) {
        Some(c) => c,
        None => {
            eprintln!("✗ No launch config found for project '{}'", project_name);
            eprintln!("  Configure a launch layout in SkillStar UI first.");
            std::process::exit(1);
        }
    };

    let project_path = resolve_project_path(project_name);

    println!(
        "Deploying launch config for '{}' ({:?} mode)...",
        project_name, config.mode
    );

    match deploy_launch_config(&config, &project_path) {
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

fn deploy_launch_config(config: &LaunchConfig, project_path: &str) -> anyhow::Result<DeployResult> {
    if let Err(errors) = validate(config) {
        return Ok(DeployResult {
            success: false,
            message: errors.join("; "),
            script_path: None,
        });
    }

    let (script, extension, script_kind) =
        generate_single_script_for_current_os(deployable_layout(config), project_path);

    let script_path = std::env::temp_dir().join(format!(
        "ss-launch-{}.{}",
        session_name(&config.project_name),
        extension
    ));
    std::fs::write(&script_path, &script)
        .with_context(|| format!("Failed to write launch script to {}", script_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))?;
    }

    open_script_in_terminal_with_kind(&script_path, script_kind)?;

    Ok(DeployResult {
        success: true,
        message: format!("Launched '{}'", config.project_name),
        script_path: Some(script_path.to_string_lossy().to_string()),
    })
}

fn resolve_project_path(project_name: &str) -> String {
    let projects_path = projects_manifest_path();
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

    match project_path {
        Some(p) => p,
        None => match std::env::current_dir() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(e) => {
                eprintln!(
                    "✗ Project '{}' not found and cannot read current dir: {}",
                    project_name, e
                );
                std::process::exit(1);
            }
        },
    }
}

pub fn cmd_launch_run(agent: &str, provider: Option<&str>, safe: bool, args: &[String]) {
    if find_cli_binary(agent).is_none() {
        eprintln!("✗ Agent CLI '{}' not found.", agent);
        eprintln!("  Available: claude, codex, opencode, gemini");
        std::process::exit(1);
    }

    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    let pane = LayoutNode::Pane {
        id: "cli-run".to_string(),
        agent_id: agent.to_string(),
        provider_id: provider.map(|s| s.to_string()),
        provider_name: None,
        model_id: None,
        safe_mode: safe,
        extra_args: args.to_vec(),
    };

    let (script, extension, script_kind) = generate_single_script_for_current_os(&pane, &cwd);

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

    match open_script_in_terminal_with_kind(&script_path, script_kind) {
        Ok(_) => println!("✓ Launched in terminal"),
        Err(e) => {
            eprintln!("✗ Failed to open terminal: {}", e);
            std::process::exit(1);
        }
    }
}

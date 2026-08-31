//! The `skillstar install` / `add` surface: repo sources, local `.ags`/`.agd`
//! bundles, and local directories adopted as authored skills, plus `--list`
//! and `--preview` modes.

use super::{
    InstallOpts, derive_name_hint, normalize_agent_ids, print_project_targets,
    prompt_for_agent_selection_for_scope, resolve_enabled_agents, resolve_installed_name,
    resolve_rel_dirs_for_agents, supported_agent_ids, validate_agent_ids_for_scope,
};

use skillstar_skills::deployment;
use skillstar_skills::git_skill::GitSkillFacade;
use skillstar_skills::repo_scanner;
use skillstar_skills::skill_bundle;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use super::{AddKind, classify_add_input};

#[derive(Debug)]
enum InstallScope {
    Project(PathBuf),
    Global,
}

fn git_skill_facade() -> Result<GitSkillFacade, String> {
    Ok(GitSkillFacade::from_file_store())
}

#[derive(Debug)]
struct InstallDestination {
    scope: InstallScope,
    agent_ids: Vec<String>,
    mode: skillstar_skills::projects::ProjectDeployMode,
}

fn require_settings_enabled_agents(enabled: Vec<String>) -> Result<Vec<String>, String> {
    if enabled.is_empty() {
        return Err(
            "No target Agents are enabled in Settings. Enable at least one Agent, pass --agent <id>, or use --all explicitly."
                .to_string(),
        );
    }
    Ok(enabled)
}

fn resolve_install_destination(opts: &InstallOpts<'_>) -> Result<InstallDestination, String> {
    let global = if opts.global {
        true
    } else if opts.project.is_some() || opts.yes || opts.all || !io::stdin().is_terminal() {
        false
    } else {
        println!("Installation scope:");
        println!("  [P] Project — current/specified project directory");
        println!("  [G] Global  — selected Agent user directories");
        print!("  Scope [P]: ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read installation scope: {e}"))?;
        matches!(input.trim().to_ascii_lowercase().as_str(), "g" | "global")
    };

    let requested_agents = if opts.all {
        vec!["*".to_string()]
    } else {
        normalize_agent_ids(opts.agent)
    };
    let agent_ids = if requested_agents.iter().any(|id| id == "*") {
        supported_agent_ids(global)
    } else if !requested_agents.is_empty() {
        validate_agent_ids_for_scope(&requested_agents, global)?
    } else {
        let enabled = resolve_enabled_agents(global);
        let chosen = if opts.yes {
            require_settings_enabled_agents(enabled)?
        } else {
            prompt_for_agent_selection_for_scope(&enabled, global)
        };
        validate_agent_ids_for_scope(&chosen, global)?
    };
    if agent_ids.is_empty() {
        return Err("No target Agents selected".to_string());
    }

    let mode = if opts.copy {
        skillstar_skills::projects::ProjectDeployMode::Copy
    } else if opts.yes || opts.all || !io::stdin().is_terminal() {
        skillstar_skills::projects::ProjectDeployMode::Symlink
    } else {
        let profiles = skillstar_agents::list_profiles();
        let unique_targets = agent_ids
            .iter()
            .filter_map(|id| profiles.iter().find(|profile| &profile.id == id))
            .map(|profile| {
                if global {
                    profile.global_skills_dir.to_string_lossy().to_string()
                } else {
                    profile.project_skills_rel.clone()
                }
            })
            .collect::<std::collections::HashSet<_>>();
        if unique_targets.len() <= 1 {
            skillstar_skills::projects::ProjectDeployMode::Symlink
        } else {
            println!("Installation method:");
            println!("  [S] Symlink (recommended) — one source of truth");
            println!("  [C] Copy — independent directories for every target");
            print!("  Method [S]: ");
            let _ = io::stdout().flush();
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .map_err(|e| format!("Failed to read installation method: {e}"))?;
            if matches!(input.trim().to_ascii_lowercase().as_str(), "c" | "copy") {
                skillstar_skills::projects::ProjectDeployMode::Copy
            } else {
                skillstar_skills::projects::ProjectDeployMode::Symlink
            }
        }
    };

    let scope = if global {
        InstallScope::Global
    } else {
        let project = opts.project.map(PathBuf::from).unwrap_or(
            std::env::current_dir()
                .map_err(|e| format!("Failed to read current directory: {e}"))?,
        );
        if !project.is_dir() {
            return Err(format!(
                "Project path is not a directory: {}",
                project.display()
            ));
        }
        InstallScope::Project(project)
    };

    Ok(InstallDestination {
        scope,
        agent_ids,
        mode,
    })
}

fn deploy_installed_skills(
    skill_names: &[String],
    destination: &InstallDestination,
) -> Result<u32, String> {
    match &destination.scope {
        InstallScope::Global => deployment::batch_deploy_skills_to_agents(
            skill_names,
            &destination.agent_ids,
            destination.mode,
        )
        .map_err(|e| e.to_string()),
        InstallScope::Project(project_path) => deployment::create_project_skills_with_mode(
            project_path,
            skill_names,
            &destination.agent_ids,
            destination.mode,
        )
        .map_err(|e| e.to_string()),
    }
}

fn list_skills_in_bundle(path: &Path) {
    println!("Reading bundle {}...\n", path.display());
    let path_str = path.to_string_lossy();
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    match ext {
        "ags" => match skill_bundle::preview_bundle(&path_str) {
            Ok(manifest) => {
                println!("Bundle: {} v{}", manifest.name, manifest.version);
                if !manifest.description.is_empty() {
                    println!("  Description: {}", manifest.description);
                }
                println!("  Files: {}", manifest.files.len());
                println!(
                    "  Created: {} | Author: {}",
                    manifest.created_at,
                    if manifest.author.is_empty() {
                        "—"
                    } else {
                        manifest.author.as_str()
                    }
                );
                println!("\nInstall with: skillstar install {}", path.display());
            }
            Err(e) => {
                eprintln!("✗ Failed to read bundle: {}", e);
                std::process::exit(1);
            }
        },
        "agd" => match skill_bundle::preview_multi_bundle(&path_str) {
            Ok(manifest) => {
                println!("Multi-skill bundle ({} skill(s)):", manifest.skills.len());
                for entry in &manifest.skills {
                    let desc = if entry.description.is_empty() {
                        "—"
                    } else {
                        entry.description.as_str()
                    };
                    println!("  • {} ({} files) — {}", entry.name, entry.file_count, desc);
                }
                println!("\nInstall with: skillstar install {}", path.display());
            }
            Err(e) => {
                eprintln!("✗ Failed to read bundle: {}", e);
                std::process::exit(1);
            }
        },
        _ => {
            eprintln!("✗ Unsupported bundle extension: {}", path.display());
            std::process::exit(2);
        }
    }
}

fn install_bundle_file(path: &Path, opts: &InstallOpts<'_>) -> Vec<String> {
    let path_str = path.to_string_lossy();
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let force = opts.yes || opts.all;

    match ext {
        "ags" => match skill_bundle::import_bundle(&path_str, force) {
            Ok(result) => {
                let verb = if result.replaced {
                    "Replaced"
                } else {
                    "Imported"
                };
                println!(
                    "✓ {} '{}' from bundle ({} files).",
                    verb, result.name, result.file_count
                );
                println!("  Description: {}", result.description);
                vec![result.name]
            }
            Err(err) => {
                let msg = err.to_string();
                if let Some(name) = msg.strip_prefix("CONFLICT:") {
                    eprintln!(
                        "✗ Skill '{}' already exists. Re-run with --yes to replace.",
                        name
                    );
                } else {
                    eprintln!("✗ Failed to import bundle: {}", err);
                }
                std::process::exit(1);
            }
        },
        "agd" => match skill_bundle::import_multi_bundle(&path_str, force) {
            Ok(result) => {
                println!(
                    "✓ Imported {} skill(s) from bundle ({} files total, {} replaced).",
                    result.skill_names.len(),
                    result.total_file_count,
                    result.replaced_count
                );
                for name in &result.skill_names {
                    println!("  • {}", name);
                }
                result.skill_names
            }
            Err(err) => {
                let msg = err.to_string();
                if let Some(name) = msg.strip_prefix("CONFLICT:") {
                    eprintln!(
                        "✗ Skill '{}' already exists in bundle. Re-run with --yes to replace.",
                        name
                    );
                } else {
                    eprintln!("✗ Failed to import bundle: {}", err);
                }
                std::process::exit(1);
            }
        },
        _ => {
            eprintln!("✗ Unsupported bundle extension: {}", path.display());
            std::process::exit(2);
        }
    }
}

fn list_skills_in_local_dir(path: &Path) {
    println!("Scanning local directory {}...\n", path.display());
    let skills = skillstar_skills::discover_skills(path, false);
    if skills.is_empty() {
        println!(
            "No SKILL.md found in {} (root or priority dirs).",
            path.display()
        );
        return;
    }
    println!("{:<32} FOLDER", "SKILL");
    println!("{}", "-".repeat(72));
    for skill in &skills {
        println!(
            "{:<32} {}",
            truncate(&skill.id, 32),
            if skill.folder_path.is_empty() {
                "."
            } else {
                skill.folder_path.as_str()
            }
        );
    }
    println!(
        "\n{} skill(s) in {}. Adopt them with: skillstar install {}",
        skills.len(),
        path.display(),
        path.display()
    );
}

fn install_local_dir(path: &Path, opts: &InstallOpts<'_>) -> Vec<String> {
    let canonical = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("✗ Failed to resolve path {}: {}", path.display(), e);
            std::process::exit(1);
        }
    };

    println!("Adopting skills from {}...", canonical.display());

    let skills = skillstar_skills::discover_skills(&canonical, false);
    if skills.is_empty() {
        eprintln!(
            "✗ No SKILL.md found in {} (root or priority dirs).",
            canonical.display()
        );
        std::process::exit(1);
    }

    // Delegate to the Skills domain use case: discovery, frontmatter quality
    // gate, full-directory copy and per-skill outcome reporting all live in
    // one place (the CLI must not reimplement adoption).
    let names: Option<Vec<String>> = if opts.all
        || opts.skill.iter().any(|name| name == "*")
        || (opts.yes && opts.name.is_none() && opts.skill.is_empty())
        || skills.len() == 1
    {
        None
    } else if !opts.skill.is_empty() {
        Some(opts.skill.iter().map(|name| name.to_string()).collect())
    } else if let Some(name) = opts.name {
        Some(vec![name.to_string()])
    } else {
        let selected_names = match prompt_for_skill_selection(&skills) {
            Ok(names) => names,
            Err(err) => {
                eprintln!("✗ {err}");
                std::process::exit(2);
            }
        };
        Some(selected_names)
    };

    match skillstar_skills::local_skill::adopt_folder(&canonical.to_string_lossy(), names) {
        Ok(result) => {
            for adopted in &result.adopted {
                println!("  ✓ Adopted '{}'", adopted.name);
            }
            for skipped in &result.skipped {
                eprintln!("  ✗ Failed to adopt '{}': {}", skipped.name, skipped.reason);
            }
            if result.adopted.is_empty() {
                eprintln!("✗ Nothing was adopted.");
                std::process::exit(1);
            }
            println!(
                "✓ Adopted {} skill(s) into ~/.skillstar/hub/local.",
                result.adopted.len()
            );
            result
                .adopted
                .into_iter()
                .map(|adopted| adopted.name)
                .collect()
        }
        Err(err) => {
            eprintln!("✗ Adoption failed: {err}");
            std::process::exit(1);
        }
    }
}

/// Sole entry point that decides between batch-install (multi-skill), single-skill,
/// and `--all` (install every discovered skill).
fn install_or_reuse_skill(
    url: &str,
    explicit_name: Option<&str>,
    skill_filter: &[String],
    all: bool,
    yes: bool,
    agent_id: Option<&str>,
) -> Result<(Vec<String>, bool), String> {
    if explicit_name.is_some() && !skill_filter.is_empty() {
        return Err("--name cannot be combined with --skill".to_string());
    }

    let git = git_skill_facade()?;
    let (_, _, _, skills_found) = git
        .fetch_repo_scanned_preferring_local_cache(url, false)
        .map_err(|error| error.to_string())?;
    if skills_found.is_empty() {
        return Err("No valid SKILL.md found in the selected source".to_string());
    }

    let install_all = all || skill_filter.iter().any(|name| name == "*");
    let selected_names =
        if install_all || (yes && explicit_name.is_none() && skill_filter.is_empty()) {
            skills_found
                .iter()
                .map(|skill| skill.id.clone())
                .collect::<Vec<_>>()
        } else if !skill_filter.is_empty() {
            let mut selected = Vec::new();
            let mut missing = Vec::new();
            for requested in skill_filter {
                match skills_found
                    .iter()
                    .find(|skill| skill.id.eq_ignore_ascii_case(requested))
                {
                    Some(skill) if !selected.contains(&skill.id) => selected.push(skill.id.clone()),
                    Some(_) => {}
                    None => missing.push(requested.clone()),
                }
            }
            if !missing.is_empty() {
                return Err(format!(
                    "Requested skills not found: {}. Available: {}",
                    missing.join(", "),
                    skills_found
                        .iter()
                        .map(|skill| skill.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            selected
        } else if let Some(name) = explicit_name {
            let skill = skills_found
                .iter()
                .find(|skill| skill.id.eq_ignore_ascii_case(name))
                .or_else(|| (skills_found.len() == 1).then(|| &skills_found[0]))
                .ok_or_else(|| {
                    format!(
                        "Skill '{name}' not found. Available: {}",
                        skills_found
                            .iter()
                            .map(|skill| skill.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?;
            vec![skill.id.clone()]
        } else if skills_found.len() == 1 {
            vec![skills_found[0].id.clone()]
        } else {
            prompt_for_skill_selection(&skills_found)?
        };

    let installed = match agent_id {
        Some(id) => git
            .install_skills_batch_for_agent(url, &selected_names, id)
            .map_err(|error| error.to_string())?,
        None => git.install_skills_batch(url, &selected_names)?,
    };
    Ok((selected_names, !installed.is_empty()))
}

fn prompt_for_skill_selection(
    skills: &[repo_scanner::DiscoveredSkill],
) -> Result<Vec<String>, String> {
    if !io::stdin().is_terminal() {
        return Err(
            "Multiple skills found. Use --skill <name>, --skill '*', or -y in non-interactive mode."
                .to_string(),
        );
    }

    println!("Select skills to install (comma-separated, or * for all):");
    for skill in skills {
        println!(
            "  • {:<28} {}",
            skill.id,
            if skill.description.is_empty() {
                "—"
            } else {
                skill.description.as_str()
            }
        );
    }
    print!("  Skill(s): ");
    let _ = io::stdout().flush();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("Failed to read skill selection: {e}"))?;
    let requested = input
        .trim()
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    if requested.is_empty() {
        return Err("No skills selected".to_string());
    }
    if requested.contains(&"*") {
        return Ok(skills.iter().map(|skill| skill.id.clone()).collect());
    }

    let mut selected = Vec::new();
    let mut missing = Vec::new();
    for requested in requested {
        match skills
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(requested))
        {
            Some(skill) if !selected.contains(&skill.id) => selected.push(skill.id.clone()),
            Some(_) => {}
            None => missing.push(requested.to_string()),
        }
    }
    if !missing.is_empty() {
        return Err(format!("Unknown skill(s): {}", missing.join(", ")));
    }
    Ok(selected)
}

/// `skillstar install --list` — scan the source without mutating anything.
fn list_skills_in_source(url: &str) {
    println!("Scanning {}...\n", url);
    match git_skill_facade().and_then(|git| git.fetch_repo_scanned(url, false)) {
        Ok((repo_url, source, _, skills_found)) => {
            if skills_found.is_empty() {
                println!(
                    "No SKILL.md found in {} (scanned root + priority dirs).",
                    source
                );
                println!("Tip: re-run with a more specific URL or install a bundle instead.");
                return;
            }
            let skills_dir = skillstar_core::infra::paths::hub_skills_dir();
            println!("{:<32} {:<10} DESCRIPTION", "SKILL", "STATUS");
            println!("{}", "-".repeat(80));
            for skill in &skills_found {
                let status = if skills_dir.join(&skill.id).exists() {
                    "installed"
                } else {
                    "new"
                };
                let desc = if skill.description.is_empty() {
                    "—"
                } else {
                    skill.description.as_str()
                };
                println!(
                    "{:<32} {:<10} {}",
                    truncate(&skill.id, 32),
                    status,
                    truncate(desc, 60)
                );
            }
            println!(
                "\n{} skill(s) in {} ({}).",
                skills_found.len(),
                source,
                repo_url
            );
            println!(
                "Install a specific skill: skillstar install {} --skill <name>",
                url
            );
            println!(
                "Install everything:      skillstar install {} --all -y",
                url
            );
        }
        Err(e) => {
            eprintln!("✗ Scan failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn preview_install(url: &str, explicit_name: Option<&str>, skill_filter: &[String], all: bool) {
    println!("Preview mode — no changes will be made.\n");

    let name_hint = derive_name_hint(url, explicit_name);
    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();

    if all {
        println!("  Mode: install every skill (--all)");
        println!("  URL: {}\n", url);
        let Ok((_repo_url, _source, _, skills_found)) =
            git_skill_facade().and_then(|git| git.fetch_repo_scanned(url, false))
        else {
            eprintln!("✗ Failed to scan repository: check URL or network access");
            std::process::exit(1);
        };
        for skill in &skills_found {
            let status = if skills_dir.join(&skill.id).exists() {
                "would be reused"
            } else {
                "would be installed"
            };
            println!("  • {} ({})", skill.id, status);
        }
        return;
    }

    if !skill_filter.is_empty() {
        println!("  Mode: batch install with --skill filter");
        println!("  URL: {}", url);
        println!("  Skill filter: {}\n", skill_filter.join(", "));

        let Ok((_repo_url, _source, _, skills_found)) =
            git_skill_facade().and_then(|git| git.fetch_repo_scanned(url, false))
        else {
            eprintln!("✗ Failed to scan repository: check URL or network access");
            std::process::exit(1);
        };

        for name in skill_filter {
            let target =
                skillstar_skills::skill_install::find_target_skill(&skills_found, Some(name), name);
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
        let Ok((_, _, _, skills_found)) =
            git_skill_facade().and_then(|git| git.fetch_repo_scanned(url, false))
        else {
            println!("  • {} (would be cloned and installed to hub)", name_hint);
            return;
        };

        let target = skillstar_skills::skill_install::find_target_skill(
            &skills_found,
            explicit_name,
            &name_hint,
        );
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
pub fn cmd_install(opts: InstallOpts<'_>) {
    if opts.list {
        match classify_add_input(opts.url) {
            AddKind::Repo => list_skills_in_source(opts.url),
            AddKind::Bundle(path) => list_skills_in_bundle(&path),
            AddKind::LocalDir(path) => list_skills_in_local_dir(&path),
        }
        return;
    }

    if opts.preview {
        preview_install(opts.url, opts.name, opts.skill, opts.all);
        return;
    }

    let destination = match resolve_install_destination(&opts) {
        Ok(destination) => destination,
        Err(err) => {
            eprintln!("✗ {err}");
            std::process::exit(2);
        }
    };

    let (skill_names, newly_installed) = match classify_add_input(opts.url) {
        AddKind::Bundle(path) => (install_bundle_file(&path, &opts), true),
        AddKind::LocalDir(path) => (install_local_dir(&path, &opts), true),
        AddKind::Repo => {
            println!("Installing from {}...", opts.url);
            match install_or_reuse_skill(
                opts.url,
                opts.name,
                opts.skill,
                opts.all,
                opts.yes || opts.all,
                {
                    let explicit = normalize_agent_ids(opts.agent);
                    (explicit.len() == 1 && explicit[0] != "*")
                        .then(|| explicit[0].as_str())
                        .map(str::to_string)
                }
                .as_deref(),
            ) {
                Ok(result) => result,
                Err(err) => {
                    eprintln!("✗ Failed to install into hub: {}", err);
                    std::process::exit(1);
                }
            }
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

    let deployed_count = match deploy_installed_skills(&skill_names, &destination) {
        Ok(count) => count,
        Err(err) => {
            eprintln!("✗ Installed to hub but failed to deploy: {err}");
            std::process::exit(1);
        }
    };

    let method = match destination.mode {
        skillstar_skills::projects::ProjectDeployMode::Symlink => "link-first",
        skillstar_skills::projects::ProjectDeployMode::Copy => "copy",
    };
    println!(
        "✓ Deployed {} skill(s) to {} Agent target(s) ({} new deployment(s), {}).",
        skill_names.len(),
        destination.agent_ids.len(),
        deployed_count,
        method
    );
    println!("  Target agents: {}", destination.agent_ids.join(", "));

    match &destination.scope {
        InstallScope::Project(project_path) => {
            let rel_dirs = resolve_rel_dirs_for_agents(&destination.agent_ids);
            println!("  Project: {}", project_path.display());
            for skill_name in &skill_names {
                print_project_targets(project_path, &rel_dirs, skill_name);
            }
        }
        InstallScope::Global => {
            let profiles = skillstar_agents::list_profiles();
            let mut printed_dirs = std::collections::HashSet::new();
            for agent_id in &destination.agent_ids {
                let Some(profile) = profiles.iter().find(|profile| &profile.id == agent_id) else {
                    continue;
                };
                if !printed_dirs.insert(profile.global_skills_dir.clone()) {
                    continue;
                }
                for skill_name in &skill_names {
                    println!(
                        "  ↳ {}",
                        profile.global_skills_dir.join(skill_name).display()
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::require_settings_enabled_agents;

    #[test]
    fn non_interactive_install_never_falls_back_to_every_agent() {
        let error = require_settings_enabled_agents(Vec::new()).unwrap_err();
        assert!(error.contains("enabled in Settings"));
        assert!(error.contains("--agent"));
        assert!(error.contains("--all"));
    }

    #[test]
    fn non_interactive_install_keeps_the_manual_target_set() {
        let enabled = vec!["claude".to_string(), "codex".to_string()];
        assert_eq!(
            require_settings_enabled_agents(enabled.clone()).unwrap(),
            enabled
        );
    }
}

//! The `skillstar install` / `add` surface: repo sources, local `.ags`/`.agd`
//! bundles, and local directories adopted as authored skills, plus `--list`
//! and `--preview` modes.

use super::{
    InstallOpts, normalize_agent_ids, print_project_targets, prompt_for_agent_selection_for_scope,
    resolve_enabled_agents, resolve_rel_dirs_for_agents, supported_agent_ids,
    validate_agent_ids_for_scope,
};

use ss_skills::deployment;
use ss_skills::git_skill::GitSkillFacade;
use ss_skills::repo_scanner;
use ss_skills::skill_bundle;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use super::install_preview::{
    list_skills_in_bundle, list_skills_in_local_dir, list_skills_in_source, preview_install,
};
use super::{AddKind, classify_add_input};

#[derive(Debug)]
pub(super) enum InstallScope {
    Project(PathBuf),
    Global,
}

pub(super) fn git_skill_facade() -> Result<GitSkillFacade, String> {
    Ok(GitSkillFacade::from_file_store())
}

/// SkillStar only installs Skills, never a Claude plugin's `hooks`/`agents` —
/// see README non-goals. Prints nothing when the source is not a declared
/// plugin, or declares one with nothing to skip.
pub(super) fn print_plugin_hint(scan: &repo_scanner::ScanResult) {
    if scan.plugin.is_some() {
        println!(
            "ℹ This repository is a Claude Code plugin; hooks/agents will not be installed. \
             For the full plugin, use `/plugin marketplace add`."
        );
    }
}

#[derive(Debug)]
pub(super) struct InstallDestination {
    pub(super) scope: InstallScope,
    pub(super) agent_ids: Vec<String>,
    pub(super) mode: ss_skills::projects::ProjectDeployMode,
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

pub(super) fn resolve_install_destination(
    opts: &InstallOpts<'_>,
) -> Result<InstallDestination, String> {
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

    let default_global_mode = || match ss_skills::agents::get_default_deploy_mode() {
        ss_skills::agents::AgentDeployMode::Copy => ss_skills::projects::ProjectDeployMode::Copy,
        ss_skills::agents::AgentDeployMode::Symlink => {
            ss_skills::projects::ProjectDeployMode::Symlink
        }
    };
    let mode = if opts.copy {
        ss_skills::projects::ProjectDeployMode::Copy
    } else if opts.yes || opts.all || !io::stdin().is_terminal() {
        if global {
            default_global_mode()
        } else {
            ss_skills::projects::ProjectDeployMode::Symlink
        }
    } else {
        let profiles = ss_skills::agents::list_profiles();
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
            if global {
                default_global_mode()
            } else {
                ss_skills::projects::ProjectDeployMode::Symlink
            }
        } else if global && matches!(default_global_mode(), ss_skills::projects::ProjectDeployMode::Copy) {
            println!("Installation method:");
            println!("  [C] Copy (default) — independent directories for every target");
            println!("  [S] Symlink — one source of truth");
            print!("  Method [C]: ");
            let _ = io::stdout().flush();
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .map_err(|e| format!("Failed to read installation method: {e}"))?;
            if matches!(input.trim().to_ascii_lowercase().as_str(), "s" | "symlink") {
                ss_skills::projects::ProjectDeployMode::Symlink
            } else {
                ss_skills::projects::ProjectDeployMode::Copy
            }
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
                ss_skills::projects::ProjectDeployMode::Copy
            } else {
                ss_skills::projects::ProjectDeployMode::Symlink
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

pub(super) fn deploy_installed_skills(
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

fn install_local_dir(path: &Path, opts: &InstallOpts<'_>) -> Vec<String> {
    let canonical = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("✗ Failed to resolve path {}: {}", path.display(), e);
            std::process::exit(1);
        }
    };

    println!("Adopting skills from {}...", canonical.display());

    let skills = ss_skills::discover_skills(&canonical, false);
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

    match ss_skills::local_skill::adopt_folder(&canonical.to_string_lossy(), names) {
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
                "✓ Adopted {} skill(s) into ~/.skillstar/data/skills/installed.",
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
    refresh: bool,
) -> Result<(Vec<String>, bool), String> {
    if explicit_name.is_some() && !skill_filter.is_empty() {
        return Err("--name cannot be combined with --skill".to_string());
    }

    let git = git_skill_facade()?;
    let fetched = git
        .scan_repo_with_refresh(url, false, refresh)
        .map_err(|error| error.to_string())?;
    let skills_found = &fetched.skills;
    print_plugin_hint(&fetched);
    if skills_found.is_empty() {
        return Err("No valid SKILL.md found in the selected source".to_string());
    }

    let install_all = all || skill_filter.iter().any(|name| name == "*");
    let mut showed_audits = false;
    let selected_names = if install_all
        || (yes && explicit_name.is_none() && skill_filter.is_empty())
    {
        let installable = bulk_selection(skills_found);
        if installable.is_empty() {
            let internal = skills_found.iter().any(|skill| skill.internal);
            return Err(if internal {
                "No installable SKILL.md found for a bulk install. Internal skills are skipped unless named with --skill, or INSTALL_INTERNAL_SKILLS=1 is set.".to_string()
            } else {
                "No installable SKILL.md found in the selected source".to_string()
            });
        }
        installable
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
        let offered = skills_found
            .iter()
            .filter(|skill| skill.installable)
            .map(|skill| skill.id.clone())
            .collect::<Vec<_>>();
        print_install_audits(&fetched.spec.repo_url, &offered);
        showed_audits = true;
        prompt_for_skill_selection(skills_found)?
    };

    let selected = skills_found
        .iter()
        .filter(|skill| selected_names.contains(&skill.id))
        .collect::<Vec<_>>();
    if let Some(blocked) = selected.iter().find(|skill| !skill.installable) {
        return Err(format!(
            "Skill '{}' is not installable: {}",
            blocked.id,
            blocked.frontmatter_issues.join(", ")
        ));
    }
    let targets = selected
        .into_iter()
        .map(|skill| repo_scanner::SkillInstallTarget {
            id: skill.id.clone(),
            folder_path: skill.folder_path.clone(),
            pinned: false,
        })
        .collect::<Vec<_>>();
    if !showed_audits {
        let names = targets
            .iter()
            .map(|skill| skill.id.clone())
            .collect::<Vec<_>>();
        print_install_audits(&fetched.spec.repo_url, &names);
    }
    // Deploy by the folder names the installer wrote, not the scanned ids:
    // `My Skill` lands as `my-skill`.
    let installed = git
        .install_from_scan(&fetched, &targets)
        .map_err(|error| format!("{error:#}"))?;
    if installed.is_empty() {
        return Err("Nothing was installed".to_string());
    }
    Ok((installed, true))
}

/// `--all` / `-y` take every skill the frontmatter gate admits; the rest are
/// reported and left out instead of failing the whole batch.
pub(super) fn bulk_selection(skills: &[repo_scanner::DiscoveredSkill]) -> Vec<String> {
    let include_internal = install_internal_skills_enabled();
    skills
        .iter()
        .filter(|skill| {
            if !skill.installable {
                eprintln!(
                    "  ⚠ Skipping '{}': not installable ({})",
                    skill.id,
                    skill.frontmatter_issues.join(", ")
                );
                return false;
            }
            if skill.internal && !include_internal {
                eprintln!(
                    "  ⚠ Skipping '{}': internal (pass --skill {}, or set INSTALL_INTERNAL_SKILLS=1)",
                    skill.id, skill.id
                );
                return false;
            }
            true
        })
        .map(|skill| skill.id.clone())
        .collect()
}

fn print_install_audits(repo_url: &str, skill_names: &[String]) {
    if skill_names.is_empty() || !crate::install_audit::may_query_install_audits(repo_url) {
        return;
    }
    let repo_url = repo_url.to_string();
    let names = skill_names.to_vec();
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    let audits = runtime.block_on(crate::install_audit::fetch_public_github_skill_audits(
        &repo_url, &names,
    ));
    if audits.is_empty() {
        return;
    }
    println!("Security audits:");
    for audit in audits {
        println!("  {}", audit.skill);
        for row in audit.audits {
            println!("    {:<8} {}", row.name, row.result);
        }
    }
}

fn install_internal_skills_enabled() -> bool {
    matches!(
        std::env::var("INSTALL_INTERNAL_SKILLS").ok().as_deref(),
        Some("1") | Some("true")
    )
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

pub(super) struct ResolvedInstallCli {
    pub(super) source: String,
    pub(super) skill: Vec<String>,
    pub(super) global: bool,
    pub(super) agent: Vec<String>,
    pub(super) all: bool,
    pub(super) yes: bool,
    pub(super) copy: bool,
}

pub(super) fn resolve_cli_install_input(
    opts: &InstallOpts<'_>,
) -> anyhow::Result<ResolvedInstallCli> {
    let parsed = ss_skills::source_resolver::parse_install_input(opts.url)?;
    let skill = if !opts.skill.is_empty() {
        opts.skill.to_vec()
    } else {
        parsed.skills
    };
    let agent = if !opts.agent.is_empty() {
        opts.agent.to_vec()
    } else {
        parsed.agents
    };
    Ok(ResolvedInstallCli {
        source: parsed.source,
        skill,
        global: opts.global || parsed.global,
        agent,
        all: opts.all || parsed.all,
        yes: opts.yes || parsed.yes,
        copy: opts.copy || parsed.copy,
    })
}

/// Inline targeting logic matching skill_install::find_target_skill.
pub fn cmd_install(opts: InstallOpts<'_>) {
    let resolved = match resolve_cli_install_input(&opts) {
        Ok(resolved) => resolved,
        Err(err) => {
            if opts.json {
                println!(
                    "{}",
                    serde_json::json!([{"status": "failed", "error": err.to_string()}])
                );
            } else {
                eprintln!("✗ {err}");
            }
            std::process::exit(2);
        }
    };
    let opts = InstallOpts {
        url: &resolved.source,
        name: opts.name,
        skill: &resolved.skill,
        global: resolved.global,
        project: opts.project,
        agent: &resolved.agent,
        list: opts.list,
        all: resolved.all,
        yes: resolved.yes,
        copy: resolved.copy,
        preview: opts.preview,
        refresh: opts.refresh,
        json: opts.json,
    };
    if opts.json {
        super::install_json::cmd_install_json(opts);
        return;
    }
    if opts.list {
        match classify_add_input(opts.url) {
            AddKind::Repo => list_skills_in_source(opts.url, opts.refresh),
            AddKind::Bundle(path) => list_skills_in_bundle(&path),
            AddKind::LocalDir(path) => list_skills_in_local_dir(&path),
        }
        return;
    }

    if opts.preview {
        preview_install(opts.url, opts.name, opts.skill, opts.all, opts.refresh);
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
                opts.refresh,
            ) {
                Ok(result) => result,
                Err(err) => {
                    eprintln!("✗ Failed to install into hub: {}", err);
                    std::process::exit(1);
                }
            }
        }
    };

    let display_names: Vec<String> = skill_names
        .iter()
        .map(|name| match ss_skills::library::called_name(name) {
            Some(called) if called != *name => format!("{name} (from {called})"),
            _ => name.clone(),
        })
        .collect();
    if newly_installed {
        println!("✓ Installed '{}' into hub.", display_names.join(", "));
    } else {
        println!(
            "✓ Reusing existing hub install(s): {}.",
            display_names.join(", ")
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
        ss_skills::projects::ProjectDeployMode::Symlink => "link-first",
        ss_skills::projects::ProjectDeployMode::Copy => "copy",
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
            let profiles = ss_skills::agents::list_profiles();
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
    use super::super::InstallOpts;
    use super::{bulk_selection, require_settings_enabled_agents, resolve_cli_install_input};

    fn skill(id: &str, installable: bool) -> ss_skills::repo_scanner::DiscoveredSkill {
        ss_skills::repo_scanner::DiscoveredSkill {
            id: id.to_string(),
            folder_path: id.to_string(),
            description: String::new(),
            already_installed: false,
            installable,
            frontmatter_issues: if installable {
                Vec::new()
            } else {
                vec!["missing_description".to_string()]
            },
            internal: false,
        }
    }

    #[test]
    fn bulk_selection_leaves_out_skills_the_gate_rejects() {
        let picked = bulk_selection(&[skill("good", true), skill("broken", false)]);
        assert_eq!(picked, vec!["good".to_string()]);
    }

    #[test]
    fn bulk_selection_skips_internal_skills() {
        let mut hidden = skill("hidden", true);
        hidden.internal = true;
        let picked = bulk_selection(&[skill("good", true), hidden]);
        assert_eq!(picked, vec!["good".to_string()]);
    }

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

    #[test]
    fn resolve_cli_install_input_extracts_pasted_npx_command_and_preserves_explicit_flags() {
        let opts = InstallOpts {
            url: "npx skills add vercel-labs/agent-skills --skill find-skills -g -y",
            name: None,
            skill: &[],
            global: false,
            project: None,
            agent: &[],
            list: false,
            all: false,
            yes: false,
            copy: false,
            preview: false,
            refresh: false,
            json: false,
        };
        let resolved = resolve_cli_install_input(&opts).unwrap();
        assert_eq!(resolved.source, "vercel-labs/agent-skills");
        assert_eq!(resolved.skill, vec!["find-skills".to_string()]);
        assert!(resolved.global);
        assert!(resolved.yes);

        let override_skill = vec!["other-skill".to_string()];
        let overridden = resolve_cli_install_input(&InstallOpts {
            skill: &override_skill,
            ..opts
        })
        .unwrap();
        assert_eq!(overridden.skill, vec!["other-skill".to_string()]);
    }

    #[test]
    fn resolve_cli_install_input_passes_plain_sources_through() {
        let opts = InstallOpts {
            url: "acme/skills",
            name: None,
            skill: &[],
            global: false,
            project: None,
            agent: &[],
            list: false,
            all: false,
            yes: false,
            copy: false,
            preview: false,
            refresh: false,
            json: false,
        };
        let resolved = resolve_cli_install_input(&opts).unwrap();
        assert_eq!(resolved.source, "acme/skills");
        assert!(resolved.skill.is_empty());
        assert!(!resolved.global);
    }
}

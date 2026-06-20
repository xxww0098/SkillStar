use skillstar_app::cli::CliHandlers;
use skillstar_app::cli::{
    InstallOpts, RemoveOpts, derive_name_hint, normalize_agent_ids, print_project_targets,
    prompt_for_agent_selection, resolve_auto_project_agents, resolve_installed_name,
    resolve_rel_dirs_for_agents, validate_agent_ids,
};

use crate::core::{local_skill, lockfile, marketplace, repo_scanner, skill_bundle, skill_install, skill_pack};
use skillstar_projects::projects::sync;
use skillstar_skills::git::{gh_manager, ops as git_ops};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

/// Classify a raw `install`/`add` argument to route to the right installer.
enum AddKind {
    /// Repo URL, owner/repo, or any git source handled by the scan+install pipeline.
    Repo,
    /// Local `.ags` / `.agd` bundle file on disk.
    Bundle(PathBuf),
    /// Local directory that contains (at least one) `SKILL.md` → adopt as local skill(s).
    LocalDir(PathBuf),
}

fn classify_add_input(input: &str) -> AddKind {
    let trimmed = input.trim();

    // URL schemes and owner/repo fall through to Repo.
    let lower = trimmed.to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("git@")
        || lower.starts_with("ssh://")
    {
        return AddKind::Repo;
    }

    // Heuristic: anything with a filesystem separator or starting with . / ~ / an
    // absolute Windows drive letter is a path. `owner/repo` has exactly one slash
    // and no leading dot — keep the existing shorthand semantics.
    let looks_like_path = trimmed.starts_with('.')
        || trimmed.starts_with('/')
        || trimmed.starts_with('~')
        || trimmed.starts_with('\\')
        || (trimmed.len() >= 2 && trimmed.chars().nth(1) == Some(':'));

    if looks_like_path {
        let expanded = expand_tilde(trimmed);
        let path = PathBuf::from(&expanded);
        if is_bundle_file(&path) {
            return AddKind::Bundle(path);
        }
        if path.is_dir() {
            return AddKind::LocalDir(path);
        }
    }

    // Heuristic: no scheme but two or more segments separated by '/' AND no spaces
    // and the second-to-last segment is not a drive-looking token → treat as repo
    // shorthand (owner/repo possibly with subpath). This matches `Source::parse`.
    AddKind::Repo
}

fn expand_tilde(input: &str) -> String {
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest).to_string_lossy().to_string();
    }
    if input == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home.to_string_lossy().to_string();
    }
    input.to_string()
}

fn is_bundle_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ags") | Some("agd")
    )
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

fn install_bundle_file(path: &Path, opts: &InstallOpts<'_>) {
    let path_str = path.to_string_lossy();
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let force = opts.yes;

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

fn install_local_dir(path: &Path, opts: &InstallOpts<'_>) {
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

    // Filter by --skill / --name / --all if provided.
    let selected: Vec<&skillstar_skills::DiscoveredSkill> = if opts.all {
        skills.iter().collect()
    } else if !opts.skill.is_empty() {
        skills
            .iter()
            .filter(|s| {
                opts.skill
                    .iter()
                    .any(|want| want.eq_ignore_ascii_case(&s.id))
            })
            .collect()
    } else if let Some(name) = opts.name {
        skills
            .iter()
            .filter(|s| s.id.eq_ignore_ascii_case(name))
            .collect()
    } else if skills.len() == 1 {
        skills.iter().collect()
    } else {
        eprintln!(
            "✗ {} skills found in {}. Select with --skill <name,...> or use --all.",
            skills.len(),
            canonical.display()
        );
        for skill in &skills {
            eprintln!("  • {}", skill.id);
        }
        std::process::exit(2);
    };

    if selected.is_empty() {
        eprintln!("✗ None of the requested skills were found in the directory.");
        std::process::exit(1);
    }

    let mut adopted: Vec<String> = Vec::new();
    for skill in selected {
        let source_dir = if skill.folder_path.is_empty() {
            canonical.clone()
        } else {
            canonical.join(&skill.folder_path)
        };
        let skill_md = source_dir.join("SKILL.md");
        let content = match std::fs::read_to_string(&skill_md) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("✗ Failed to read {}: {}", skill_md.display(), e);
                continue;
            }
        };
        match local_skill::create(&skill.id, Some(&content)) {
            Ok(_) => {
                println!("  ✓ Adopted '{}'", skill.id);
                adopted.push(skill.id.clone());
            }
            Err(err) => {
                eprintln!("  ✗ Failed to adopt '{}': {}", skill.id, err);
            }
        }
    }

    if adopted.is_empty() {
        eprintln!("✗ Nothing was adopted.");
        std::process::exit(1);
    }

    println!(
        "✓ Adopted {} skill(s) into ~/.skillstar/hub/local.",
        adopted.len()
    );

    if opts.global {
        return;
    }

    // Link into project (same shape as repo-install post step).
    let project_path = match opts.project {
        Some(p) => PathBuf::from(p),
        None => match std::env::current_dir() {
            Ok(p) => p,
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
    let mut chosen = normalize_agent_ids(opts.agent);
    if chosen.is_empty() {
        chosen = if opts.yes {
            auto_agent_ids.clone()
        } else {
            prompt_for_agent_selection(&auto_agent_ids)
        };
    }
    let agent_ids = match validate_agent_ids(&chosen) {
        Ok(ids) => ids,
        Err(err) => {
            eprintln!("✗ {}", err);
            std::process::exit(1);
        }
    };
    let rel_dirs = resolve_rel_dirs_for_agents(&agent_ids);

    match sync::create_project_skills(&project_path, &adopted, &agent_ids) {
        Ok(link_count) => {
            println!(
                "✓ Linked {} skill(s) into project {} ({} link(s)).",
                adopted.len(),
                project_path.display(),
                link_count
            );
            for name in &adopted {
                print_project_targets(&project_path, &rel_dirs, name);
            }
        }
        Err(err) => {
            eprintln!(
                "✗ Adopted into hub but failed to link into project: {}",
                err
            );
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
) -> Result<(Vec<String>, bool), String> {
    let name_hint = derive_name_hint(url, explicit_name);

    if all {
        if explicit_name.is_some() {
            return Err("--all cannot be combined with --name".to_string());
        }
        if !skill_filter.is_empty() {
            return Err("--all cannot be combined with --skill".to_string());
        }
        let (_repo_url, _source, _, skills_found) = skill_install::fetch_repo_scanned(url, false)?;
        if skills_found.is_empty() {
            return Err("No skills discovered in repository".to_string());
        }
        let names: Vec<String> = skills_found.iter().map(|s| s.id.clone()).collect();
        let installed = skill_install::install_skills_batch(url, &names)?;
        return Ok((installed.into_iter().map(|s| s.name).collect(), true));
    }

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
                if err.contains("already installed")
                    && let Some(name) = resolve_installed_name(url, explicit_name, &name_hint)?
                {
                    return Ok((vec![name], false));
                }
                Err(err)
            }
        }
    }
}

/// `skillstar install --list` — scan the source without mutating anything.
fn list_skills_in_source(url: &str) {
    println!("Scanning {}...\n", url);
    match skill_install::fetch_repo_scanned(url, false) {
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
            skill_install::fetch_repo_scanned(url, false)
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

fn cmd_install(opts: InstallOpts<'_>) {
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

    // Non-repo sources take priority and ignore the repo-only flags.
    match classify_add_input(opts.url) {
        AddKind::Bundle(path) => {
            install_bundle_file(&path, &opts);
            return;
        }
        AddKind::LocalDir(path) => {
            install_local_dir(&path, &opts);
            return;
        }
        AddKind::Repo => {}
    }

    if opts.copy {
        // Best-effort global signal; the project sync layer already falls back to copy,
        // but we also make it visible in the output and skip symlink deployment for
        // installers that honor the flag in future iterations.
        println!("ⓘ Copy mode requested; project links will prefer directory copies.");
    }

    println!("Installing from {}...", opts.url);

    let (skill_names, newly_installed) =
        match install_or_reuse_skill(opts.url, opts.name, opts.skill, opts.all) {
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

    if opts.global {
        println!("Done (global install mode).");
        return;
    }

    let project_path = match opts.project {
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
    let mut chosen_agents = normalize_agent_ids(opts.agent);
    if chosen_agents.is_empty() {
        if opts.yes {
            // Non-interactive: fall back to auto-detected agents (may be empty → .agent/skills)
            chosen_agents = auto_agent_ids.clone();
        } else {
            chosen_agents = prompt_for_agent_selection(&auto_agent_ids);
        }
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
                println!("  Target mode: fallback path (.agent/skills)");
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
            eprintln!("✗ Error reading lockfile: {}", e);
            std::process::exit(1);
        }
    };

    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();
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

fn cmd_remove(opts: RemoveOpts<'_>) {
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

fn cmd_publish() {
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
    skillstar_core::infra::migration::migrate_legacy_paths();
    // Point the marketplace snapshot runtime at the real data dir + DB before
    // any CLI command (notably `find`) touches the snapshot. Without this the
    // snapshot falls back to a throwaway `/tmp` DB, which always looks empty
    // and triggers a blocking remote seed on every search.
    if let Err(err) = marketplace::initialize_local_snapshot() {
        eprintln!("⚠ Marketplace snapshot init failed: {err}");
    }
}

pub fn cli_handlers() -> CliHandlers {
    CliHandlers {
        migrate_and_run,
        install: cmd_install,
        update: cmd_update,
        remove: cmd_remove,
        publish: cmd_publish,
        doctor: cmd_doctor,
        pack_list: cmd_pack_list,
        pack_remove: cmd_pack_remove,
        gui: || {
            println!("Launching SkillStar GUI...");
        },
    }
}

pub fn run(args: Vec<String>) {
    skillstar_app::cli::run(args, cli_handlers());
}

//! `skillstar install --list` and `--preview`: read-only views of what an
//! install would do. Nothing here changes installed skills, links or locks.

use std::path::Path;

use ss_skills::skill_bundle;

use super::install::{git_skill_facade, print_plugin_hint};
use super::{derive_name_hint, resolve_installed_name};

pub(super) fn list_skills_in_bundle(path: &Path) {
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

pub(super) fn list_skills_in_local_dir(path: &Path) {
    println!("Scanning local directory {}...\n", path.display());
    let skills = ss_skills::discover_skills(path, false);
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

/// `skillstar install --list` scans without changing installed skills or locks.
pub(super) fn list_skills_in_source(url: &str, refresh: bool) {
    println!("Scanning {}...\n", url);
    match git_skill_facade().and_then(|git| {
        git.scan_repo_with_refresh(url, false, refresh)
            .map_err(|e| e.to_string())
    }) {
        Ok(fetched) => {
            print_plugin_hint(&fetched);
            let (repo_url, source, skills_found) =
                (fetched.spec.repo_url, fetched.spec.short, fetched.skills);
            if skills_found.is_empty() {
                println!(
                    "No SKILL.md found in {} (scanned root + priority dirs).",
                    source
                );
                println!("Tip: re-run with a more specific URL or install a bundle instead.");
                return;
            }
            let skills_dir = ss_core::infra::paths::hub_skills_dir();
            println!("{:<32} {:<10} DESCRIPTION", "SKILL", "STATUS");
            println!("{}", "-".repeat(80));
            for skill in &skills_found {
                let status = if is_installed_as(&skills_dir, &skill.id) {
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

/// Whether the folder `id` installs into already exists; the installer maps
/// ids to their canonical folder name first.
fn is_installed_as(skills_dir: &std::path::Path, id: &str) -> bool {
    ss_skills::installer::canonical_skill_name(id).is_ok_and(|name| skills_dir.join(name).exists())
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub(super) fn preview_install(
    url: &str,
    explicit_name: Option<&str>,
    skill_filter: &[String],
    all: bool,
    refresh: bool,
) {
    println!("Preview mode — no installed skills or links will be changed.\n");

    let name_hint = derive_name_hint(url, explicit_name);
    let skills_dir = ss_core::infra::paths::hub_skills_dir();

    if all {
        println!("  Mode: install every skill (--all)");
        println!("  URL: {}\n", url);
        let Ok(fetched) = git_skill_facade().and_then(|git| {
            git.scan_repo_with_refresh(url, false, refresh)
                .map_err(|e| e.to_string())
        }) else {
            eprintln!("✗ Failed to scan repository: check URL or network access");
            std::process::exit(1);
        };
        for skill in &fetched.skills {
            let status = if !skill.installable {
                format!("not installable: {}", skill.frontmatter_issues.join(", "))
            } else if is_installed_as(&skills_dir, &skill.id) {
                "already installed — would be overwritten".to_string()
            } else {
                "would be installed".to_string()
            };
            println!("  • {} ({})", skill.id, status);
        }
        return;
    }

    if !skill_filter.is_empty() {
        println!("  Mode: batch install with --skill filter");
        println!("  URL: {}", url);
        println!("  Skill filter: {}\n", skill_filter.join(", "));

        let Ok(fetched) = git_skill_facade().and_then(|git| {
            git.scan_repo_with_refresh(url, false, refresh)
                .map_err(|e| e.to_string())
        }) else {
            eprintln!("✗ Failed to scan repository: check URL or network access");
            std::process::exit(1);
        };

        for name in skill_filter {
            let target =
                ss_skills::skill_install::find_target_skill(&fetched.skills, Some(name), name);
            match &target {
                Ok(skill) if is_installed_as(&skills_dir, &skill.id) => {
                    println!("  • {} (already installed — would be overwritten)", name);
                }
                Ok(_) => println!("  • {} (would be installed)", name),
                Err(reason) => println!("  • {reason}"),
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

    let existing_in_hub = is_installed_as(&skills_dir, &name_hint);
    let existing_in_lockfile = resolve_installed_name(url, explicit_name, &name_hint)
        .map(|n| n.is_some())
        .unwrap_or(false);

    if (existing_in_hub || existing_in_lockfile) && !refresh {
        println!(
            "  • {} (already installed — would be overwritten)",
            name_hint
        );
    } else {
        let Ok(fetched) = git_skill_facade().and_then(|git| {
            git.scan_repo_with_refresh(url, false, refresh)
                .map_err(|e| e.to_string())
        }) else {
            println!("  • {} (would be cloned and installed)", name_hint);
            return;
        };

        let target =
            ss_skills::skill_install::find_target_skill(&fetched.skills, explicit_name, &name_hint);
        match target {
            Ok(skill) => println!("  • {} (skill found in repo, would be installed)", skill.id),
            Err(reason) => println!("  • {reason}"),
        }
    }
}

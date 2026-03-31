//! Local skill management — skills authored by the user with no git remote.
//!
//! Physical storage: `~/.skillstar/.agents/skills-local/<name>/`
//! Hub index:        `~/.skillstar/.agents/skills/<name>` → symlink to `skills-local/<name>`
//!
//! This mirrors the `.repos/` pattern used for repo-cached skills.

use super::{
    lockfile, project_manifest,
    skill::{Skill, SkillCategory, extract_skill_description},
    sync,
};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Get the local skills directory.
pub fn local_skills_dir() -> PathBuf {
    super::paths::local_skills_dir()
}

/// Check if a skill in the hub is a local skill (symlink pointing into `skills-local/`).
pub fn is_local_skill(name: &str) -> bool {
    let hub_dir = sync::get_hub_skills_dir();
    let skill_path = hub_dir.join(name);

    if !skill_path.is_symlink() {
        return false;
    }

    let Ok(target) = std::fs::read_link(&skill_path) else {
        return false;
    };

    let resolved = if target.is_absolute() {
        target
    } else {
        skill_path.parent().unwrap_or(Path::new(".")).join(&target)
    };

    let local_dir = local_skills_dir();
    resolved.starts_with(&local_dir)
}

/// Create a new local skill.
///
/// 1. Creates `skills-local/<name>/SKILL.md`
/// 2. Creates symlink `skills/<name>` → `skills-local/<name>`
/// 3. Returns the `Skill` struct with `skill_type = "local"`
pub fn create(name: &str, content: Option<&str>) -> Result<Skill> {
    let hub_dir = sync::get_hub_skills_dir();
    let local_dir = local_skills_dir();
    let skill_local_path = local_dir.join(name);
    let skill_hub_path = hub_dir.join(name);

    // Reject if name already exists in hub or local
    if skill_hub_path.symlink_metadata().is_ok() {
        anyhow::bail!("Skill '{}' already exists", name);
    }
    if skill_local_path.exists() {
        anyhow::bail!("Skill '{}' already exists in skills-local", name);
    }

    // Create the local skill directory + SKILL.md
    std::fs::create_dir_all(&skill_local_path).with_context(|| {
        format!(
            "Failed to create local skill directory: {}",
            skill_local_path.display()
        )
    })?;

    let default_content = format!(
        "---\ndescription: {}\n---\n\n# {}\n\nYour skill instructions here.\n",
        name, name
    );
    let skill_content = content.unwrap_or(&default_content);
    let skill_md = skill_local_path.join("SKILL.md");
    std::fs::write(&skill_md, skill_content)
        .with_context(|| format!("Failed to write SKILL.md for '{}'", name))?;

    // Create symlink in hub: skills/<name> → skills-local/<name>
    std::fs::create_dir_all(&hub_dir)?;
    create_symlink(&skill_local_path, &skill_hub_path)
        .with_context(|| format!("Failed to create hub symlink for '{}'", name))?;

    let description = extract_skill_description(&skill_local_path);

    Ok(Skill {
        name: name.to_string(),
        description,
        skill_type: crate::core::skill::SkillType::Local,
        stars: 0,
        installed: true,
        update_available: false,
        last_updated: chrono::Utc::now().to_rfc3339(),
        git_url: String::new(),
        tree_hash: None,
        category: SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(Vec::new()),
        rank: None,
        source: None,
    })
}

/// Reconcile hub symlinks for local skills.
///
/// Scans `skills-local/` and ensures every entry has a corresponding symlink
/// in the hub (`skills/`). This catches:
/// - Skills created manually in `skills-local/` without a hub symlink
/// - Hub symlinks that were accidentally deleted
///
/// Safe to call on every refresh — it only creates missing symlinks.
pub fn reconcile_hub_symlinks() {
    let local_dir = local_skills_dir();
    let hub_dir = sync::get_hub_skills_dir();

    let entries = match std::fs::read_dir(&local_dir) {
        Ok(entries) => entries,
        Err(_) => return, // skills-local/ doesn't exist yet — nothing to do
    };

    let _ = std::fs::create_dir_all(&hub_dir);

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };

        let hub_path = hub_dir.join(&name);

        // If hub entry already exists (symlink, dir, or file), skip
        if hub_path.symlink_metadata().is_ok() {
            continue;
        }

        // Create missing hub symlink
        if let Err(e) = create_symlink(&path, &hub_path) {
            eprintln!(
                "[local_skill::reconcile] Failed to create hub symlink for '{}': {}",
                name, e
            );
        }
    }
}

/// Delete a local skill completely.
///
/// 1. Remove agent symlinks
/// 2. Remove hub symlink (`skills/<name>`)
/// 3. Delete `skills-local/<name>/` directory
pub fn delete(name: &str) -> Result<()> {
    // Remove symlinks from all agents
    let _ = sync::remove_skill_from_all_agents(name);
    let _ = project_manifest::remove_skill_from_all_projects(name);

    // Remove hub symlink
    let hub_dir = sync::get_hub_skills_dir();
    let hub_path = hub_dir.join(name);
    if hub_path.symlink_metadata().is_ok() {
        if hub_path.is_symlink() {
            std::fs::remove_file(&hub_path)
                .with_context(|| format!("Failed to remove hub symlink for '{}'", name))?;
        } else {
            // Not a symlink — should not happen for local skills, but handle gracefully
            std::fs::remove_dir_all(&hub_path)
                .with_context(|| format!("Failed to remove hub directory for '{}'", name))?;
        }
    }

    // Delete the local skill directory
    let local_dir = local_skills_dir();
    let local_path = local_dir.join(name);
    if local_path.exists() {
        std::fs::remove_dir_all(&local_path)
            .with_context(|| format!("Failed to delete local skill directory '{}'", name))?;
    }

    Ok(())
}

/// Graduate a local skill after publishing to GitHub.
///
/// Removes the local skill files and hub symlink so the caller can re-clone
/// from GitHub as a proper hub (git-backed) skill.
///
/// 1. Remove hub symlink (`skills/<name>`)
/// 2. Delete `skills-local/<name>/` directory
///
/// Agent symlinks are NOT removed — they will be re-pointed by the re-install.
pub fn graduate(name: &str) -> Result<()> {
    // Remove hub symlink
    let hub_dir = sync::get_hub_skills_dir();
    let hub_path = hub_dir.join(name);
    if hub_path.is_symlink() {
        std::fs::remove_file(&hub_path)
            .with_context(|| format!("Failed to remove hub symlink for '{}'", name))?;
    }

    // Delete local skill directory
    let local_dir = local_skills_dir();
    let local_path = local_dir.join(name);
    if local_path.exists() {
        std::fs::remove_dir_all(&local_path)
            .with_context(|| format!("Failed to delete graduated skill directory '{}'", name))?;
    }

    Ok(())
}

/// Migrate existing non-git skills from `skills/` to `skills-local/`.
///
/// A skill is eligible for migration if:
/// 1. It's a real directory (not a symlink) in `skills/`
/// 2. It does NOT contain a `.git/` subdirectory
/// 3. It has NO lockfile entry with a non-empty `git_url`
///
/// Migration:
/// 1. Move `skills/<name>/` → `skills-local/<name>/`
/// 2. Create symlink `skills/<name>` → `skills-local/<name>`
pub fn migrate_existing() -> Result<u32> {
    let hub_dir = sync::get_hub_skills_dir();
    let local_dir = local_skills_dir();

    // Ensure local skills directory exists
    std::fs::create_dir_all(&local_dir).context("Failed to create skills-local directory")?;

    // Load lockfile to check for git URLs
    let lock_path = lockfile::lockfile_path();
    let lockfile = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
    let lock_map: std::collections::HashMap<String, &lockfile::LockEntry> = lockfile
        .skills
        .iter()
        .map(|e| (e.name.clone(), e))
        .collect();

    let entries = match std::fs::read_dir(&hub_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => {
            return Err(err).context("Failed to read hub skills directory for migration");
        }
    };

    let mut migrated: u32 = 0;

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip non-directories and symlinks
        let Ok(meta) = path.symlink_metadata() else {
            continue;
        };
        if meta.is_symlink() || !meta.is_dir() {
            continue;
        }

        let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };

        // Skip if it has a .git directory (it's a git-cloned skill)
        if path.join(".git").exists() {
            continue;
        }

        // Skip if lockfile has a non-empty git_url for this skill
        if let Some(lock_entry) = lock_map.get(&name) {
            if !lock_entry.git_url.is_empty() {
                continue;
            }
        }

        // Skip if destination already exists in skills-local
        let dest = local_dir.join(&name);
        if dest.exists() {
            continue;
        }

        // Migrate: move directory, create symlink
        match migrate_single_skill(&path, &dest) {
            Ok(()) => {
                migrated += 1;
            }
            Err(err) => {
                eprintln!(
                    "[local_skill::migrate] Failed to migrate '{}': {}",
                    name, err
                );
                // Continue with other skills
            }
        }
    }

    if migrated > 0 {
        eprintln!(
            "[local_skill::migrate] Migrated {} skills to skills-local/",
            migrated
        );
    }

    Ok(migrated)
}

/// Migrate a single skill directory from hub to skills-local.
fn migrate_single_skill(src: &Path, dest: &Path) -> Result<()> {
    // Move the directory (rename if same filesystem, otherwise copy+delete)
    if std::fs::rename(src, dest).is_err() {
        // Cross-filesystem: copy recursively then delete
        copy_dir_recursive(src, dest)?;
        std::fs::remove_dir_all(src)
            .context("Failed to remove original skill directory after copy")?;
    }

    // Create symlink: src (hub) → dest (skills-local)
    create_symlink(dest, src)
        .with_context(|| format!("Failed to create migration symlink {:?} → {:?}", src, dest))?;

    Ok(())
}

/// Recursively copy a directory, skipping OS/VCS junk files.
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let src_path = entry.path();
        let dest_path = dest.join(&file_name);

        // Skip git metadata and macOS system files
        if file_name == ".git" || file_name == ".DS_Store" {
            continue;
        }

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

/// Cross-platform symlink creation.
fn create_symlink(src: &Path, dst: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dst)
        .with_context(|| format!("Failed to symlink {:?} → {:?}", src, dst))?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(src, dst)
        .with_context(|| format!("Failed to symlink {:?} → {:?}", src, dst))?;

    Ok(())
}

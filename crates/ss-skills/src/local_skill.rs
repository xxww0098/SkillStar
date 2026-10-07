//! Local skill management — skills authored by the user with no git remote.
//!
//! Physical storage: `~/.skillstar/data/skills/local/<name>/`
//! Hub index:        `~/.skillstar/data/skills/installed/<name>` → symlink to local
//!
//! This mirrors the `.repos/` pattern used for repo-cached skills.

use crate::deployment;
use crate::projects;
use anyhow::{Context, Result};
use serde::Serialize;
use ss_core::types::{Skill, SkillCategory, extract_skill_description};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

mod intake;
mod repair;
pub use intake::{
    IntakeAction, IntakeApplyOptions, IntakeFinding, IntakeKind, IntakeOutcome, IntakePlan,
    IntakeReport, IntakeStatus, IntakeStep, IntakeStepOutcome, apply as apply_agent_intake,
    apply_with as apply_agent_intake_with, plan as plan_agent_intake, scan as scan_agent_intake,
};
pub use repair::{SkillRepairIssue, SkillRepairReport, repair_installations};

#[derive(Debug, Clone, Serialize)]
pub struct AdoptedSkill {
    pub name: String,
    pub folder_path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdoptLocalFolderResult {
    pub adopted: Vec<AdoptedSkill>,
    pub skipped: Vec<SkippedLocalSkill>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedLocalSkill {
    pub name: String,
    pub reason: String,
}

pub fn adopt_folder(
    folder_path: &str,
    names: Option<Vec<String>>,
) -> Result<AdoptLocalFolderResult> {
    let path = PathBuf::from(folder_path);
    if !path.is_dir() {
        anyhow::bail!("Not a directory: {}", path.display());
    }
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    let canonical = std::fs::canonicalize(&path)
        .with_context(|| format!("Failed to resolve {}", path.display()))?;
    let skills = crate::discover_skills(&canonical, false);
    if skills.is_empty() {
        anyhow::bail!(
            "No SKILL.md found in {} (root or priority dirs)",
            canonical.display()
        );
    }

    let requested: Option<Vec<String>> = names.map(|values| {
        values
            .into_iter()
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty())
            .collect()
    });
    let selected: Vec<_> = match &requested {
        Some(wanted) if !wanted.is_empty() => skills
            .iter()
            .filter(|skill| wanted.iter().any(|name| name == &skill.id.to_lowercase()))
            .collect(),
        _ => skills.iter().collect(),
    };
    if selected.is_empty() {
        anyhow::bail!("None of the requested skills were found in the folder");
    }

    let mut adopted = Vec::new();
    let mut skipped = Vec::new();
    for skill in selected {
        let source_dir = if skill.folder_path.is_empty() {
            canonical.clone()
        } else {
            canonical.join(&skill.folder_path)
        };

        // The frontmatter `name` becomes a folder under the data dir; it must
        // map to one safe canonical folder name (no `../`, no separators).
        let name = match crate::installer::canonical_skill_name(&skill.id) {
            Ok(name) => name,
            Err(error) => {
                skipped.push(SkippedLocalSkill {
                    name: skill.id.clone(),
                    reason: error.to_string(),
                });
                continue;
            }
        };
        if let Err(error) = crate::skill_mutation::policy().ensure_skill_mutation_allowed(&name) {
            skipped.push(SkippedLocalSkill {
                name: skill.id.clone(),
                reason: error.to_string(),
            });
            continue;
        }

        // Frontmatter quality gate (shared with the repo-install path): a
        // skill without a usable description is not adoptable.
        if let Err(reason) = crate::validation::ensure_installable(&source_dir) {
            skipped.push(SkippedLocalSkill {
                name: skill.id.clone(),
                reason,
            });
            continue;
        }

        // The hub entry must not already exist — a local skill is owned by
        // the hub, so adopting over another install would clobber it.
        let hub_dir = ss_core::infra::paths::hub_skills_dir();
        let hub_path = hub_dir.join(&name);
        let local_path = ss_core::infra::paths::local_skills_dir().join(&name);
        if hub_path.symlink_metadata().is_ok() || local_path.symlink_metadata().is_ok() {
            skipped.push(SkippedLocalSkill {
                name: skill.id.clone(),
                reason: "already exists".to_string(),
            });
            continue;
        }

        // Adopt the complete skill directory (SKILL.md + scripts/references/
        // assets), not just the manifest, so adopted skills keep working.
        match copy_adopted_skill(&name, &source_dir, &canonical) {
            Ok(created) => adopted.push(AdoptedSkill {
                name: created.name,
                folder_path: skill.folder_path.clone(),
                description: created.description,
            }),
            Err(error) => skipped.push(SkippedLocalSkill {
                name: skill.id.clone(),
                reason: error.to_string(),
            }),
        }
    }

    if !adopted.is_empty() {
        crate::installed_skill::invalidate_cache();
    }
    Ok(AdoptLocalFolderResult { adopted, skipped })
}

/// Copy a complete skill directory into `skills-local/` and expose it via a
/// hub symlink, rolling back the copy if the symlink cannot be created.
///
/// Unlike [`create`] (which writes only `SKILL.md`), adoption preserves the
/// full skill: scripts, references, templates, assets and Unix executable
/// state all keep working after the source folder is gone.
fn copy_adopted_skill(name: &str, source_dir: &Path, boundary: &Path) -> Result<Skill> {
    let local_path = ss_core::infra::paths::local_skills_dir().join(name);
    let hub_path = ss_core::infra::paths::hub_skills_dir().join(name);
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create skills-local directory: {}",
                parent.display()
            )
        })?;
    }
    if let Some(parent) = hub_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create hub skills directory: {}",
                parent.display()
            )
        })?;
    }

    let mut staged = crate::materialize::StagedReplace::stage(&local_path, |staging| {
        crate::materialize::copy_confined(source_dir, staging, boundary, ADOPT_COPY_EXCLUDES)
    })
    .with_context(|| format!("Failed to copy adopted skill '{name}' into skills-local"))?;
    staged.swap()?;
    staged.commit();
    if let Err(error) = ss_core::infra::fs_ops::create_symlink(&local_path, &hub_path) {
        let _ = ss_core::infra::fs_ops::remove_dir_all_retry(&local_path);
        return Err(error).with_context(|| format!("Failed to create hub symlink for '{name}'"));
    }
    crate::local_identity::replace_untrusted_sidecar(&local_path)
        .with_context(|| format!("Failed to mint local Skill identity for '{name}'"))?;

    let description = ss_core::types::extract_skill_description(&local_path);
    installed_local_skill(name, description)
}

/// Check if a skill in the hub is a local skill (symlink pointing into `skills-local/`).
pub fn is_local_skill(name: &str) -> bool {
    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    let skill_path = hub_dir.join(name);

    if !ss_core::infra::fs_ops::is_link(&skill_path) {
        return false;
    }

    let Ok(resolved) = ss_core::infra::fs_ops::read_link_resolved(&skill_path) else {
        return false;
    };

    // Intake stores an absolute target. On macOS that target is canonical
    // (`/private/var`) while `local_skills_dir()` still says `/var`.
    let local_dir = ss_core::infra::fs_ops::canonicalize_existing_prefix(
        &ss_core::infra::paths::local_skills_dir(),
    );
    let resolved = ss_core::infra::fs_ops::canonicalize_existing_prefix(&resolved);
    resolved.starts_with(&local_dir)
}

fn prepare_new_local_skill_paths(name: &str) -> Result<(PathBuf, PathBuf)> {
    crate::content::validate_skill_name(name)
        .map_err(|error| anyhow::anyhow!("Invalid local Skill name: {error}"))?;
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;

    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    let local_dir = ss_core::infra::paths::local_skills_dir();
    let skill_local_path = local_dir.join(name);
    let skill_hub_path = hub_dir.join(name);

    if skill_hub_path.symlink_metadata().is_ok() {
        anyhow::bail!("Skill '{}' already exists", name);
    }
    if skill_local_path.symlink_metadata().is_ok() {
        anyhow::bail!("Skill '{}' already exists in skills-local", name);
    }

    std::fs::create_dir_all(&local_dir).with_context(|| {
        format!(
            "Failed to create local skills directory: {}",
            local_dir.display()
        )
    })?;
    std::fs::create_dir_all(&hub_dir).with_context(|| {
        format!(
            "Failed to create hub skills directory: {}",
            hub_dir.display()
        )
    })?;

    Ok((skill_local_path, skill_hub_path))
}

/// Create a new local skill.
///
/// 1. Creates `skills-local/<name>/SKILL.md`
/// 2. Creates symlink `skills/<name>` → `skills-local/<name>`
/// 3. Returns the `Skill` struct with `skill_type = "local"`
pub fn create(name: &str, content: Option<&str>) -> Result<Skill> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    create_locked(name, content)
}

fn create_locked(name: &str, content: Option<&str>) -> Result<Skill> {
    let (skill_local_path, skill_hub_path) = prepare_new_local_skill_paths(name)?;

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
    ss_core::infra::fs_ops::create_symlink(&skill_local_path, &skill_hub_path)
        .with_context(|| format!("Failed to create hub symlink for '{}'", name))?;
    crate::local_identity::ensure_local_identity(&skill_local_path)
        .with_context(|| format!("Failed to mint local Skill identity for '{name}'"))?;

    let description = extract_skill_description(&skill_local_path);

    Ok(Skill {
        name: name.to_string(),
        description,
        localized_description: None,
        skill_type: ss_core::types::SkillType::Local,
        stars: 0,
        installed: true,
        update_available: false,
        upstream_change: None,
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

/// Preserve a bounded content snapshot as a new independently-owned local
/// Skill. The snapshot was captured before any source update, so every managed
/// regular file is copied from one coherent view of disk.
pub(crate) fn create_from_snapshot(
    name: &str,
    snapshot: &crate::content::SkillSnapshot,
) -> Result<Skill> {
    let (skill_local_path, skill_hub_path) = prepare_new_local_skill_paths(name)?;
    snapshot
        .materialize_owned_to(&skill_local_path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to preserve Skill as local copy '{name}'"))?;

    if let Err(error) = ss_core::infra::fs_ops::create_symlink(&skill_local_path, &skill_hub_path)
        .with_context(|| format!("Failed to create hub symlink for '{name}'"))
    {
        let _ = ss_core::infra::fs_ops::remove_dir_all_retry(&skill_local_path);
        return Err(error);
    }
    crate::local_identity::ensure_local_identity(&skill_local_path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to mint local Skill identity for '{name}'"))?;

    let description = extract_skill_description(&skill_local_path);
    installed_local_skill(name, description)
}

pub fn preserve_installed_copy(name: &str, local_name: &str) -> Result<Skill> {
    let snapshot = crate::content::snapshot(name)
        .with_context(|| format!("failed to capture local divergence for '{name}'"))?;
    let local_copy = create_from_snapshot(local_name, &snapshot)?;
    crate::installed_skill::invalidate_cache();
    Ok(local_copy)
}

fn installed_local_skill(name: &str, description: String) -> Result<Skill> {
    Ok(Skill {
        name: name.to_string(),
        description,
        localized_description: None,
        skill_type: ss_core::types::SkillType::Local,
        stars: 0,
        installed: true,
        update_available: false,
        upstream_change: None,
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

/// Adopt an existing skill directory into `skills-local/` and expose it via
/// the hub symlink in `skills/`.
///
/// This is used when SkillStar discovers a new unmanaged skill inside a
/// project-level agent folder and needs to normalize it into local-skill
/// storage without leaving a real directory behind in the hub.
pub(crate) fn adopt_existing_dir_locked(name: &str, source_dir: &Path) -> Result<PathBuf> {
    if !source_dir.is_dir() {
        anyhow::bail!(
            "Source skill directory '{}' does not exist or is not a directory",
            source_dir.display()
        );
    }

    let (skill_local_path, skill_hub_path) = prepare_new_local_skill_paths(name)?;

    move_dir(source_dir, &skill_local_path).with_context(|| {
        format!(
            "Failed to move discovered skill '{}' into skills-local",
            name
        )
    })?;

    if let Err(err) = ss_core::infra::fs_ops::create_symlink(&skill_local_path, &skill_hub_path)
        .with_context(|| format!("Failed to create hub symlink for '{}'", name))
    {
        if let Err(rollback_err) = move_dir(&skill_local_path, source_dir).with_context(|| {
            format!(
                "Failed to restore discovered skill '{}' after hub symlink error",
                name
            )
        }) {
            return Err(anyhow::anyhow!(
                "{}; rollback also failed: {}",
                err,
                rollback_err
            ));
        }
        return Err(err);
    }
    crate::local_identity::replace_untrusted_sidecar(&skill_local_path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to mint local Skill identity for '{name}'"))?;

    Ok(skill_local_path)
}

/// Reconcile hub symlinks for local skills.
///
/// Scans `skills-local/` and ensures every entry has a corresponding symlink
/// in the hub (`skills/`). This catches:
/// - Skills created manually in `skills-local/` without a hub symlink
/// - Hub symlinks that were accidentally deleted
///
/// Only creates missing symlinks. A repair step (process startup, health
/// repair), not part of listing: reads must not write.
pub fn reconcile_hub_symlinks() {
    let local_dir = ss_core::infra::paths::local_skills_dir();
    let hub_dir = ss_core::infra::paths::hub_skills_dir();

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

        match crate::skill_mutation::policy().managed_repository_for_skill(&name) {
            Ok(None) => {}
            Ok(Some(_)) => {
                warn!(
                    target: "local_skill",
                    skill = %name,
                    "skipping local symlink reconciliation for shared-channel-owned Skill"
                );
                continue;
            }
            Err(error) => {
                warn!(
                    target: "local_skill",
                    skill = %name,
                    error = %error,
                    "skipping local symlink reconciliation because channel ownership is unknown"
                );
                continue;
            }
        }

        // Create missing hub symlink
        if let Err(e) = ss_core::infra::fs_ops::create_symlink(&path, &hub_path) {
            warn!(
                target: "local_skill",
                skill = %name,
                error = %e,
                "failed to create hub symlink during reconcile"
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
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;
    // Remove symlinks from all agents
    let _ = deployment::remove_skill_from_all_agents(name);
    let _ = projects::remove_skill_from_all_projects(name);

    // Remove hub symlink
    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    let hub_path = hub_dir.join(name);
    if hub_path.symlink_metadata().is_ok() {
        if ss_core::infra::fs_ops::is_link(&hub_path) {
            ss_core::infra::fs_ops::remove_symlink(&hub_path)
                .with_context(|| format!("Failed to remove hub symlink for '{}'", name))?;
        } else {
            // Not a symlink — should not happen for local skills, but handle gracefully
            std::fs::remove_dir_all(&hub_path)
                .with_context(|| format!("Failed to remove hub directory for '{}'", name))?;
        }
    }

    // Delete the local skill directory
    let local_dir = ss_core::infra::paths::local_skills_dir();
    let local_path = local_dir.join(name);
    if local_path.exists() {
        std::fs::remove_dir_all(&local_path)
            .with_context(|| format!("Failed to delete local skill directory '{}'", name))?;
    }

    crate::skill_lock::mutate(|lock| {
        lock.remove(name);
    })
    .with_context(|| format!("Failed to remove the install lock entry for '{name}'"))?;

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
pub(crate) fn graduate(name: &str) -> Result<()> {
    // Remove hub symlink
    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    let hub_path = hub_dir.join(name);
    if ss_core::infra::fs_ops::is_link(&hub_path) {
        ss_core::infra::fs_ops::remove_symlink(&hub_path)
            .with_context(|| format!("Failed to remove hub symlink for '{}'", name))?;
    }

    // Delete local skill directory
    let local_dir = ss_core::infra::paths::local_skills_dir();
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
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    let local_dir = ss_core::infra::paths::local_skills_dir();

    // Ensure local skills directory exists
    std::fs::create_dir_all(&local_dir).context("Failed to create skills-local directory")?;

    // Load the install lock to check provenance URLs.
    let lock = crate::skill_lock::load();
    let lock_map: std::collections::HashMap<String, &crate::skill_lock::SkillLockEntry> = lock
        .skills
        .iter()
        .map(|(name, entry)| (name.clone(), entry))
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
        if ss_core::infra::fs_ops::is_link(&path) || !meta.is_dir() {
            continue;
        }

        let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };

        if let Err(error) = crate::skill_mutation::policy().ensure_skill_mutation_allowed(&name) {
            warn!(
                target: "local_skill",
                skill = %name,
                error = %error,
                "skipped local migration because Skill ownership is managed or unavailable"
            );
            continue;
        }

        // Skip if it has a .git directory (it's a git-cloned skill)
        if path.join(".git").exists() {
            continue;
        }

        // Skip if lockfile has a non-empty git_url for this skill
        if let Some(lock_entry) = lock_map.get(&name)
            && !lock_entry.source_url.is_empty()
        {
            continue;
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
                warn!(
                    target: "local_skill",
                    skill = %name,
                    error = %err,
                    "failed to migrate skill"
                );
                // Continue with other skills
            }
        }
    }

    if migrated > 0 {
        info!(
            target: "local_skill",
            count = migrated,
            "migrated skills to skills-local/"
        );
    }

    Ok(migrated)
}

/// Migrate a single skill directory from hub to skills-local.
fn migrate_single_skill(src: &Path, dest: &Path) -> Result<()> {
    move_dir(src, dest)?;

    // Create symlink: src (hub) → dest (skills-local)
    ss_core::infra::fs_ops::create_symlink(dest, src)
        .with_context(|| format!("Failed to create migration symlink {:?} → {:?}", src, dest))?;
    crate::local_identity::ensure_local_identity(dest).with_context(|| {
        format!(
            "Failed to mint local Skill identity during migration to {}",
            dest.display()
        )
    })?;

    Ok(())
}

fn move_dir(src: &Path, dest: &Path) -> Result<()> {
    // Move the directory (rename if same filesystem, otherwise copy+delete)
    if std::fs::rename(src, dest).is_err() {
        // Cross-filesystem: copy recursively then delete
        copy_dir_recursive(src, dest)?;
        std::fs::remove_dir_all(src)
            .context("Failed to remove original skill directory after copy")?;
    }

    Ok(())
}

/// Turn an unlocked canonical folder into a local Skill: copy it into local
/// storage, then replace the canonical folder with a link to the copy. The
/// original goes only after the link is in place; any failure restores it.
pub(crate) fn adopt_canonical(name: &str) -> Result<()> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    crate::content::validate_skill_name(name)?;
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;
    let hub_path = ss_core::infra::paths::hub_skills_dir().join(name);
    let local_path = ss_core::infra::paths::local_skills_dir().join(name);
    if ss_core::infra::fs_ops::is_link(&hub_path) || !hub_path.is_dir() {
        anyhow::bail!("{} is not a canonical Skill folder", hub_path.display());
    }
    if hub_path.join(".git").exists() {
        anyhow::bail!(
            "{} is a Git working tree; adopting it would drop its history",
            hub_path.display()
        );
    }
    if local_path.symlink_metadata().is_ok() {
        anyhow::bail!("A local Skill named '{name}' already exists; nothing was adopted");
    }
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut local = crate::materialize::StagedReplace::stage(&local_path, |staging| {
        crate::materialize::copy_confined(&hub_path, staging, &hub_path, ADOPT_COPY_EXCLUDES)
    })?;
    local.swap()?;
    let mut link = crate::materialize::StagedReplace::stage(&hub_path, |staging| {
        ss_core::infra::fs_ops::create_symlink(&local_path, staging)
    })?;
    link.swap()?;
    crate::local_identity::ensure_local_identity(&local_path)?;
    link.commit();
    local.commit();
    crate::installed_skill::invalidate_cache();
    Ok(())
}

/// Git metadata and OS junk never carried into local storage.
const ADOPT_COPY_EXCLUDES: &[&str] = &[".git", ".DS_Store", "Thumbs.db", "desktop.ini"];

/// Copy a directory without following links that leave it.
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    crate::materialize::copy_confined(src, dest, src, ADOPT_COPY_EXCLUDES)
}

#[cfg(test)]
mod adopt_folder_tests;

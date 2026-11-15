//! Local skill management — skills authored by the user with no git remote.
//!
//! Under the v4 unified layout (D-113), local skills live directly alongside
//! remote skills as real directories in `~/.skillstar/data/skills/installed/<name>/`
//! carrying a `.skillstar/identity.json` UUID sidecar and `origin: "local"` in
//! `data/skills/library.json` (no `data/skills/local/` double-hop symlink).

use anyhow::{Context, Result};
use serde::Serialize;
use ss_core::types::{Skill, SkillCategory, extract_skill_description};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

mod intake;
mod removal;
mod repair;
pub use intake::{
    IntakeAction, IntakeApplyOptions, IntakeFinding, IntakeKind, IntakeOutcome, IntakePlan,
    IntakeReport, IntakeStatus, IntakeStep, IntakeStepOutcome, apply as apply_agent_intake,
    apply_with as apply_agent_intake_with, plan as plan_agent_intake, scan as scan_agent_intake,
};
pub(crate) use removal::{delete_files_and_lock, unlink_hub_and_lock};
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

/// Copy a complete skill directory directly into `data/skills/installed/<name>/`
/// as a local skill with a `.skillstar/identity.json` sidecar and library record.
///
/// Unlike [`create`] (which writes only `SKILL.md`), adoption preserves the
/// full skill: scripts, references, templates, assets and Unix executable
/// state all keep working after the source folder is gone.
fn copy_adopted_skill(name: &str, source_dir: &Path, boundary: &Path) -> Result<Skill> {
    let dest_path = ss_core::infra::paths::agents_skill_dir(name);
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create installed skills directory: {}",
                parent.display()
            )
        })?;
    }

    let mut staged = crate::materialize::StagedReplace::stage(&dest_path, |staging| {
        crate::materialize::copy_confined(source_dir, staging, boundary, ADOPT_COPY_EXCLUDES)
    })
    .with_context(|| format!("Failed to copy adopted skill '{name}' into installed skills"))?;
    staged.swap()?;
    if let Err(error) = crate::local_identity::replace_untrusted_sidecar(&dest_path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to mint local Skill identity for '{name}'"))
    {
        drop(staged);
        return Err(error);
    }
    if let Err(error) = crate::library::record_local_skill(name)
        .with_context(|| format!("Failed to record local skill '{name}' in library.json"))
    {
        drop(staged);
        return Err(error);
    }
    staged.commit();

    let description = ss_core::types::extract_skill_description(&dest_path);
    installed_local_skill(name, description)
}

/// Check if `name` is a user-authored local skill.
///
/// True for:
/// - legacy v3 hub symlinks targeting `data/skills/local/<name>` (until migrated)
/// - v4 real directories in `data/skills/installed/<name>` that are not
///   channel-owned and either carry `origin: "local"` in `library.json` or hold
///   a `.skillstar/identity.json` sidecar without a non-local `.skill-lock.json`
///   entry.
pub fn is_local_skill(name: &str) -> bool {
    let skill_path = ss_core::infra::paths::hub_skills_dir().join(name);

    if ss_core::infra::fs_ops::is_link(&skill_path) {
        let Ok(resolved) = ss_core::infra::fs_ops::read_link_resolved(&skill_path) else {
            return false;
        };
        let legacy_local_dir = ss_core::infra::fs_ops::canonicalize_existing_prefix(
            &ss_core::infra::paths::legacy_local_skills_dir(),
        );
        let resolved = ss_core::infra::fs_ops::canonicalize_existing_prefix(&resolved);
        return resolved.starts_with(&legacy_local_dir);
    }

    if !skill_path.is_dir() {
        return false;
    }

    if matches!(
        crate::skill_mutation::skill_is_channel_managed(name),
        Ok(true)
    ) {
        return false;
    }

    if crate::library::is_local_origin(name) {
        return true;
    }

    let has_remote_lock = crate::skill_lock::load()
        .entry_for_folder(name)
        .is_some_and(|(_, entry)| entry.source_type != crate::skill_lock::SourceType::Local);
    !has_remote_lock
        && matches!(
            crate::local_identity::read_local_identity(&skill_path),
            Ok(Some(_))
        )
}

fn prepare_new_local_skill_path(name: &str) -> Result<PathBuf> {
    crate::content::validate_skill_name(name)
        .map_err(|error| anyhow::anyhow!("Invalid local Skill name: {error}"))?;
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;

    let installed_dir = ss_core::infra::paths::agents_skills_root();
    let legacy_local = ss_core::infra::paths::legacy_local_skills_dir().join(name);
    let skill_path = installed_dir.join(name);

    if skill_path.symlink_metadata().is_ok() {
        anyhow::bail!("Skill '{}' already exists", name);
    }
    if legacy_local != skill_path && legacy_local.symlink_metadata().is_ok() {
        anyhow::bail!("Skill '{}' already exists in legacy local storage", name);
    }

    std::fs::create_dir_all(&installed_dir).with_context(|| {
        format!(
            "Failed to create installed skills directory: {}",
            installed_dir.display()
        )
    })?;

    Ok(skill_path)
}

/// Create a new local skill directly in `data/skills/installed/<name>/`.
///
/// 1. Creates `installed/<name>/SKILL.md`
/// 2. Mints `.skillstar/identity.json`
/// 3. Records `origin: "local"` in `data/skills/library.json`
/// 4. Returns the `Skill` struct with `skill_type = "local"`
pub fn create(name: &str, content: Option<&str>) -> Result<Skill> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    create_locked(name, content)
}

fn create_locked(name: &str, content: Option<&str>) -> Result<Skill> {
    let skill_path = prepare_new_local_skill_path(name)?;

    std::fs::create_dir_all(&skill_path).with_context(|| {
        format!(
            "Failed to create local skill directory: {}",
            skill_path.display()
        )
    })?;

    let default_content = format!(
        "---\ndescription: {}\n---\n\n# {}\n\nYour skill instructions here.\n",
        name, name
    );
    let skill_content = content.unwrap_or(&default_content);
    let skill_md = skill_path.join("SKILL.md");
    std::fs::write(&skill_md, skill_content)
        .with_context(|| format!("Failed to write SKILL.md for '{}'", name))?;

    if let Err(error) = crate::local_identity::ensure_local_identity(&skill_path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to mint local Skill identity for '{name}'"))
    {
        let _ = ss_core::infra::fs_ops::remove_dir_all_retry(&skill_path);
        return Err(error);
    }
    if let Err(error) = crate::library::record_local_skill(name)
        .with_context(|| format!("Failed to record local skill '{name}' in library.json"))
    {
        let _ = ss_core::infra::fs_ops::remove_dir_all_retry(&skill_path);
        return Err(error);
    }

    let description = extract_skill_description(&skill_path);
    installed_local_skill(name, description)
}

/// Preserve a bounded content snapshot as a new independently-owned local
/// Skill directly in `installed/<name>/`.
pub(crate) fn create_from_snapshot(
    name: &str,
    snapshot: &crate::content::SkillSnapshot,
) -> Result<Skill> {
    let skill_path = prepare_new_local_skill_path(name)?;
    snapshot
        .materialize_owned_to(&skill_path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to preserve Skill as local copy '{name}'"))?;

    if let Err(error) = crate::local_identity::ensure_local_identity(&skill_path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to mint local Skill identity for '{name}'"))
    {
        let _ = ss_core::infra::fs_ops::remove_dir_all_retry(&skill_path);
        return Err(error);
    }
    if let Err(error) = crate::library::record_local_skill(name)
        .with_context(|| format!("Failed to record local skill '{name}' in library.json"))
    {
        let _ = ss_core::infra::fs_ops::remove_dir_all_retry(&skill_path);
        return Err(error);
    }

    let description = extract_skill_description(&skill_path);
    installed_local_skill(name, description)
}

pub fn preserve_installed_copy(name: &str, local_name: &str) -> Result<Skill> {
    let snapshot = crate::content::snapshot(name)
        .with_context(|| format!("failed to capture local divergence for '{name}'"))?;
    let target_installed = ss_core::infra::paths::agents_skill_dir(local_name);
    // Two-phase channel conversion: if `local_name` collides with the still-
    // present canonical channel copy, stage in `.preserve-staging/<local_name>`
    // so `uninstall_hub_skill_with_commit` can promote it after commit.
    let local_copy = if local_name == name && target_installed.symlink_metadata().is_ok() {
        crate::content::validate_skill_name(local_name)
            .map_err(|error| anyhow::anyhow!("Invalid local Skill name: {error}"))?;
        let staging_root = ss_core::infra::paths::skills_preserve_staging_dir();
        std::fs::create_dir_all(&staging_root)?;
        let staged = staging_root.join(local_name);
        if staged.symlink_metadata().is_ok() {
            ss_core::infra::fs_ops::remove_dir_all_retry(&staged)?;
        }
        snapshot
            .materialize_owned_to(&staged)
            .map_err(anyhow::Error::from)
            .with_context(|| format!("Failed to stage preserved copy '{local_name}'"))?;
        if let Err(error) = crate::local_identity::ensure_local_identity(&staged)
            .map_err(anyhow::Error::from)
            .with_context(|| format!("Failed to mint local Skill identity for '{local_name}'"))
        {
            let _ = ss_core::infra::fs_ops::remove_dir_all_retry(&staged);
            return Err(error);
        }
        let description = extract_skill_description(&staged);
        installed_local_skill(local_name, description)?
    } else {
        create_from_snapshot(local_name, &snapshot)?
    };
    crate::installed_skill::invalidate_cache();
    Ok(local_copy)
}

/// Promote a staged two-phase channel conversion copy into `installed/<name>/`
/// once the channel entry has been uninstalled.
pub(crate) fn commit_preserved_staging(name: &str) -> Result<bool> {
    let staging_root = ss_core::infra::paths::skills_preserve_staging_dir();
    let staged = staging_root.join(name);
    if !staged.is_dir() {
        return Ok(false);
    }
    let dest = ss_core::infra::paths::agents_skill_dir(name);
    if dest.symlink_metadata().is_ok() {
        anyhow::bail!(
            "Cannot promote preserved copy for '{name}': destination '{}' already exists",
            dest.display()
        );
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    move_dir(&staged, &dest)?;
    crate::local_identity::ensure_local_identity(&dest)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to ensure local identity for '{name}'"))?;
    crate::library::record_local_skill(name)?;
    if staging_root
        .read_dir()
        .map(|mut d| d.next().is_none())
        .unwrap_or(false)
    {
        let _ = std::fs::remove_dir(&staging_root);
    }
    crate::installed_skill::invalidate_cache();
    Ok(true)
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
        called: crate::library::called_name(name),
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

    let skill_path = prepare_new_local_skill_path(name)?;

    move_dir(source_dir, &skill_path).with_context(|| {
        format!(
            "Failed to move discovered skill '{}' into installed skills",
            name
        )
    })?;

    if let Err(err) = crate::local_identity::replace_untrusted_sidecar(&skill_path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("Failed to mint local Skill identity for '{name}'"))
    {
        let _ = move_dir(&skill_path, source_dir);
        return Err(err);
    }
    if let Err(err) = crate::library::record_local_skill(name)
        .with_context(|| format!("Failed to record local skill '{name}' in library.json"))
    {
        let _ = move_dir(&skill_path, source_dir);
        return Err(err);
    }

    Ok(skill_path)
}

/// Reconcile v3 local skills into the unified v4 `installed/` directory and
/// refresh `library.json`.
pub fn reconcile_hub_symlinks() {
    crate::storage_migration::migrate_v3_local_skills_in_current_root();
    if ss_core::infra::paths::agents_skills_root().is_dir()
        && let Err(error) = crate::library::reconcile_library_from_disk()
    {
        warn!(
            target: "local_skill",
            error = %error,
            "failed to reconcile library.json during startup repair"
        );
    }
}

/// Delete a local skill completely (or discard staged channel-conversion copy
/// during rollback).
pub fn delete(name: &str) -> Result<()> {
    let staging_root = ss_core::infra::paths::skills_preserve_staging_dir();
    let staged = staging_root.join(name);
    if staged.symlink_metadata().is_ok() {
        let channel_managed = matches!(
            crate::skill_mutation::policy().managed_repository_for_skill(name),
            Ok(Some(_))
        );
        ss_core::infra::fs_ops::remove_dir_all_retry(&staged)
            .with_context(|| format!("Failed to discard staged local copy '{name}'"))?;
        if staging_root
            .read_dir()
            .map(|mut d| d.next().is_none())
            .unwrap_or(false)
        {
            let _ = std::fs::remove_dir(&staging_root);
        }
        if channel_managed {
            return Ok(());
        }
    }
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;
    crate::skill_install::remove_local_skill(name)
}

/// Graduate a local skill after publishing to GitHub.
///
/// Removes the local skill files and hub/library records so the caller can
/// re-clone from GitHub as a proper remote skill.
pub(crate) fn graduate(name: &str) -> Result<()> {
    let hub_path = ss_core::infra::paths::hub_skills_dir().join(name);
    if ss_core::infra::fs_ops::is_link(&hub_path) {
        ss_core::infra::fs_ops::remove_symlink(&hub_path)
            .with_context(|| format!("Failed to remove hub symlink for '{}'", name))?;
    } else if hub_path.is_dir() {
        ss_core::infra::fs_ops::remove_dir_all_retry(&hub_path)
            .with_context(|| format!("Failed to delete graduated skill directory '{}'", name))?;
    }

    let legacy_path = ss_core::infra::paths::legacy_local_skills_dir().join(name);
    if legacy_path != hub_path && legacy_path.is_dir() {
        ss_core::infra::fs_ops::remove_dir_all_retry(&legacy_path)
            .with_context(|| format!("Failed to delete legacy local skill '{}'", name))?;
    }
    let _ = crate::library::forget_skill(name);

    Ok(())
}

/// Mark existing non-git skills in `data/skills/installed/` as local skills
/// in-place (`library.json` `origin: "local"` + `.skillstar/identity.json`).
pub fn migrate_existing() -> Result<u32> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    let hub_dir = ss_core::infra::paths::hub_skills_dir();

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
            return Err(err).context("Failed to read installed skills directory for migration");
        }
    };

    let mut migrated: u32 = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = path.symlink_metadata() else {
            continue;
        };
        if ss_core::infra::fs_ops::is_link(&path) || !meta.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if let Err(error) = crate::skill_mutation::policy().ensure_skill_mutation_allowed(&name) {
            warn!(
                target: "local_skill",
                skill = %name,
                error = %error,
                "skipped local migration because Skill ownership is managed or unavailable"
            );
            continue;
        }
        if path.join(".git").exists() {
            continue;
        }
        if let Some(lock_entry) = lock_map.get(&name)
            && !lock_entry.source_url.is_empty()
        {
            continue;
        }
        if crate::library::is_local_origin(&name)
            && matches!(crate::local_identity::read_local_identity(&path), Ok(Some(_)))
        {
            continue;
        }

        match adopt_canonical_dir_in_place(&name, &path) {
            Ok(()) => migrated += 1,
            Err(err) => warn!(
                target: "local_skill",
                skill = %name,
                error = %err,
                "failed to mark skill as local"
            ),
        }
    }

    if migrated > 0 {
        info!(
            target: "local_skill",
            count = migrated,
            "marked unmanaged skills in installed/ as local"
        );
    }

    Ok(migrated)
}

fn adopt_canonical_dir_in_place(name: &str, dir: &Path) -> Result<()> {
    crate::local_identity::ensure_local_identity(dir).with_context(|| {
        format!(
            "Failed to mint local Skill identity during local adoption of {}",
            dir.display()
        )
    })?;
    crate::library::record_local_skill(name)?;
    Ok(())
}

fn move_dir(src: &Path, dest: &Path) -> Result<()> {
    if std::fs::rename(src, dest).is_err() {
        copy_dir_recursive(src, dest)?;
        std::fs::remove_dir_all(src)
            .context("Failed to remove original skill directory after copy")?;
    }
    Ok(())
}

/// Turn an unlocked canonical folder into a local Skill in-place (minting
/// `.skillstar/identity.json` and recording `origin: "local"` in `library.json`).
pub(crate) fn adopt_canonical(name: &str) -> Result<()> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    crate::content::validate_skill_name(name)?;
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;
    let hub_path = ss_core::infra::paths::hub_skills_dir().join(name);
    if ss_core::infra::fs_ops::is_link(&hub_path) || !hub_path.is_dir() {
        anyhow::bail!("{} is not a canonical Skill folder", hub_path.display());
    }
    if hub_path.join(".git").exists() {
        anyhow::bail!(
            "{} is a Git working tree; adopting it would drop its history",
            hub_path.display()
        );
    }
    if is_local_skill(name) {
        return Ok(());
    }
    crate::local_identity::ensure_local_identity(&hub_path)?;
    crate::library::record_local_skill(name)?;
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

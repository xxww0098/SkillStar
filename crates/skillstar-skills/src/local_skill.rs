//! Local skill management — skills authored by the user with no git remote.
//!
//! Physical storage: `~/.skillstar/.agents/skills-local/<name>/`
//! Hub index:        `~/.skillstar/.agents/skills/<name>` → symlink to `skills-local/<name>`
//!
//! This mirrors the `.repos/` pattern used for repo-cached skills.

use crate::deployment;
use crate::{lockfile, projects};
use anyhow::{Context, Result};
use serde::Serialize;
use skillstar_core::types::{Skill, SkillCategory, extract_skill_description};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

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
        let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
        let hub_path = hub_dir.join(&skill.id);
        let local_path = skillstar_core::infra::paths::local_skills_dir().join(&skill.id);
        if hub_path.symlink_metadata().is_ok() || local_path.symlink_metadata().is_ok() {
            skipped.push(SkippedLocalSkill {
                name: skill.id.clone(),
                reason: "already exists".to_string(),
            });
            continue;
        }

        // Adopt the complete skill directory (SKILL.md + scripts/references/
        // assets), not just the manifest, so adopted skills keep working.
        match copy_adopted_skill(&skill.id, &source_dir) {
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
fn copy_adopted_skill(name: &str, source_dir: &Path) -> Result<Skill> {
    let local_path = skillstar_core::infra::paths::local_skills_dir().join(name);
    let hub_path = skillstar_core::infra::paths::hub_skills_dir().join(name);
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

    skillstar_core::infra::fs_ops::create_copy_deploy(source_dir, &local_path)
        .with_context(|| format!("Failed to copy adopted skill '{name}' into skills-local"))?;
    if let Err(error) = skillstar_core::infra::fs_ops::create_symlink(&local_path, &hub_path) {
        let _ = skillstar_core::infra::fs_ops::remove_dir_all_retry(&local_path);
        return Err(error).with_context(|| format!("Failed to create hub symlink for '{name}'"));
    }

    let description = skillstar_core::types::extract_skill_description(&local_path);
    installed_local_skill(name, description)
}

/// Check if a skill in the hub is a local skill (symlink pointing into `skills-local/`).
pub fn is_local_skill(name: &str) -> bool {
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let skill_path = hub_dir.join(name);

    if !skillstar_core::infra::fs_ops::is_link(&skill_path) {
        return false;
    }

    let Ok(resolved) = skillstar_core::infra::fs_ops::read_link_resolved(&skill_path) else {
        return false;
    };

    let local_dir = skillstar_core::infra::paths::local_skills_dir();
    resolved.starts_with(&local_dir)
}

fn prepare_new_local_skill_paths(name: &str) -> Result<(PathBuf, PathBuf)> {
    crate::content::validate_skill_name(name)
        .map_err(|error| anyhow::anyhow!("Invalid local Skill name: {error}"))?;
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;

    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let local_dir = skillstar_core::infra::paths::local_skills_dir();
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
    skillstar_core::infra::fs_ops::create_symlink(&skill_local_path, &skill_hub_path)
        .with_context(|| format!("Failed to create hub symlink for '{}'", name))?;

    let description = extract_skill_description(&skill_local_path);

    Ok(Skill {
        name: name.to_string(),
        description,
        localized_description: None,
        skill_type: skillstar_core::types::SkillType::Local,
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

    if let Err(error) =
        skillstar_core::infra::fs_ops::create_symlink(&skill_local_path, &skill_hub_path)
            .with_context(|| format!("Failed to create hub symlink for '{name}'"))
    {
        let _ = skillstar_core::infra::fs_ops::remove_dir_all_retry(&skill_local_path);
        return Err(error);
    }

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
        skill_type: skillstar_core::types::SkillType::Local,
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

    if let Err(err) =
        skillstar_core::infra::fs_ops::create_symlink(&skill_local_path, &skill_hub_path)
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

    Ok(skill_local_path)
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
    let local_dir = skillstar_core::infra::paths::local_skills_dir();
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();

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
        if let Err(e) = skillstar_core::infra::fs_ops::create_symlink(&path, &hub_path) {
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
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let hub_path = hub_dir.join(name);
    if hub_path.symlink_metadata().is_ok() {
        if skillstar_core::infra::fs_ops::is_link(&hub_path) {
            skillstar_core::infra::fs_ops::remove_symlink(&hub_path)
                .with_context(|| format!("Failed to remove hub symlink for '{}'", name))?;
        } else {
            // Not a symlink — should not happen for local skills, but handle gracefully
            std::fs::remove_dir_all(&hub_path)
                .with_context(|| format!("Failed to remove hub directory for '{}'", name))?;
        }
    }

    // Delete the local skill directory
    let local_dir = skillstar_core::infra::paths::local_skills_dir();
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
pub(crate) fn graduate(name: &str) -> Result<()> {
    // Remove hub symlink
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let hub_path = hub_dir.join(name);
    if skillstar_core::infra::fs_ops::is_link(&hub_path) {
        skillstar_core::infra::fs_ops::remove_symlink(&hub_path)
            .with_context(|| format!("Failed to remove hub symlink for '{}'", name))?;
    }

    // Delete local skill directory
    let local_dir = skillstar_core::infra::paths::local_skills_dir();
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
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let local_dir = skillstar_core::infra::paths::local_skills_dir();

    // Ensure local skills directory exists
    std::fs::create_dir_all(&local_dir).context("Failed to create skills-local directory")?;

    // Load lockfile to check for git URLs
    let lock_path = lockfile::lockfile_path();
    let lockfile = lockfile::Lockfile::load(&lock_path)
        .context("Failed to verify Skill ownership before local migration")?;
    let lock_map: std::collections::HashMap<String, &crate::lockfile::LockEntry> = lockfile
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
        if skillstar_core::infra::fs_ops::is_link(&path) || !meta.is_dir() {
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
            && !lock_entry.git_url.is_empty()
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
    skillstar_core::infra::fs_ops::create_symlink(dest, src)
        .with_context(|| format!("Failed to create migration symlink {:?} → {:?}", src, dest))?;

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

/// Recursively copy a directory, skipping OS/VCS junk files.
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let src_path = entry.path();
        let dest_path = dest.join(&file_name);

        // Skip git metadata and OS system files
        if file_name == ".git"
            || file_name == ".DS_Store"
            || file_name == "Thumbs.db"
            || file_name == "desktop.ini"
        {
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

#[cfg(test)]
mod adopt_folder_tests {
    use super::*;
    use std::ffi::OsString;

    struct Sandbox {
        previous: Vec<(&'static str, Option<OsString>)>,
        _temp: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Sandbox {
        fn new() -> Self {
            let _guard = crate::lock_test_env();
            let temp = tempfile::tempdir().unwrap();
            let overrides = [
                ("SKILLSTAR_HUB_DIR", temp.path().join("hub")),
                ("SKILLSTAR_DATA_DIR", temp.path().join("data")),
                ("SKILLSTAR_TOOL_SYNC_HOME", temp.path().join("tool-home")),
                ("HOME", temp.path().join("home")),
                ("USERPROFILE", temp.path().join("home")),
            ];
            let previous = overrides
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            unsafe {
                for (key, value) in overrides {
                    std::env::set_var(key, value);
                }
            }
            Self {
                previous,
                _temp: temp,
                _guard,
            }
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            unsafe {
                for (key, previous) in self.previous.drain(..).rev() {
                    match previous {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    fn valid_skill(root: &Path, name: &str) {
        let dir = root.join(format!("skills/{name}"));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} does things\n---\n\n# {name}\n"),
        )
        .unwrap();
        std::fs::write(dir.join("scripts/run.sh"), "echo hi\n").unwrap();
    }

    #[test]
    fn adopt_folder_copies_the_full_skill_directory() {
        let _sandbox = Sandbox::new();
        let source = tempfile::tempdir().unwrap();
        valid_skill(source.path(), "demo");

        let result = adopt_folder(source.path().to_str().unwrap(), None).unwrap();
        assert_eq!(result.adopted.len(), 1);
        assert_eq!(result.adopted[0].name, "demo");

        // The whole directory (scripts included) is preserved in skills-local.
        let local = skillstar_core::infra::paths::local_skills_dir().join("demo");
        assert!(local.join("SKILL.md").exists());
        assert!(local.join("scripts/run.sh").exists());
        // And exposed through the hub link.
        assert!(is_local_skill("demo"));
        // The source folder is untouched (adoption copies, it does not move).
        assert!(source.path().join("skills/demo/SKILL.md").exists());
    }

    #[test]
    fn adopt_folder_skips_skills_with_invalid_frontmatter() {
        let _sandbox = Sandbox::new();
        let source = tempfile::tempdir().unwrap();
        valid_skill(source.path(), "good");
        let bare_dir = source.path().join("skills/bare");
        std::fs::create_dir_all(&bare_dir).unwrap();
        std::fs::write(bare_dir.join("SKILL.md"), "# No frontmatter\n").unwrap();

        let result = adopt_folder(source.path().to_str().unwrap(), None).unwrap();
        assert_eq!(result.adopted.len(), 1);
        assert_eq!(result.adopted[0].name, "good");
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].name, "bare");
        assert!(
            result.skipped[0].reason.contains("description"),
            "{}",
            result.skipped[0].reason
        );
        assert!(!is_local_skill("bare"));
    }
}

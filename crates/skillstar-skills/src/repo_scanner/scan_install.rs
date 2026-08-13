use crate::git::ops as git_ops;
use crate::{content, local_skill, lockfile, source_resolver};
use anyhow::{Context, Result, anyhow};
use skillstar_core::infra::{fs_ops, paths};
use std::path::Path;
use tracing::warn;

use super::SkillInstallTarget;

struct PreparedRepoInstall {
    name: String,
    source: std::path::PathBuf,
    destination: std::path::PathBuf,
    staging: std::path::PathBuf,
    previous_target: Option<std::path::PathBuf>,
    lock_entry: crate::lockfile::LockEntry,
}

pub fn install_from_repo_in_session(
    source: &str,
    repo_url: &str,
    targets: &[SkillInstallTarget],
    session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<String>> {
    ensure_generic_targets_mutable(targets)?;
    crate::skill_mutation::policy().ensure_repository_mutation_allowed(repo_url)?;
    let repo_dir = super::clone_or_fetch_repo_in_session(repo_url, source, session)?;
    install_from_repo_at_with_source_migrations(&repo_dir, repo_url, None, targets, &[])
}

pub fn install_from_repo_at(
    repo_dir: &Path,
    repo_url: &str,
    git_ref: Option<&str>,
    targets: &[SkillInstallTarget],
) -> Result<Vec<String>> {
    ensure_generic_targets_mutable(targets)?;
    crate::skill_mutation::policy().ensure_repository_mutation_allowed(repo_url)?;
    install_from_repo_at_with_source_migrations(repo_dir, repo_url, git_ref, targets, &[])
}

fn ensure_generic_targets_mutable(targets: &[SkillInstallTarget]) -> Result<()> {
    for target in targets {
        crate::skill_mutation::policy().ensure_skill_mutation_allowed(&target.id)?;
    }
    Ok(())
}

pub fn install_from_repo_at_with_source_migrations(
    repo_dir: &Path,
    repo_url: &str,
    git_ref: Option<&str>,
    targets: &[SkillInstallTarget],
    authorized_previous_sources: &[(String, String)],
) -> Result<Vec<String>> {
    let hub_skills_dir = paths::hub_skills_dir();
    std::fs::create_dir_all(&hub_skills_dir).context("Failed to create hub skills directory")?;

    if !repo_dir.exists() {
        return Err(anyhow!(
            "Repo cache not found. Please scan the repository first."
        ));
    }
    let repo_root = std::fs::canonicalize(repo_dir)
        .with_context(|| format!("Failed to resolve repo cache '{}'", repo_dir.display()))?;

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow!("Lockfile mutex poisoned"))?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path)?;

    let mut prepared = Vec::new();
    let mut frontmatter_errors: Vec<String> = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        content::validate_skill_name(&target.id)
            .with_context(|| format!("Invalid Skill id '{}'", target.id))?;
        let dest = hub_skills_dir.join(&target.id);
        let existing_entry = lf.skills.iter().find(|entry| entry.name == target.id);
        let source_path = if target.folder_path.is_empty() {
            repo_root.clone()
        } else {
            repo_root.join(&target.folder_path)
        };

        if !source_path.exists() {
            warn!(
                target: "repo_scanner",
                path = %source_path.display(),
                "skill folder not found"
            );
            continue;
        }
        let source_path = std::fs::canonicalize(&source_path).with_context(|| {
            format!("Failed to resolve Skill source '{}'", source_path.display())
        })?;
        if !source_path.starts_with(&repo_root) {
            anyhow::bail!(
                "Skill source escapes repo cache '{}': '{}'",
                repo_root.display(),
                source_path.display()
            );
        }

        // Frontmatter quality gate: a skill without a usable `description` is
        // not installable per the open Agent Skills spec. Collect every
        // failure so the caller fixes all of them in one pass.
        if let Err(reason) = crate::validation::ensure_installable(&source_path) {
            frontmatter_errors.push(format!("'{}': {reason}", target.id));
            continue;
        }
        let content_hash = content::snapshot_path(&target.id, &source_path)
            .with_context(|| format!("Failed to capture content baseline for '{}'", target.id))?
            .content_hash;

        if dest.symlink_metadata().is_ok()
            && (!fs_ops::is_link(&dest)
                || !can_replace_existing_skill(
                    &target.id,
                    repo_url,
                    existing_entry,
                    authorized_previous_sources,
                ))
        {
            warn!(
                target: "repo_scanner",
                skill = %target.id,
                "refusing to replace existing skill from a different source"
            );
            continue;
        }

        let relative_source = source_path
            .strip_prefix(&repo_root)
            .expect("contained source has a relative path")
            .to_string_lossy()
            .replace('\\', "/");
        let source_folder = (!relative_source.is_empty()).then_some(relative_source);
        let tree_hash = match source_folder.as_deref() {
            Some(folder) => git_ops::compute_subtree_hash(&repo_root, folder).unwrap_or_default(),
            None => git_ops::compute_tree_hash(&repo_root).unwrap_or_default(),
        };
        let previous_target = if fs_ops::is_link(&dest) {
            Some(fs_ops::read_link_resolved(&dest).with_context(|| {
                format!("Failed to record existing Skill link '{}'", dest.display())
            })?)
        } else {
            None
        };

        prepared.push(PreparedRepoInstall {
            name: target.id.clone(),
            source: source_path,
            destination: dest.clone(),
            staging: hub_skills_dir.join(format!(".skillstar-install-{index}")),
            previous_target,
            lock_entry: crate::lockfile::LockEntry {
                name: target.id.clone(),
                git_url: repo_url.to_string(),
                git_ref: git_ref.map(str::to_string),
                tree_hash,
                content_hash: Some(content_hash),
                content_hash_version: Some(content::SNAPSHOT_HASH_VERSION),
                installed_at: chrono::Utc::now().to_rfc3339(),
                source_folder,
            },
        });
    }

    // Fail closed before any disk mutation: every requested skill that
    // produced a frontmatter error is reported, nothing is installed.
    if !frontmatter_errors.is_empty() {
        return Err(anyhow!(
            "Refusing to install invalid skill(s):\n{}",
            frontmatter_errors.join("\n")
        ));
    }

    for item in &prepared {
        if item.staging.symlink_metadata().is_ok()
            && let Err(error) = fs_ops::remove_link_or_copy(&item.staging)
        {
            cleanup_staging(&prepared);
            return Err(error).with_context(|| {
                format!(
                    "Failed to clear stale install staging '{}'",
                    item.staging.display()
                )
            });
        }
        if let Err(error) = fs_ops::create_symlink(&item.source, &item.staging) {
            cleanup_staging(&prepared);
            return Err(error).with_context(|| {
                format!(
                    "Failed to stage symlink {:?} → {:?}",
                    item.source, item.destination
                )
            });
        }
    }

    let mut applied = 0usize;
    for item in &prepared {
        if fs_ops::is_link(&item.destination)
            && let Err(error) = fs_ops::remove_symlink(&item.destination)
        {
            rollback_repo_installs(&prepared, applied);
            return Err(error).with_context(|| format!("Failed to replace Skill '{}'", item.name));
        }
        if let Err(error) = std::fs::rename(&item.staging, &item.destination) {
            rollback_repo_installs(&prepared, applied + 1);
            return Err(error).with_context(|| format!("Failed to install Skill '{}'", item.name));
        }
        applied += 1;
    }

    for item in &prepared {
        lf.upsert(item.lock_entry.clone());
    }
    if let Err(error) = lf
        .save(&lock_path)
        .context("Failed to save lockfile after batch install")
    {
        rollback_repo_installs(&prepared, applied);
        return Err(error);
    }

    Ok(prepared.into_iter().map(|item| item.name).collect())
}

fn cleanup_staging(prepared: &[PreparedRepoInstall]) {
    for item in prepared {
        if item.staging.symlink_metadata().is_ok() {
            let _ = fs_ops::remove_link_or_copy(&item.staging);
        }
    }
}

fn rollback_repo_installs(prepared: &[PreparedRepoInstall], applied: usize) {
    for item in prepared.iter().take(applied).rev() {
        if item.destination.symlink_metadata().is_ok() {
            let _ = fs_ops::remove_link_or_copy(&item.destination);
        }
        if let Some(previous) = &item.previous_target {
            let _ = fs_ops::create_symlink(previous, &item.destination);
        }
    }
    cleanup_staging(prepared);
}

fn can_replace_existing_skill(
    skill_name: &str,
    repo_url: &str,
    existing_entry: Option<&crate::lockfile::LockEntry>,
    authorized_previous_sources: &[(String, String)],
) -> bool {
    if local_skill::is_local_skill(skill_name) {
        return false;
    }
    existing_entry.is_some_and(|entry| {
        source_resolver::same_remote_url(&entry.git_url, repo_url)
            || authorized_previous_sources.iter().any(|(id, source)| {
                id.eq_ignore_ascii_case(skill_name)
                    && source_resolver::same_remote_url(&entry.git_url, source)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    struct Sandbox {
        previous: Vec<(&'static str, Option<OsString>)>,
        _temp: tempfile::TempDir,
        // Serializes env mutation against other env-mutating tests.
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

    fn target(id: &str) -> SkillInstallTarget {
        SkillInstallTarget {
            id: id.to_string(),
            folder_path: format!("skills/{id}"),
        }
    }

    #[test]
    fn install_fails_closed_when_any_skill_has_blocking_frontmatter_issues() {
        let _sandbox = Sandbox::new();
        let repo = tempfile::tempdir().unwrap();

        let valid = repo.path().join("skills/valid");
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::write(
            valid.join("SKILL.md"),
            "---\nname: valid\ndescription: A valid skill\n---\n\n# Valid\n",
        )
        .unwrap();

        let bare = repo.path().join("skills/bare");
        std::fs::create_dir_all(&bare).unwrap();
        std::fs::write(bare.join("SKILL.md"), "# No frontmatter\n").unwrap();

        let error = install_from_repo_at(
            repo.path(),
            "https://github.com/example/repo",
            None,
            &[target("valid"), target("bare")],
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("bare"), "{error}");
        assert!(error.contains("description"), "{error}");
        // Fail-closed: the valid skill must not be installed either.
        let hub = skillstar_core::infra::paths::hub_skills_dir();
        assert!(!hub.join("valid").symlink_metadata().is_ok(), "{error}");
        assert!(!hub.join("bare").symlink_metadata().is_ok());
    }

    #[test]
    fn install_succeeds_when_all_skills_are_valid() {
        let _sandbox = Sandbox::new();
        let repo = tempfile::tempdir().unwrap();

        let valid = repo.path().join("skills/valid");
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::write(
            valid.join("SKILL.md"),
            "---\nname: valid\ndescription: A valid skill\n---\n\n# Valid\n",
        )
        .unwrap();

        let installed = install_from_repo_at(
            repo.path(),
            "https://github.com/example/repo",
            None,
            &[target("valid")],
        )
        .expect("valid skill installs");

        assert_eq!(installed, vec!["valid".to_string()]);
        let hub = skillstar_core::infra::paths::hub_skills_dir();
        assert!(hub.join("valid").symlink_metadata().is_ok());
    }
}

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::git::ops as git_ops;
use crate::{installed_skill, lockfile, project_manifest, repo_scanner, update_checker};
use skillstar_projects::projects::sync;

/// Result of a hub skill update, including any project-level cascade work.
#[derive(Debug, Clone)]
pub struct SkillUpdateOutcome {
    pub tree_hash: String,
    pub git_url: String,
    pub sibling_names: Vec<String>,
    pub agent_links: Vec<String>,
    /// Per-agent re-link failures ("Agent: error"). The update itself
    /// succeeded; these tell the UI which agent deployments need attention.
    pub agent_link_failures: Vec<String>,
    pub cascade: project_manifest::CascadeUpdateSummary,
}

fn push_unique(values: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !values.contains(&value) {
        values.push(value);
    }
}

fn resolve_repo_root_from_symlink(skill_path: &Path) -> Option<PathBuf> {
    update_checker::resolve_skill_repo_root(skill_path)
}

fn compute_hash_for_skill_entry(skill_path: &Path, source_folder: Option<&str>) -> Option<String> {
    if let Some(folder) = source_folder.filter(|folder| !folder.is_empty()) {
        let repo_root = resolve_repo_root_from_symlink(skill_path)?;
        return update_checker::compute_subtree_hash(&repo_root, folder).ok();
    }

    if let Some(repo_root) = resolve_repo_root_from_symlink(skill_path) {
        return git_ops::compute_tree_hash(&repo_root)
            .ok()
            .or_else(|| git_ops::compute_tree_hash(skill_path).ok());
    }

    git_ops::compute_tree_hash(skill_path).ok()
}

pub fn update_skill(name: &str) -> Result<SkillUpdateOutcome> {
    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();
    let path = skills_dir.join(name);

    if !path.exists() && !skillstar_core::infra::fs_ops::is_link(&path) {
        anyhow::bail!("Skill '{}' not found in hub", name);
    }

    let is_repo_skill = update_checker::is_repo_cached_skill(&path);

    let lock_entry = {
        let lock_path = lockfile::lockfile_path();
        lockfile::Lockfile::load(&lock_path)
            .ok()
            .and_then(|lf| lf.skills.into_iter().find(|entry| entry.name == name))
    };

    let tree_hash = if is_repo_skill {
        let source_folder = lock_entry
            .as_ref()
            .and_then(|entry| entry.source_folder.as_deref());
        repo_scanner::pull_repo_skill_update(&path, source_folder)
            .context("failed to pull repo-cached skill update")?
    } else {
        git_ops::pull_repo(&path).context("failed to pull hub skill update")?;
        git_ops::compute_tree_hash(&path).context("failed to compute updated tree hash")?
    };

    let mut sibling_names: Vec<String> = Vec::new();

    {
        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| anyhow::anyhow!("Lockfile mutex poisoned"))?;
        let lock_path = lockfile::lockfile_path();
        let mut lockfile = lockfile::Lockfile::load(&lock_path)
            .with_context(|| format!("Failed to load lockfile '{}'", lock_path.display()))?;

        if is_repo_skill {
            if let Some(ref entry) = lock_entry {
                let git_url = &entry.git_url;
                for sibling in lockfile
                    .skills
                    .iter_mut()
                    .filter(|entry| entry.git_url == *git_url)
                {
                    if sibling.name == name {
                        sibling.tree_hash = tree_hash.clone();
                        continue;
                    }

                    let sibling_path = skills_dir.join(&sibling.name);
                    if !sibling_path.exists() {
                        continue;
                    }

                    if let Some(hash) = compute_hash_for_skill_entry(
                        &sibling_path,
                        sibling.source_folder.as_deref(),
                    ) {
                        sibling.tree_hash = hash;
                    }
                    push_unique(&mut sibling_names, sibling.name.clone());
                }
            }
        } else if let Some(entry) = lockfile.skills.iter_mut().find(|entry| entry.name == name) {
            entry.tree_hash = tree_hash.clone();
        }

        lockfile
            .save(&lock_path)
            .with_context(|| format!("Failed to save lockfile '{}'", lock_path.display()))?;
    }

    installed_skill::invalidate_cache();

    let mut affected_skills = vec![name.to_string()];
    for sibling_name in &sibling_names {
        push_unique(&mut affected_skills, sibling_name.clone());
    }

    for skill_name in &affected_skills {
        installed_skill::clear_update_state(skill_name);
    }

    let mut agent_links = Vec::new();
    let mut agent_link_failures = Vec::new();
    for skill_name in &affected_skills {
        match sync::resync_existing_links(skill_name) {
            Ok(report) => {
                for failure in report.failures {
                    agent_link_failures.push(if skill_name == name {
                        failure
                    } else {
                        format!("{skill_name} → {failure}")
                    });
                }
                if skill_name == name {
                    agent_links = report.linked_to;
                }
            }
            Err(err) => {
                // The update itself succeeded — report the re-link problem
                // instead of failing the whole update or silently dropping it.
                agent_link_failures.push(format!("{skill_name}: {err:#}"));
            }
        }
    }

    let cascade = project_manifest::cascade_skill_update_to_projects(&affected_skills);
    let git_url = lock_entry.map(|entry| entry.git_url).unwrap_or_default();

    sibling_names.sort();
    sibling_names.dedup();

    Ok(SkillUpdateOutcome {
        tree_hash,
        git_url,
        sibling_names,
        agent_links,
        agent_link_failures,
        cascade,
    })
}

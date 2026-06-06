//! Project-level deployment: full sync, additive add, and targeted cleanup.

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::helpers::{clear_project_symlinks, prune_empty_dirs_upward};
use super::index::{list_projects, register_project};
use super::store::{load_skills_list, save_skills_list};
use super::types::{
    deploy_skill_auto, ensure_project_root_exists, prune_deploy_modes_for_agents, ProjectDeployMode,
    SkillsList,
};
use crate::projects::agents as agent_profile;
use skillstar_core::infra::{fs_ops, paths as fs_paths};

fn normalize_project_agents(agents: HashMap<String, Vec<String>>) -> HashMap<String, Vec<String>> {
    agents
        .into_iter()
        .filter_map(|(agent_id, skill_names)| {
            let mut seen = HashSet::new();
            let normalized = skill_names
                .into_iter()
                .filter_map(|name| {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        return None;
                    }

                    let normalized = trimmed.to_string();
                    if seen.insert(normalized.clone()) {
                        Some(normalized)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if normalized.is_empty() {
                None
            } else {
                Some((agent_id, normalized))
            }
        })
        .collect()
}

/// Remove a skill from every registered project's persisted metadata and
/// project-level symlinks.
///
/// This is a targeted cleanup pass used when a skill is uninstalled from the
/// hub. It intentionally removes only the named symlink from project folders
/// instead of running a full project sync, so unmanaged on-disk directories are
/// left alone.
pub fn remove_skill_from_all_projects(skill_name: &str) -> Result<Vec<String>> {
    let profiles = agent_profile::list_profiles();
    let mut touched_projects = Vec::new();

    for entry in list_projects() {
        let mut touched = false;

        if let Some(mut skills_list) = load_skills_list(&entry.name) {
            let mut list_changed = false;
            for skill_names in skills_list.agents.values_mut() {
                let before = skill_names.len();
                skill_names.retain(|name| name != skill_name);
                if skill_names.len() != before {
                    list_changed = true;
                }
            }
            if list_changed {
                skills_list.agents.retain(|_, skills| !skills.is_empty());
                skills_list.updated_at = chrono::Utc::now().to_rfc3339();
                save_skills_list(&entry.name, &skills_list)?;
                touched = true;
            }
        }

        let project_root = Path::new(&entry.path);
        for profile in &profiles {
            if !profile.has_project_skills() {
                continue;
            }

            let skill_path = project_root
                .join(&profile.project_skills_rel)
                .join(skill_name);
            if !fs_ops::is_link(&skill_path) && !skill_path.is_dir() {
                continue;
            }

            // Remove symlink, junction, or copy
            fs_ops::remove_link_or_copy(&skill_path).with_context(|| {
                format!(
                    "failed to remove project skill '{}' from {}",
                    skill_name, entry.path
                )
            })?;

            if let Some(parent) = skill_path.parent() {
                prune_empty_dirs_upward(parent, project_root)?;
            }

            touched = true;
        }

        if touched {
            touched_projects.push(entry.name);
        }
    }

    Ok(touched_projects)
}

/// Perform a full sync: clear existing symlinks in managed agent directories,
/// then recreate them from the provided skills list.
///
/// Only touches agent directories that appear in `skills_list.agents` or in
/// `cleanup_agents` (agents removed from a previous config).  Agent directories
/// not mentioned in either are left untouched, preventing cross-agent data loss
/// from CLI-deployed or externally-managed symlinks. Empty/invalid skill lists
/// never create placeholder project folders.
///
/// Returns the total number of symlinks created.
pub fn full_sync(
    project_path: &str,
    skills_list: &SkillsList,
    cleanup_agents: Option<&[String]>,
) -> Result<u32> {
    let hub_dir = fs_paths::hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = ensure_project_root_exists(project_path)?;
    let mut total = 0u32;
    let mut failures: Vec<String> = Vec::new();

    // Only clear directories for agents we are actively managing:
    // - agents in the new skills_list (will be rebuilt)
    // - agents in cleanup_agents (were in old config, now removed)
    let agents_in_list: HashSet<&str> = skills_list.agents.keys().map(|s| s.as_str()).collect();

    for profile in &profiles {
        if !profile.has_project_skills() {
            continue;
        }
        let should_clear = agents_in_list.contains(profile.id.as_str())
            || cleanup_agents.is_some_and(|ids| ids.iter().any(|id| id == &profile.id));
        if !should_clear {
            continue;
        }
        clear_project_symlinks(project.as_path(), profile)?;
    }

    for (agent_id, skill_names) in &skills_list.agents {
        // Find the agent profile to get its project_skills_rel
        let Some(profile) = profiles.iter().find(|p| &p.id == agent_id) else {
            continue;
        };
        // Skip agents that have no project-level skills support
        if !profile.has_project_skills() {
            continue;
        }

        let target_dir = project.join(&profile.project_skills_rel);
        let mut prepared_target_dir = false;
        let mut created_for_agent = 0u32;

        // Create new symlinks (auto-fallback to copy if symlink fails)
        for skill_name in skill_names {
            let source = hub_dir.join(skill_name);
            if !source.exists() {
                continue;
            }
            if !prepared_target_dir {
                std::fs::create_dir_all(&target_dir).with_context(|| {
                    format!("failed to create skill dir: {}", target_dir.display())
                })?;
                prepared_target_dir = true;
            }
            let target = target_dir.join(skill_name);
            match deploy_skill_auto(&source, &target) {
                Ok(()) => {
                    total += 1;
                    created_for_agent += 1;
                }
                Err(err) => failures.push(format!(
                    "Failed to link '{skill_name}' for agent '{agent_id}' at {target}: {err}",
                    target = target.display()
                )),
            }
        }

        if created_for_agent == 0 && target_dir.exists() {
            prune_empty_dirs_upward(&target_dir, project.as_path())?;
        }
    }

    if !failures.is_empty() {
        let failed = failures.len();
        let preview = failures.into_iter().take(6).collect::<Vec<_>>().join("\n");
        anyhow::bail!(
            "Project sync incomplete: created {total} link(s), {failed} failure(s).\n{preview}",
            failed = failed
        );
    }

    Ok(total)
}

/// Register a project, save its skills list, and perform a full sync.
///
/// This is the main entry point for both initial deployment and subsequent
/// modifications. Returns `(project_name, symlink_count)`.
///
/// Compares the old skills-list against the new one to determine which agent
/// directories need cleanup (agents that were configured before but removed
/// now). Only those directories plus the new active ones are cleared and
/// rebuilt; unrelated agent directories are left untouched.
pub fn save_and_sync(
    project_path: &str,
    agents: HashMap<String, Vec<String>>,
    deploy_modes: HashMap<String, ProjectDeployMode>,
) -> Result<(String, u32)> {
    let entry = register_project(project_path)?;
    let agents = normalize_project_agents(agents);

    // Snapshot the old config so we can compute which agents were removed.
    let old_list = load_skills_list(&entry.name);

    let _profiles = agent_profile::list_profiles();
    // deploy_modes accepted for backward-compat but ignored;
    // deploy always tries symlink first, auto-falling back to copy.
    let _ = &deploy_modes;
    let skills_list = SkillsList {
        agents,
        deploy_modes: HashMap::new(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    save_skills_list(&entry.name, &skills_list)?;

    // Agents that existed in old config but are absent from new config need
    // their project directories cleaned up.
    let cleanup: Vec<String> = old_list
        .map(|old| {
            old.agents
                .keys()
                .filter(|id| !skills_list.agents.contains_key(*id))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    let count = full_sync(project_path, &skills_list, Some(&cleanup))?;

    Ok((entry.name, count))
}

/// Register a project and persist its skills-list.json without mutating any
/// project filesystem symlinks.
///
/// This is used for non-destructive metadata updates (for example, resolving
/// shared-path ownership) where we must not clear or recreate links.
pub fn save_skills_list_only(
    project_path: &str,
    agents: HashMap<String, Vec<String>>,
) -> Result<SkillsList> {
    let entry = register_project(project_path)?;

    let profiles = agent_profile::list_profiles();
    let mut skills_list = load_skills_list(&entry.name).unwrap_or_default();
    skills_list.agents = normalize_project_agents(agents);
    skills_list.updated_at = chrono::Utc::now().to_rfc3339();
    prune_deploy_modes_for_agents(&mut skills_list.deploy_modes, &skills_list.agents, &profiles);

    save_skills_list(&entry.name, &skills_list)?;

    Ok(skills_list)
}

/// Incrementally add skills to a project for the given agents.
///
/// Unlike `save_and_sync` (which replaces the entire skills-list and rebuilds
/// all symlinks), this function **merges** the requested skills into the
/// existing skills-list and only creates symlinks for the new entries —
/// leaving other agents' project directories untouched.
///
/// This is the canonical path for CLI `skillstar install` and quick-deploy
/// operations that should be additive, not destructive.
///
/// If `agent_ids` is empty, falls back to the first profile whose
/// `project_skills_rel` is `".agents/skills"` (i.e. Antigravity), or the
/// first available profile with project-level support.
pub fn add_skills_to_project(
    project_path: &str,
    skill_names: &[String],
    agent_ids: &[String],
) -> Result<u32> {
    let entry = register_project(project_path)?;
    let hub_dir = fs_paths::hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = ensure_project_root_exists(project_path)?;

    // Resolve which agents to target
    let mut target_agent_ids: Vec<String> = agent_ids
        .iter()
        .filter(|id| {
            profiles
                .iter()
                .any(|p| p.id == **id && p.has_project_skills())
        })
        .cloned()
        .collect();

    if target_agent_ids.is_empty() {
        if !agent_ids.is_empty() {
            return Err(anyhow::anyhow!(
                "No valid project-level agents selected: {}",
                agent_ids.join(", ")
            ));
        }
        // Fallback: prefer the profile using .agents/skills, then first available
        if let Some(fallback) = profiles
            .iter()
            .find(|p| p.project_skills_rel == ".agents/skills" && p.has_project_skills())
            .or_else(|| profiles.iter().find(|p| p.has_project_skills()))
        {
            target_agent_ids.push(fallback.id.clone());
        }
    }

    // Merge into existing skills-list.json
    let mut skills_list = load_skills_list(&entry.name).unwrap_or_default();

    for agent_id in &target_agent_ids {
        let agent_skills = skills_list.agents.entry(agent_id.clone()).or_default();
        for name in skill_names {
            if !agent_skills.contains(name) {
                agent_skills.push(name.clone());
            }
        }
    }
    skills_list.updated_at = chrono::Utc::now().to_rfc3339();
    save_skills_list(&entry.name, &skills_list)?;

    // Create only the new symlinks (incremental — no clearing)
    let mut total = 0u32;
    let mut failures: Vec<String> = Vec::new();

    for agent_id in &target_agent_ids {
        let Some(profile) = profiles.iter().find(|p| &p.id == agent_id) else {
            continue;
        };
        if !profile.has_project_skills() {
            continue;
        }

        let target_dir = project.join(&profile.project_skills_rel);
        std::fs::create_dir_all(&target_dir)
            .with_context(|| format!("failed to create skill dir: {}", target_dir.display()))?;

        for name in skill_names {
            let source = hub_dir.join(name);
            if !source.exists() {
                continue;
            }
            let target = target_dir.join(name);

            // If already a symlink, remove and recreate (refresh)
            if fs_ops::is_link(&target) {
                let _ = fs_ops::remove_symlink(&target);
            } else if target.exists() {
                // Real directory exists — skip to avoid data loss
                continue;
            }

            match deploy_skill_auto(&source, &target) {
                Ok(()) => total += 1,
                Err(err) => failures.push(format!(
                    "Failed to link '{}' for agent '{}' at {}: {}",
                    name,
                    agent_id,
                    target.display(),
                    err
                )),
            }
        }
    }

    if !failures.is_empty() {
        let failed = failures.len();
        let preview = failures.into_iter().take(6).collect::<Vec<_>>().join("\n");
        anyhow::bail!(
            "Project deploy incomplete: created {total} link(s), {failed} failure(s).\n{preview}"
        );
    }

    Ok(total)
}

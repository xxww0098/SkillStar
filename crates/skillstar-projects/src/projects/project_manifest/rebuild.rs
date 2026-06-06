//! Reconstruct a project's skills-list.json from on-disk directory state.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use super::index::register_project;
use super::store::{load_skills_list, save_skills_list};
use super::types::{prune_deploy_modes_for_agents, SkillsList};
use crate::projects::agents as agent_profile;
use skillstar_core::infra::fs_ops;

/// Rebuild a project's skills-list.json from on-disk project skill directories.
///
/// For shared paths (multiple agents with the same `project_skills_rel`), this
/// function picks a single owner agent:
/// 1) prefer an agent that already exists in current skills-list.json
/// 2) otherwise use the first agent in builtin profile order.
///
/// It persists and returns the rebuilt list, without performing full sync.
pub fn rebuild_skills_list_from_disk(project_path: &str) -> Result<SkillsList> {
    let entry = register_project(project_path)?;
    let project = Path::new(project_path);
    let profiles = agent_profile::list_profiles();
    let existing_agents = load_skills_list(&entry.name)
        .map(|list| list.agents)
        .unwrap_or_default();

    // Group profiles by project_skills_rel while preserving profile order.
    let mut path_order = Vec::new();
    let mut groups: HashMap<String, Vec<agent_profile::AgentProfile>> = HashMap::new();
    for profile in profiles {
        if !profile.has_project_skills() {
            continue;
        }
        if !groups.contains_key(&profile.project_skills_rel) {
            path_order.push(profile.project_skills_rel.clone());
        }
        groups
            .entry(profile.project_skills_rel.clone())
            .or_default()
            .push(profile);
    }

    let mut rebuilt_agents: HashMap<String, Vec<String>> = HashMap::new();

    for rel_path in path_order {
        let Some(group_profiles) = groups.get(&rel_path) else {
            continue;
        };
        let skills_dir = project.join(&rel_path);
        let entries = match std::fs::read_dir(&skills_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        let mut names = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() && !fs_ops::is_link(&path) {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            if name.is_empty() || name.starts_with('.') {
                continue;
            }

            // Keep explicit skill folders and symlinked skills.
            // Skip arbitrary non-skill directories without SKILL.md.
            let has_skill_md = path.join("SKILL.md").exists();
            if !fs_ops::is_link(&path) && !has_skill_md {
                continue;
            }

            if !names.contains(&name) {
                names.push(name);
            }
        }

        if names.is_empty() {
            continue;
        }

        // Prefer previously configured owner for shared paths.
        let owner = group_profiles
            .iter()
            .find(|profile| existing_agents.contains_key(&profile.id))
            .or_else(|| group_profiles.first())
            .map(|profile| profile.id.clone());

        let Some(owner_id) = owner else {
            continue;
        };

        let bucket = rebuilt_agents.entry(owner_id).or_default();
        bucket.extend(names);
    }

    for names in rebuilt_agents.values_mut() {
        names.sort();
        names.dedup();
    }

    let prior_deploy = load_skills_list(&entry.name)
        .map(|list| list.deploy_modes)
        .unwrap_or_default();

    let mut rebuilt = SkillsList {
        agents: rebuilt_agents,
        deploy_modes: prior_deploy,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    prune_deploy_modes_for_agents(
        &mut rebuilt.deploy_modes,
        &rebuilt.agents,
        &agent_profile::list_profiles(),
    );

    save_skills_list(&entry.name, &rebuilt)?;

    Ok(rebuilt)
}

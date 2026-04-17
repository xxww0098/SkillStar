use std::collections::HashMap;
use tracing::{error, warn};

use crate::core::{
    infra::error::AppError,
    marketplace, project_manifest,
    skill_group::{self, SkillGroup},
    skill_install,
};

#[tauri::command]
pub async fn list_skill_groups() -> Result<Vec<SkillGroup>, AppError> {
    Ok(skill_group::list_groups())
}

#[tauri::command]
pub async fn create_skill_group(
    name: String,
    description: String,
    icon: String,
    skills: Vec<String>,
    skill_sources: Option<HashMap<String, String>>,
) -> Result<SkillGroup, AppError> {
    skill_group::create_group(
        name,
        description,
        icon,
        skills,
        skill_sources.unwrap_or_default(),
    )
    .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn update_skill_group(
    id: String,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    skills: Option<Vec<String>>,
    skill_sources: Option<HashMap<String, String>>,
) -> Result<SkillGroup, AppError> {
    skill_group::update_group(id, name, description, icon, skills, skill_sources)
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn delete_skill_group(id: String) -> Result<(), AppError> {
    skill_group::delete_group(&id).map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn duplicate_skill_group(id: String) -> Result<SkillGroup, AppError> {
    skill_group::duplicate_group(&id).map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn deploy_skill_group(
    group_id: String,
    project_path: String,
    agent_types: Vec<String>,
) -> Result<u32, AppError> {
    let groups = skill_group::list_groups();
    let group = groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| AppError::Other(format!("Group '{}' not found", group_id)))?;

    let skills_dir = crate::core::infra::paths::hub_skills_dir();
    let mut sources = group.skill_sources.clone();

    let names_needing_source: Vec<String> = group
        .skills
        .iter()
        .filter(|name| !skills_dir.join(name).exists() && !sources.contains_key(*name))
        .cloned()
        .collect();

    if !names_needing_source.is_empty() {
        warn!(
            target: "deploy_skill_group",
            "resolving {} missing skill source(s) via marketplace snapshot",
            names_needing_source.len()
        );
        match marketplace::resolve_skill_sources_local_first(&names_needing_source, &sources).await
        {
            Ok(resolved) => {
                for (name, url) in resolved {
                    sources.insert(name, url);
                }
            }
            Err(err) => {
                error!(
                    target: "deploy_skill_group",
                    "failed to resolve missing skill sources: {err}"
                );
            }
        }
    }

    let mut batch_by_url: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for skill_name in &group.skills {
        if !skills_dir.join(skill_name).exists() {
            if let Some(git_url) = sources.get(skill_name) {
                batch_by_url
                    .entry(git_url.clone())
                    .or_default()
                    .push(skill_name.clone());
            }
        }
    }

    let mut install_tasks = tokio::task::JoinSet::new();
    for (url, names) in batch_by_url {
        install_tasks.spawn_blocking(move || {
            let _ = skill_install::install_skills_batch(&url, &names);
        });
    }
    while let Some(result) = install_tasks.join_next().await {
        if let Err(e) = result {
            error!(target: "deploy_skill_group", "install task join error: {e}");
        }
    }

    let agents: HashMap<String, Vec<String>> = agent_types
        .into_iter()
        .map(|id| (id, group.skills.clone()))
        .collect();

    let entry = project_manifest::register_project(&project_path)
        .map_err(|e| AppError::Project(e.to_string()))?;
    let deploy_modes = project_manifest::load_skills_list(&entry.name)
        .map(|list| list.deploy_modes)
        .unwrap_or_default();

    let (_name, count) = project_manifest::save_and_sync(&project_path, agents, deploy_modes)
        .map_err(|e| AppError::Project(e.to_string()))?;

    Ok(count)
}

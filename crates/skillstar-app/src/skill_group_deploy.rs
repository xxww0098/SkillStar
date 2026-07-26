//! Cross-domain skill-group deployment use case.

use std::collections::HashMap;

use skillstar_core::infra::error::AppError;
use skillstar_skills::{projects, skill_group, skill_install};
use tracing::{error, warn};

pub async fn deploy(
    group_id: String,
    project_path: String,
    agent_types: Vec<String>,
) -> Result<u32, AppError> {
    let groups = skill_group::list_groups();
    let group = groups
        .iter()
        .find(|group| group.id == group_id)
        .ok_or_else(|| AppError::Other(format!("Group '{group_id}' not found")))?;

    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();
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
        match skillstar_marketplace::snapshot::resolve_skill_sources_local_first(
            &names_needing_source,
            &sources,
        )
        .await
        {
            Ok(resolved) => sources.extend(resolved),
            Err(error) => {
                error!(target: "deploy_skill_group", "failed to resolve missing skill sources: {error}");
            }
        }
    }

    let mut batch_by_url: HashMap<String, Vec<String>> = HashMap::new();
    for skill_name in &group.skills {
        if !skills_dir.join(skill_name).exists()
            && let Some(git_url) = sources.get(skill_name)
        {
            batch_by_url
                .entry(git_url.clone())
                .or_default()
                .push(skill_name.clone());
        }
    }

    let mut install_tasks = tokio::task::JoinSet::new();
    for (url, names) in batch_by_url {
        install_tasks.spawn_blocking(move || {
            let _ = skill_install::install_skills_batch(&url, &names);
        });
    }
    while let Some(result) = install_tasks.join_next().await {
        if let Err(error) = result {
            error!(target: "deploy_skill_group", "install task join error: {error}");
        }
    }

    let agents = agent_types
        .into_iter()
        .map(|id| (id, group.skills.clone()))
        .collect();
    let entry = projects::register_project(&project_path)
        .map_err(|error| AppError::Project(error.to_string()))?;
    let deploy_modes = projects::load_skills_list(&entry.name)
        .map(|list| list.deploy_modes)
        .unwrap_or_default();
    let (_, count) = projects::save_and_sync(&project_path, agents, deploy_modes)
        .map_err(|error| AppError::Project(error.to_string()))?;
    Ok(count)
}

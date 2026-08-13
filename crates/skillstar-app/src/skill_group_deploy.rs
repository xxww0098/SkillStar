//! Cross-domain skill-group deployment use case.

use std::collections::HashMap;

use skillstar_core::infra::error::AppError;
use skillstar_skills::{projects, skill_group, skill_install};
use tracing::{error, warn};

#[derive(Debug, PartialEq, Eq)]
struct MissingSkillInstallFailure {
    names: Vec<String>,
    reason: String,
}

fn summarize_install_failures(failures: &[MissingSkillInstallFailure]) -> Option<String> {
    if failures.is_empty() {
        return None;
    }
    Some(format!(
        "Unable to install missing Skills: {}",
        failures
            .iter()
            .map(|failure| format!("{}: {}", failure.names.join(", "), failure.reason))
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

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

    let unresolved_names = group
        .skills
        .iter()
        .filter(|name| !skills_dir.join(name).exists() && !sources.contains_key(*name))
        .cloned()
        .collect::<Vec<_>>();
    let mut install_failures = Vec::new();
    if !unresolved_names.is_empty() {
        install_failures.push(MissingSkillInstallFailure {
            names: unresolved_names,
            reason: "no install source could be resolved".to_string(),
        });
    }

    // Spawn every URL batch before awaiting any handle, preserving cross-URL concurrency
    // while keeping each batch's Skill names available even if its task fails to join.
    let install_tasks = batch_by_url
        .into_iter()
        .map(|(url, names)| {
            let task_names = names.clone();
            let handle = tokio::task::spawn_blocking(move || {
                skill_install::install_skills_batch(&url, &names)
            });
            (task_names, handle)
        })
        .collect::<Vec<_>>();
    for (names, task) in install_tasks {
        match task.await {
            Ok(Ok(_)) => {}
            Ok(Err(reason)) => {
                install_failures.push(MissingSkillInstallFailure { names, reason });
            }
            Err(error) => {
                install_failures.push(MissingSkillInstallFailure {
                    names,
                    reason: format!("install task failed: {error}"),
                });
            }
        }
    }
    if let Some(message) = summarize_install_failures(&install_failures) {
        return Err(AppError::Other(message));
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

#[cfg(test)]
mod tests {
    use super::{MissingSkillInstallFailure, summarize_install_failures};

    #[test]
    fn install_failure_summary_keeps_skill_names_and_underlying_reasons() {
        let summary = summarize_install_failures(&[
            MissingSkillInstallFailure {
                names: vec!["old-matt-skill".to_string(), "renamed-skill".to_string()],
                reason: "not found; source may have deleted or renamed them".to_string(),
            },
            MissingSkillInstallFailure {
                names: vec!["network-skill".to_string()],
                reason: "network unavailable".to_string(),
            },
        ])
        .expect("failures should produce a summary");

        assert!(summary.contains("old-matt-skill, renamed-skill"));
        assert!(summary.contains("source may have deleted or renamed them"));
        assert!(summary.contains("network-skill: network unavailable"));
    }
}

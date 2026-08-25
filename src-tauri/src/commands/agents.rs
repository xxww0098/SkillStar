use skillstar_agents as agent_profile;
use skillstar_core::infra::error::AppError;
use skillstar_skills::deployment;
use skillstar_skills::installed_skill;
use std::time::Instant;

#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchSkillToggleFailure {
    pub skill_name: String,
    pub error: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchSkillToggleReport {
    pub succeeded: Vec<String>,
    pub failed: Vec<BatchSkillToggleFailure>,
}

fn run_batch_toggle<F>(skill_names: &[String], mut toggle: F) -> BatchSkillToggleReport
where
    F: FnMut(&str) -> anyhow::Result<()>,
{
    let mut report = BatchSkillToggleReport {
        succeeded: Vec::with_capacity(skill_names.len()),
        failed: Vec::new(),
    };
    for skill_name in skill_names {
        match toggle(skill_name) {
            Ok(()) => report.succeeded.push(skill_name.clone()),
            Err(error) => report.failed.push(BatchSkillToggleFailure {
                skill_name: skill_name.clone(),
                error: format!("{error:#}"),
            }),
        }
    }
    report
}

#[tauri::command]
pub async fn list_agent_profiles() -> Result<Vec<agent_profile::AgentProfile>, AppError> {
    Ok(agent_profile::list_profiles())
}

#[tauri::command]
pub async fn toggle_agent_profile(id: String) -> Result<bool, AppError> {
    tracing::info!(target: "cmd::agents", id, "toggle_agent_profile called");
    let result = agent_profile::toggle_profile(&id).map_err(|e| {
        tracing::error!(target: "cmd::agents", id, error = %e, "toggle_agent_profile failed");
        AppError::Other(e.to_string())
    });
    if let Ok(new_state) = &result {
        tracing::info!(target: "cmd::agents", id, enabled = *new_state, "toggle_agent_profile completed");
        deployment::invalidate_profile_cache();
    }
    result
}

#[tauri::command]
pub async fn unlink_all_skills_from_agent(agent_id: String) -> Result<u32, AppError> {
    tracing::info!(target: "cmd::agents", agent_id, "unlink_all_skills_from_agent called");
    let result = deployment::unlink_all_skills_from_agent(&agent_id).map_err(|e| {
        tracing::error!(target: "cmd::agents", agent_id, error = %e, "unlink_all_skills_from_agent failed");
        AppError::Other(e.to_string())
    });
    if let Ok(removed) = &result {
        tracing::info!(target: "cmd::agents", agent_id, removed, "unlink_all_skills_from_agent completed");
        installed_skill::invalidate_cache();
    }
    result
}

#[tauri::command]
pub async fn batch_link_skills_to_agent(
    skill_names: Vec<String>,
    agent_id: String,
) -> Result<u32, AppError> {
    let result = deployment::batch_link_skills_to_agent(&skill_names, &agent_id)
        .map_err(|e| AppError::Other(e.to_string()));
    if result.is_ok() {
        installed_skill::invalidate_cache();
    }
    result
}

#[tauri::command]
pub async fn batch_toggle_skills_for_agent(
    skill_names: Vec<String>,
    agent_id: String,
    enable: bool,
    operation_id: String,
) -> Result<BatchSkillToggleReport, AppError> {
    let started = Instant::now();
    let total = skill_names.len();
    tracing::info!(
        target: "cmd::agents",
        operation = "batch_toggle_skills_for_agent",
        phase = "started",
        operation_id,
        agent_id,
        enable,
        total,
        "batch Agent skill toggle started"
    );

    let report = run_batch_toggle(&skill_names, |skill_name| {
        deployment::toggle_skill_for_agent(skill_name, &agent_id, enable)
    });
    for failure in &report.failed {
        tracing::warn!(
            target: "cmd::agents",
            operation = "batch_toggle_skills_for_agent",
            phase = "item_failed",
            operation_id,
            agent_id,
            enable,
            skill_name = %failure.skill_name,
            error = %failure.error,
            "batch Agent skill toggle item failed"
        );
    }

    if !report.succeeded.is_empty() {
        installed_skill::invalidate_cache();
    }
    let succeeded = report.succeeded.len();
    let failed = report.failed.len();
    let elapsed_ms = started.elapsed().as_millis() as u64;
    if failed == 0 {
        tracing::info!(
            target: "cmd::agents",
            operation = "batch_toggle_skills_for_agent",
            phase = "completed",
            operation_id,
            agent_id,
            enable,
            total,
            succeeded,
            failed,
            elapsed_ms,
            "batch Agent skill toggle completed"
        );
    } else {
        tracing::warn!(
            target: "cmd::agents",
            operation = "batch_toggle_skills_for_agent",
            phase = "completed_with_failures",
            operation_id,
            agent_id,
            enable,
            total,
            succeeded,
            failed,
            elapsed_ms,
            "batch Agent skill toggle completed with failures"
        );
    }
    Ok(report)
}

#[tauri::command]
pub async fn list_linked_skills(agent_id: String) -> Result<Vec<String>, AppError> {
    deployment::list_linked_skills(&agent_id).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn unlink_skill_from_agent(skill_name: String, agent_id: String) -> Result<(), AppError> {
    tracing::info!(
        target: "cmd::agents",
        skill_name,
        agent_id,
        "unlink_skill_from_agent called"
    );
    let result = deployment::unlink_skill_from_agent(&skill_name, &agent_id).map_err(|e| {
        tracing::error!(target: "cmd::agents", skill_name, agent_id, error = %e, "unlink_skill_from_agent failed");
        AppError::Other(e.to_string())
    });
    if result.is_ok() {
        tracing::info!(target: "cmd::agents", skill_name, agent_id, "unlink_skill_from_agent completed");
        installed_skill::invalidate_cache();
    }
    result
}

#[tauri::command]
pub async fn batch_remove_skills_from_all_agents(skill_names: Vec<String>) -> Result<(), AppError> {
    for name in &skill_names {
        let _ = deployment::remove_skill_from_all_agents(name);
    }
    installed_skill::invalidate_cache();
    Ok(())
}

#[tauri::command]
pub async fn add_custom_agent_profile(
    def: agent_profile::CustomProfileDef,
) -> Result<(), AppError> {
    agent_profile::add_custom_profile(def).map_err(|e| AppError::Other(e.to_string()))?;
    deployment::invalidate_profile_cache();
    Ok(())
}

#[tauri::command]
pub async fn remove_custom_agent_profile(id: String) -> Result<(), AppError> {
    agent_profile::remove_custom_profile(&id).map_err(|e| AppError::Other(e.to_string()))?;
    deployment::invalidate_profile_cache();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run_batch_toggle;

    #[test]
    fn batch_toggle_report_keeps_successes_and_names_each_failure() {
        let skills = vec!["writing-shape".to_string(), "research".to_string()];
        let report = run_batch_toggle(&skills, |name| {
            if name == "research" {
                anyhow::bail!(
                    "Cannot link Skill 'research': target '/tmp/skills/research' is an unmanaged real directory"
                );
            }
            Ok(())
        });

        assert_eq!(report.succeeded, vec!["writing-shape"]);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].skill_name, "research");
        assert!(report.failed[0].error.contains("unmanaged real directory"));
        assert!(report.failed[0].error.contains("/tmp/skills/research"));
    }
}

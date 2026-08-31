use skillstar_agents as agent_profile;
use skillstar_app::agent_managed_skills;
use skillstar_core::infra::error::AppError;
use skillstar_skills::deployment::{self, ToggleSkillOutcome};
use skillstar_skills::installed_skill;
use std::time::Instant;

#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchSkillToggleFailure {
    pub skill_name: String,
    pub error: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchSkillToggleSkip {
    pub skill_name: String,
    pub code: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchSkillToggleReport {
    pub succeeded: Vec<String>,
    pub skipped: Vec<BatchSkillToggleSkip>,
    pub failed: Vec<BatchSkillToggleFailure>,
}

fn run_batch_toggle<F>(skill_names: &[String], mut toggle: F) -> BatchSkillToggleReport
where
    F: FnMut(&str) -> anyhow::Result<ToggleSkillOutcome>,
{
    let mut report = BatchSkillToggleReport {
        succeeded: Vec::with_capacity(skill_names.len()),
        skipped: Vec::new(),
        failed: Vec::new(),
    };
    for skill_name in skill_names {
        match toggle(skill_name) {
            Ok(ToggleSkillOutcome::Applied) => report.succeeded.push(skill_name.clone()),
            Ok(ToggleSkillOutcome::Skipped { code, path, reason }) => {
                report.skipped.push(BatchSkillToggleSkip {
                    skill_name: skill_name.clone(),
                    code,
                    path,
                    reason,
                })
            }
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
    for skip in &report.skipped {
        tracing::warn!(
            target: "cmd::agents",
            operation = "batch_toggle_skills_for_agent",
            phase = "item_skipped",
            operation_id,
            agent_id,
            enable,
            skill_name = %skip.skill_name,
            reason = %skip.reason,
            "batch Agent skill toggle item skipped"
        );
    }
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
    let skipped = report.skipped.len();
    let failed = report.failed.len();
    let elapsed_ms = started.elapsed().as_millis() as u64;
    if failed == 0 {
        tracing::info!(
            target: "cmd::agents",
            operation = "batch_toggle_skills_for_agent",
            phase = if skipped == 0 {
                "completed"
            } else {
                "completed_with_skips"
            },
            operation_id,
            agent_id,
            enable,
            total,
            succeeded,
            skipped,
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
            skipped,
            failed,
            elapsed_ms,
            "batch Agent skill toggle completed with failures"
        );
    }
    Ok(report)
}

#[tauri::command]
pub async fn get_agent_managed_skills_state(
    agent_id: String,
) -> Result<agent_managed_skills::AgentManagedSkillsState, AppError> {
    agent_managed_skills::get_agent_managed_skills_state(&agent_id)
        .map_err(|error| AppError::Other(error.to_string()))
}

#[tauri::command]
pub async fn toggle_agent_managed_skills(
    agent_id: String,
    operation_id: String,
) -> Result<agent_managed_skills::AgentManagedSkillsToggleReport, AppError> {
    let started = Instant::now();
    tracing::info!(
        target: "cmd::agents",
        operation = "toggle_agent_managed_skills",
        phase = "started",
        operation_id,
        agent_id,
        "temporary Agent managed-skills toggle started"
    );

    let result = agent_managed_skills::toggle_agent_managed_skills(&agent_id).map_err(|error| {
        tracing::error!(
            target: "cmd::agents",
            operation = "toggle_agent_managed_skills",
            phase = "failed",
            operation_id,
            agent_id,
            error = %error,
            "temporary Agent managed-skills toggle failed"
        );
        AppError::Other(error.to_string())
    });

    if let Ok(report) = &result {
        if !report.succeeded.is_empty() {
            installed_skill::invalidate_cache();
        }
        tracing::info!(
            target: "cmd::agents",
            operation = "toggle_agent_managed_skills",
            phase = if report.failed.is_empty() {
                "completed"
            } else {
                "completed_with_failures"
            },
            operation_id,
            agent_id,
            action = ?report.action,
            succeeded = report.succeeded.len(),
            skipped = report.skipped.len(),
            failed = report.failed.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "temporary Agent managed-skills toggle completed"
        );
    }
    result
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
    use skillstar_skills::deployment::ToggleSkillOutcome;

    #[test]
    fn batch_toggle_report_separates_skips_from_failures() {
        let skills = vec![
            "writing-shape".to_string(),
            "research".to_string(),
            "missing".to_string(),
        ];
        let report = run_batch_toggle(&skills, |name| {
            match name {
            "research" => Ok(ToggleSkillOutcome::Skipped {
                code: "unmanaged_real_directory".into(),
                path: "/tmp/skills/research".into(),
                reason: "name collision: target '/tmp/skills/research' is an unmanaged real directory (left in place)"
                    .into(),
            }),
            "missing" => anyhow::bail!("Skill 'missing' not found in hub"),
            _ => Ok(ToggleSkillOutcome::Applied),
        }
        });

        assert_eq!(report.succeeded, vec!["writing-shape"]);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].skill_name, "research");
        assert_eq!(report.skipped[0].code, "unmanaged_real_directory");
        assert_eq!(report.skipped[0].path, "/tmp/skills/research");
        assert!(
            report.skipped[0]
                .reason
                .contains("unmanaged real directory")
        );
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].skill_name, "missing");
        assert!(report.failed[0].error.contains("not found in hub"));
    }
}

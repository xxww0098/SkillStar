use skillstar_agents as agent_profile;
use skillstar_core::infra::error::AppError;
use skillstar_skills::deployment;
use skillstar_skills::installed_skill;

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

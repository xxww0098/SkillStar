use crate::core::{agent_profile, error::AppError, installed_skill, sync};

#[tauri::command]
pub async fn list_agent_profiles() -> Result<Vec<agent_profile::AgentProfile>, AppError> {
    Ok(agent_profile::list_profiles())
}

#[tauri::command]
pub async fn toggle_agent_profile(id: String) -> Result<bool, AppError> {
    agent_profile::toggle_profile(&id).map_err(|e| AppError::AgentProfile(e.to_string()))
}

#[tauri::command]
pub async fn unlink_all_skills_from_agent(agent_id: String) -> Result<u32, AppError> {
    let removed = sync::unlink_all_skills_from_agent(&agent_id)?;
    installed_skill::invalidate_cache();
    Ok(removed)
}

#[tauri::command]
pub async fn batch_link_skills_to_agent(
    skill_names: Vec<String>,
    agent_id: String,
) -> Result<u32, AppError> {
    let linked = sync::batch_link_skills_to_agent(&skill_names, &agent_id)?;
    installed_skill::invalidate_cache();
    Ok(linked)
}

#[tauri::command]
pub async fn list_linked_skills(agent_id: String) -> Result<Vec<String>, AppError> {
    sync::list_linked_skills(&agent_id).map_err(|e| AppError::Anyhow(e))
}

#[tauri::command]
pub async fn unlink_skill_from_agent(skill_name: String, agent_id: String) -> Result<(), AppError> {
    sync::unlink_skill_from_agent(&skill_name, &agent_id)?;
    installed_skill::invalidate_cache();
    Ok(())
}

#[tauri::command]
pub async fn batch_remove_skills_from_all_agents(skill_names: Vec<String>) -> Result<(), AppError> {
    for name in skill_names {
        let _ = sync::remove_skill_from_all_agents(&name);
    }
    installed_skill::invalidate_cache();
    Ok(())
}

#[tauri::command]
pub async fn add_custom_agent_profile(
    def: agent_profile::CustomProfileDef,
) -> Result<(), AppError> {
    agent_profile::add_custom_profile(def).map_err(|e| AppError::AgentProfile(e.to_string()))
}

#[tauri::command]
pub async fn remove_custom_agent_profile(id: String) -> Result<(), AppError> {
    agent_profile::remove_custom_profile(&id).map_err(|e| AppError::AgentProfile(e.to_string()))
}

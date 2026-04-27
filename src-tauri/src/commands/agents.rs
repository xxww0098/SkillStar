use crate::core::infra::error::AppError;
use crate::core::installed_skill;
use skillstar_commands::agents as core_agents;

#[tauri::command]
pub async fn list_agent_profiles() -> Result<Vec<core_agents::AgentProfile>, AppError> {
    core_agents::list_agent_profiles().await
}

#[tauri::command]
pub async fn toggle_agent_profile(id: String) -> Result<bool, AppError> {
    core_agents::toggle_agent_profile(id).await
}

#[tauri::command]
pub async fn unlink_all_skills_from_agent(agent_id: String) -> Result<u32, AppError> {
    let result = core_agents::unlink_all_skills_from_agent(agent_id.clone()).await;
    if result.is_ok() {
        installed_skill::invalidate_cache();
    }
    result
}

#[tauri::command]
pub async fn batch_link_skills_to_agent(
    skill_names: Vec<String>,
    agent_id: String,
) -> Result<u32, AppError> {
    let result =
        core_agents::batch_link_skills_to_agent(skill_names.clone(), agent_id.clone()).await;
    if result.is_ok() {
        installed_skill::invalidate_cache();
    }
    result
}

#[tauri::command]
pub async fn list_linked_skills(agent_id: String) -> Result<Vec<String>, AppError> {
    core_agents::list_linked_skills(agent_id).await
}

#[tauri::command]
pub async fn unlink_skill_from_agent(skill_name: String, agent_id: String) -> Result<(), AppError> {
    let result = core_agents::unlink_skill_from_agent(skill_name.clone(), agent_id.clone()).await;
    if result.is_ok() {
        installed_skill::invalidate_cache();
    }
    result
}

#[tauri::command]
pub async fn batch_remove_skills_from_all_agents(skill_names: Vec<String>) -> Result<(), AppError> {
    let result = core_agents::batch_remove_skills_from_all_agents(skill_names.clone()).await;
    if result.is_ok() {
        installed_skill::invalidate_cache();
    }
    result
}

#[tauri::command]
pub async fn add_custom_agent_profile(def: core_agents::CustomProfileDef) -> Result<(), AppError> {
    core_agents::add_custom_agent_profile(def).await
}

#[tauri::command]
pub async fn remove_custom_agent_profile(id: String) -> Result<(), AppError> {
    core_agents::remove_custom_agent_profile(id).await
}

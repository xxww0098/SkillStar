use crate::core::{agent_profile, sync};

#[tauri::command]
pub async fn list_agent_profiles() -> Result<Vec<agent_profile::AgentProfile>, String> {
    Ok(agent_profile::list_profiles())
}

#[tauri::command]
pub async fn toggle_agent_profile(id: String) -> Result<bool, String> {
    agent_profile::toggle_profile(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unlink_all_skills_from_agent(agent_id: String) -> Result<u32, String> {
    sync::unlink_all_skills_from_agent(&agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn batch_link_skills_to_agent(
    skill_names: Vec<String>,
    agent_id: String,
) -> Result<u32, String> {
    sync::batch_link_skills_to_agent(&skill_names, &agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_linked_skills(agent_id: String) -> Result<Vec<String>, String> {
    sync::list_linked_skills(&agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unlink_skill_from_agent(skill_name: String, agent_id: String) -> Result<(), String> {
    sync::unlink_skill_from_agent(&skill_name, &agent_id).map_err(|e| e.to_string())
}

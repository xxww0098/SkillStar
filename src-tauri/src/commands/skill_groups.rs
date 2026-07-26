use std::collections::HashMap;

use skillstar_core::infra::error::AppError;
use skillstar_skills::skill_group::{self, SkillGroup};

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
    skillstar_app::skill_group_deploy::deploy(group_id, project_path, agent_types).await
}

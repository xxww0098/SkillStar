use crate::core::{project_manifest, sync};
use std::collections::HashMap;

#[tauri::command]
pub async fn create_project_skills(
    project_path: String,
    selected_skills: Vec<String>,
    agent_types: Vec<String>,
) -> Result<u32, String> {
    sync::create_project_skills(
        &std::path::PathBuf::from(project_path),
        &selected_skills,
        &agent_types,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn register_project(
    project_path: String,
) -> Result<project_manifest::ProjectEntry, String> {
    project_manifest::register_project(&project_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_projects() -> Result<Vec<project_manifest::ProjectEntry>, String> {
    Ok(project_manifest::list_projects())
}

#[tauri::command]
pub async fn get_project_skills(
    name: String,
) -> Result<Option<project_manifest::SkillsList>, String> {
    Ok(project_manifest::load_skills_list(&name))
}

#[tauri::command]
pub async fn save_and_sync_project(
    project_path: String,
    agents: HashMap<String, Vec<String>>,
) -> Result<u32, String> {
    let (_name, count) =
        project_manifest::save_and_sync(&project_path, agents).map_err(|e| e.to_string())?;
    Ok(count)
}

#[tauri::command]
pub async fn save_project_skills_list(
    project_path: String,
    agents: HashMap<String, Vec<String>>,
) -> Result<project_manifest::SkillsList, String> {
    project_manifest::save_skills_list_only(&project_path, agents).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_project_path(name: String, new_path: String) -> Result<u32, String> {
    project_manifest::update_project_path(&name, &new_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_project(name: String) -> Result<(), String> {
    project_manifest::remove_project(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_project_skills(
    project_path: String,
) -> Result<project_manifest::ProjectScanResult, String> {
    Ok(project_manifest::scan_project_skills(&project_path))
}

#[tauri::command]
pub async fn rebuild_project_skills_from_disk(
    project_path: String,
) -> Result<project_manifest::SkillsList, String> {
    project_manifest::rebuild_skills_list_from_disk(&project_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_project_skills(
    project_path: String,
    project_name: String,
    targets: Vec<project_manifest::ImportTarget>,
) -> Result<project_manifest::ImportResult, String> {
    project_manifest::import_scanned_skills(&project_path, &project_name, &targets)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn detect_project_agents(
    project_path: String,
) -> Result<project_manifest::ProjectAgentDetection, String> {
    Ok(project_manifest::detect_project_agents(&project_path))
}

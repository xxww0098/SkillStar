use skillstar_core::infra::error::AppError;
use skillstar_skills::SkillContent;
use skillstar_skills::content as skill_content;

#[tauri::command]
pub async fn read_skill_file_raw(name: String) -> Result<String, AppError> {
    skill_content::read_raw(&name)
}

#[tauri::command]
pub async fn delete_local_skill(name: String) -> Result<(), AppError> {
    skill_content::delete_local(&name)
}

#[tauri::command]
pub async fn migrate_local_skills() -> Result<u32, AppError> {
    tokio::task::spawn_blocking(skill_content::migrate_local_skills).await?
}

#[tauri::command]
pub async fn list_skill_files(name: String) -> Result<Vec<String>, AppError> {
    skill_content::list_files(&name)
}

#[tauri::command]
pub async fn read_skill_content(name: String) -> Result<SkillContent, AppError> {
    skill_content::read(&name)
}

#[tauri::command]
pub async fn update_skill_content(name: String, content: String) -> Result<(), AppError> {
    skill_content::update(&name, &content)
}

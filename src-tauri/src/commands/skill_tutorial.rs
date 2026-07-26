//! Thin Tauri adapters for persistent ACP-generated Skill tutorials.

use skillstar_core::infra::error::AppError;
use skillstar_skills::tutorial::TutorialArtifact;

#[tauri::command]
pub async fn get_skill_tutorial(
    name: String,
    locale: String,
) -> Result<TutorialArtifact, AppError> {
    crate::core::skill_tutorial::load_for_skill(&name, &locale).await
}

#[tauri::command]
pub async fn generate_skill_tutorial(
    name: String,
    locale: String,
    force_refresh: Option<bool>,
) -> Result<TutorialArtifact, AppError> {
    crate::core::skill_tutorial::generate_for_skill(&name, &locale, force_refresh.unwrap_or(false))
        .await
}

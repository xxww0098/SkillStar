//! Thin Tauri adapters for Learn / Guide / Draft.

use skillstar_app::learning::dto::{
    GuideDraftDto, GuideDto, GuideSummaryDto, LearningProgressDto, PracticeInstallPreviewDto,
    ProgressSnapshotDto,
};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub fn list_guides() -> Result<Vec<GuideSummaryDto>, AppError> {
    skillstar_app::learning::list_guides()
}

#[tauri::command]
pub fn get_guide(id: String) -> Result<Option<GuideDto>, AppError> {
    skillstar_app::learning::get_guide(&id)
}

#[tauri::command]
pub fn load_learning_progress(
    guide_id: String,
    guide_revision_key: String,
) -> Result<ProgressSnapshotDto, AppError> {
    skillstar_app::learning::load_guide_progress(&guide_id, &guide_revision_key)
}

#[tauri::command]
pub fn save_learning_progress(
    guide_id: String,
    guide_revision_key: String,
    current_step_id: String,
    completed_step_ids: Vec<String>,
) -> Result<LearningProgressDto, AppError> {
    skillstar_app::learning::save_guide_progress(
        guide_id,
        guide_revision_key,
        current_step_id,
        completed_step_ids,
    )
}

#[tauri::command]
pub fn preview_practice_install(
    guide_id: String,
    step_id: String,
) -> Result<PracticeInstallPreviewDto, AppError> {
    skillstar_app::learning::preview_practice_install(&guide_id, &step_id)
}

#[tauri::command]
pub fn preview_guide_draft(name: String, locale: String) -> Result<GuideDraftDto, AppError> {
    skillstar_app::learning::preview_guide_draft(&name, &locale)
}

#[tauri::command]
pub fn create_guide_draft(name: String, locale: String) -> Result<GuideDraftDto, AppError> {
    skillstar_app::learning::create_guide_draft(&name, &locale)
}

#[tauri::command]
pub fn list_guide_drafts() -> Result<Vec<GuideDraftDto>, AppError> {
    skillstar_app::learning::list_guide_drafts()
}

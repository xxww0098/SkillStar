//! Guide, progress and draft placeholders owned by this crate.
//!
//! Product surface lands in later tickets; the types and fail-closed Draft
//! gate exist so callers cannot keep those rules in skills or Tauri.

use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;

use crate::identity::SkillRevisionKey;
use crate::tutorial::PrivateTutorial;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideSummary {
    pub id: String,
    pub title: String,
    pub revision_key: SkillRevisionKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningProgress {
    pub guide_revision_key: SkillRevisionKey,
    pub completed_steps: Vec<String>,
}

pub fn list_guides() -> Result<Vec<GuideSummary>, AppError> {
    Ok(Vec::new())
}

pub fn get_guide(_id: &str) -> Result<Option<GuideSummary>, AppError> {
    Ok(None)
}

pub fn load_progress(
    _guide_revision: &SkillRevisionKey,
) -> Result<Option<LearningProgress>, AppError> {
    Ok(None)
}

pub fn save_progress(progress: &LearningProgress) -> Result<(), AppError> {
    let _ = progress;
    Err(AppError::Other(
        "Learning progress persistence is not available until the Learn UI lands".to_string(),
    ))
}

pub fn create_guide_draft_from_tutorial(tutorial: &PrivateTutorial) -> Result<(), AppError> {
    crate::tutorial::create_guide_draft_from_tutorial(tutorial)
}

//! Desktop approval for a project-skill plan.
//!
//! The body only reads plans and records a SkillStar approval. It does not
//! deploy, and it does not touch the external MCP command module.

use chrono::Utc;
use skillstar_app::project_skills_mcp::host::{
    ProjectSkillPlanDiff, approve_plan_from_skillstar, pending_plan_diffs,
};
use skillstar_core::infra::error::AppError;

#[tauri::command]
pub async fn list_pending_project_skill_plans(
    project_path: String,
) -> Result<Vec<ProjectSkillPlanDiff>, AppError> {
    pending_plan_diffs(&project_path, Utc::now()).map_err(|err| AppError::Other(err.to_string()))
}

#[tauri::command]
pub async fn approve_project_skill_plan(plan_id: String) -> Result<(), AppError> {
    approve_plan_from_skillstar(&plan_id, Utc::now())
        .map_err(|err| AppError::Other(err.to_string()))
}

use crate::core::{
    infra::error::AppError,
    installed_skill, local_skill,
    projects::sync,
    skill::{Skill, extract_github_source_from_url, extract_skill_description},
    skills::{DefaultSkillManager, SkillManager},
};

use super::skill_paths::resolve_skill_content_dir;

/// Result of updating a single skill. For repo-cached skills, pulling the
/// repo also advances all sibling skills from the same repository.
#[derive(serde::Serialize)]
pub struct UpdateResult {
    /// The skill that was explicitly updated.
    pub skill: Skill,
    /// Names of sibling skills from the same repo whose update state was
    /// also cleared by the pull. The frontend should set
    /// `update_available = false` for these.
    pub siblings_cleared: Vec<String>,
}

#[tauri::command]
pub async fn list_skills() -> Result<Vec<Skill>, AppError> {
    installed_skill::list_installed_skills()
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn refresh_skill_updates(
    names: Option<Vec<String>>,
) -> Result<Vec<installed_skill::SkillUpdateState>, AppError> {
    installed_skill::refresh_skill_updates(names)
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn install_skill(url: String, name: Option<String>) -> Result<Skill, AppError> {
    tokio::task::spawn_blocking(move || DefaultSkillManager.install_skill(url, name))
        .await
        .map_err(|e| AppError::Other(format!("install task panicked: {e}")))?
        .map_err(AppError::Other)
}

#[tauri::command]
pub async fn uninstall_skill(name: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || uninstall_skill_sync(name))
        .await
        .map_err(|e| AppError::Other(format!("uninstall task panicked: {e}")))?
}

fn uninstall_skill_sync(name: String) -> Result<(), AppError> {
    DefaultSkillManager
        .uninstall_skill(&name)
        .map_err(AppError::Other)
}

#[tauri::command]
pub async fn toggle_skill_for_agent(
    skill_name: String,
    agent_id: String,
    enable: bool,
) -> Result<(), AppError> {
    tracing::info!(
        target: "cmd",
        skill_name,
        agent_id,
        enable,
        "toggle_skill_for_agent called"
    );
    sync::toggle_skill_for_agent(&skill_name, &agent_id, enable).map_err(|e| {
        tracing::error!(target: "cmd", skill_name, agent_id, enable, error = %e, "toggle_skill_for_agent failed");
        AppError::Anyhow(e)
    })?;
    crate::core::installed_skill::invalidate_cache();
    tracing::info!(target: "cmd", skill_name, agent_id, enable, "toggle_skill_for_agent completed");
    Ok(())
}

#[tauri::command]
pub async fn update_skill(name: String) -> Result<UpdateResult, AppError> {
    tokio::task::spawn_blocking(move || update_skill_sync(name))
        .await
        .map_err(|e| AppError::Other(format!("update task panicked: {e}")))?
}

fn update_skill_sync(name: String) -> Result<UpdateResult, AppError> {
    let outcome = DefaultSkillManager
        .update_skill(&name)
        .map_err(AppError::Anyhow)?;

    let skills_dir = crate::core::infra::paths::hub_skills_dir();
    let path = skills_dir.join(&name);

    let description = resolve_skill_content_dir(&name)
        .map(|dir| extract_skill_description(&dir))
        .unwrap_or_else(|| extract_skill_description(&path));

    let source = extract_github_source_from_url(&outcome.git_url);

    let skill_type = if local_skill::is_local_skill(&name) {
        crate::core::skill::SkillType::Local
    } else {
        crate::core::skill::SkillType::Hub
    };

    Ok(UpdateResult {
        skill: Skill {
            name,
            description,
            localized_description: None,
            skill_type,
            stars: 0,
            installed: true,
            update_available: false,
            last_updated: chrono::Utc::now().to_rfc3339(),
            git_url: outcome.git_url,
            tree_hash: Some(outcome.tree_hash),
            category: crate::core::skill::SkillCategory::None,
            author: None,
            topics: Vec::new(),
            agent_links: Some(outcome.agent_links),
            rank: None,
            source,
        },
        siblings_cleared: outcome.sibling_names,
    })
}

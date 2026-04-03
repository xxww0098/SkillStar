use crate::core::{
    acp_client,
    error::AppError,
    setup_hook::{self, DiscoveredRebuildSkill, HookRunOutput, SetupHook},
};

/// Launch an ACP Agent to generate a setup hook for a skill.
///
/// The agent will analyse the repo, write and execute a setup script,
/// then return it. SkillStar saves the script for future re-use.
#[tauri::command]
pub async fn acp_generate_setup_hook(
    skill_name: String,
    agent_command: String,
) -> Result<SetupHook, AppError> {
    let result = acp_client::run_setup_via_acp(&agent_command, &skill_name, |_chunk| {
        // Streaming chunks could be forwarded to UI via Tauri events in future.
    })
    .await
    .map_err(|e| AppError::Other(format!("ACP setup failed: {}", e)))?;

    // Save the successfully-executed script
    setup_hook::save_hook(&skill_name, &result.script)
        .map_err(|e| AppError::Other(format!("Failed to save setup hook: {}", e)))?;

    // Return the saved hook
    setup_hook::load_hook(&skill_name)
        .ok_or_else(|| AppError::Other("Failed to load saved hook".to_string()))
}

/// Launch an ACP Agent to build a multi-skill repo and create skills-rebuild/.
///
/// Full flow:
/// 1. ACP agent analyzes the repo and builds it
/// 2. Agent creates `skills-rebuild/<name>/SKILL.md` for each skill
/// 3. SkillStar saves the build script as a setup hook
/// 4. SkillStar removes the old monolithic symlink and creates individual skill symlinks
///
/// Returns the list of newly installed skill names.
#[tauri::command]
pub async fn acp_rebuild_skills(
    skill_name: String,
    agent_command: String,
    repo_url: String,
) -> Result<Vec<String>, AppError> {
    // Step 1: Run ACP with REBUILD_PROMPT
    let result = acp_client::run_rebuild_via_acp(&agent_command, &skill_name, |_chunk| {})
        .await
        .map_err(|e| AppError::Other(format!("ACP rebuild failed: {}", e)))?;

    // Step 2: Save the script as a setup hook for future re-use
    setup_hook::save_hook(&skill_name, &result.script)
        .map_err(|e| AppError::Other(format!("Failed to save rebuild hook: {}", e)))?;

    // Step 3: Rebuild skills from skills-rebuild/
    setup_hook::rebuild_skills_from_repo(&skill_name, &repo_url)
        .map_err(|e| AppError::Other(format!("Failed to rebuild skills: {}", e)))
}

/// Scan the skills-rebuild/ directory for a skill (preview before applying).
#[tauri::command]
pub async fn scan_rebuild_skills(
    skill_name: String,
) -> Result<Vec<DiscoveredRebuildSkill>, AppError> {
    Ok(setup_hook::scan_rebuild_dir(&skill_name))
}

/// Apply skill rebuild from an existing skills-rebuild/ directory.
///
/// Use this when the setup hook has already been run (or manually built)
/// and you just want to create the symlinks.
#[tauri::command]
pub async fn apply_rebuild_skills(
    skill_name: String,
    repo_url: String,
) -> Result<Vec<String>, AppError> {
    setup_hook::rebuild_skills_from_repo(&skill_name, &repo_url)
        .map_err(|e| AppError::Other(format!("Failed to rebuild skills: {}", e)))
}

/// Get the setup hook for a skill (if any).
#[tauri::command]
pub async fn get_setup_hook(skill_name: String) -> Result<Option<SetupHook>, AppError> {
    Ok(setup_hook::load_hook(&skill_name))
}

/// Save a setup hook script (manual edit).
#[tauri::command]
pub async fn save_setup_hook(skill_name: String, script: String) -> Result<(), AppError> {
    setup_hook::save_hook(&skill_name, &script)
        .map_err(|e| AppError::Other(format!("Failed to save setup hook: {}", e)))
}

/// Delete a setup hook.
#[tauri::command]
pub async fn delete_setup_hook(skill_name: String) -> Result<(), AppError> {
    setup_hook::delete_hook(&skill_name)
        .map_err(|e| AppError::Other(format!("Failed to delete setup hook: {}", e)))
}

/// Manually execute the setup hook for a skill.
#[tauri::command]
pub async fn run_setup_hook(skill_name: String) -> Result<HookRunOutput, AppError> {
    setup_hook::execute_hook(&skill_name)
        .await
        .map_err(|e| AppError::Other(format!("Setup hook execution failed: {}", e)))
}

/// Get the ACP configuration.
#[tauri::command]
pub async fn get_acp_config() -> Result<acp_client::AcpConfig, AppError> {
    Ok(acp_client::load_config())
}

/// Save ACP configuration.
#[tauri::command]
pub async fn save_acp_config(config: acp_client::AcpConfig) -> Result<(), AppError> {
    acp_client::save_config(&config)
        .map_err(|e| AppError::Other(format!("Failed to save ACP config: {}", e)))
}

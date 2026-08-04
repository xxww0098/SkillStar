use super::ai::ensure_ai_config;
use skillstar_core::infra::error::AppError;
use skillstar_marketplace::snapshot;
use skillstar_marketplace::{
    AiKeywordSearchResult, LocalFirstResult, MarketplaceSkillDetails, OfficialPublisher,
    PublisherRepo, Skill, SyncStateEntry,
};
use skillstar_models::ai_provider;
use std::collections::HashMap;
use tracing::{debug, info};

#[tauri::command]
pub async fn resolve_skill_sources(
    names: Vec<String>,
    existing_sources: HashMap<String, String>,
) -> Result<HashMap<String, String>, AppError> {
    debug!(target: "marketplace", count = names.len(), "resolve_skill_sources called");
    let total = names.len();
    let resolved = snapshot::resolve_skill_sources_local_first(&names, &existing_sources)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    info!(target: "marketplace", resolved = resolved.len(), total = total, "resolve_skill_sources done");
    Ok(resolved)
}

#[tauri::command]
pub async fn ai_extract_search_keywords(query: String) -> Result<Vec<String>, AppError> {
    debug!(target: "marketplace", query = %query, "ai_extract_search_keywords called");
    let resolved = ensure_ai_config().await?;
    let keywords = ai_provider::extract_search_keywords(&resolved, &query)
        .await
        .map_err(|e| AppError::Other(format!("AI keyword extraction failed: {}", e)))?;
    info!(
        target: "marketplace",
        count = keywords.len(),
        keywords = ?keywords,
        "ai_extract_search_keywords success"
    );
    Ok(keywords)
}

#[tauri::command]
pub async fn get_leaderboard_local(
    category: String,
) -> Result<LocalFirstResult<Vec<Skill>>, AppError> {
    snapshot::get_leaderboard_local(&category)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_skills_local() -> Result<LocalFirstResult<Vec<Skill>>, AppError> {
    snapshot::list_skills_local()
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn search_marketplace_local(
    query: String,
    limit: Option<u32>,
) -> Result<LocalFirstResult<Vec<Skill>>, AppError> {
    snapshot::search_local(&query, limit)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn get_publishers_local() -> Result<LocalFirstResult<Vec<OfficialPublisher>>, AppError> {
    snapshot::get_publishers_local()
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn get_publisher_repos_local(
    publisher_name: String,
) -> Result<LocalFirstResult<Vec<PublisherRepo>>, AppError> {
    snapshot::get_publisher_repos_local(&publisher_name)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn get_repo_skills_local(
    source: String,
) -> Result<LocalFirstResult<Vec<Skill>>, AppError> {
    snapshot::get_repo_skills_local(&source)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn get_skill_detail_local(
    source: String,
    name: String,
) -> Result<LocalFirstResult<MarketplaceSkillDetails>, AppError> {
    snapshot::get_skill_detail_local(&source, &name)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn ai_search_marketplace_local(
    keywords: Vec<String>,
    limit: Option<u32>,
) -> Result<LocalFirstResult<AiKeywordSearchResult>, AppError> {
    snapshot::ai_search_local(&keywords, limit)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn sync_marketplace_scope(scope: String) -> Result<(), AppError> {
    snapshot::sync_marketplace_scope(&scope)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn get_marketplace_sync_states() -> Result<Vec<SyncStateEntry>, AppError> {
    snapshot::get_marketplace_sync_states().map_err(|e| AppError::Other(e.to_string()))
}

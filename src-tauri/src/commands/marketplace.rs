use crate::core::{
    infra::error::AppError,
    marketplace,
    skill::{OfficialPublisher, Skill},
};
use std::collections::HashMap;
use tracing::{debug, error, info};

#[tauri::command]
pub async fn search_skills_sh(query: String) -> Result<marketplace::MarketplaceResult, AppError> {
    debug!(target: "marketplace", query = %query, "search_skills_sh called");
    match marketplace::search_skills_sh(&query, 200).await {
        Ok(result) => {
            debug!(target: "marketplace", count = result.skills.len(), "search_skills_sh success");
            Ok(result)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "search_skills_sh failed");
            Err(AppError::Marketplace(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_skills_sh_leaderboard(category: String) -> Result<Vec<Skill>, AppError> {
    debug!(target: "marketplace", category = %category, "get_skills_sh_leaderboard called");
    match marketplace::get_skills_sh_leaderboard(&category).await {
        Ok(result) => {
            debug!(target: "marketplace", count = result.len(), "get_skills_sh_leaderboard success");
            Ok(result)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "get_skills_sh_leaderboard failed");
            Err(AppError::Marketplace(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_official_publishers() -> Result<Vec<OfficialPublisher>, AppError> {
    debug!(target: "marketplace", "get_official_publishers called");
    match marketplace::get_official_publishers().await {
        Ok(result) => {
            debug!(target: "marketplace", count = result.len(), "get_official_publishers success");
            Ok(result)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "get_official_publishers failed");
            Err(AppError::Marketplace(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_publisher_repos(
    publisher_name: String,
) -> Result<Vec<marketplace::PublisherRepo>, AppError> {
    debug!(target: "marketplace", publisher = %publisher_name, "get_publisher_repos called");
    match marketplace::get_publisher_repos(&publisher_name).await {
        Ok(repos) => {
            debug!(target: "marketplace", count = repos.len(), "get_publisher_repos success");
            Ok(repos)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "get_publisher_repos failed");
            Err(AppError::Marketplace(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_publisher_repo_skills(
    publisher_name: String,
    repo_name: String,
) -> Result<Vec<marketplace::PublisherRepoSkill>, AppError> {
    debug!(target: "marketplace", publisher = %publisher_name, repo = %repo_name, "get_publisher_repo_skills called");
    match marketplace::get_publisher_repo_skills(&publisher_name, &repo_name).await {
        Ok(skills) => {
            debug!(target: "marketplace", count = skills.len(), "get_publisher_repo_skills success");
            Ok(skills)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "get_publisher_repo_skills failed");
            Err(AppError::Marketplace(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_marketplace_skill_details(
    source: String,
    name: String,
) -> Result<marketplace::MarketplaceSkillDetails, AppError> {
    debug!(target: "marketplace", source = %source, name = %name, "get_marketplace_skill_details called");
    match marketplace::fetch_marketplace_skill_details(&source, &name).await {
        Ok(details) => {
            debug!(
                target: "marketplace",
                has_summary = details.summary.is_some(),
                has_readme = details.readme.is_some(),
                audits = details.security_audits.len(),
                "get_marketplace_skill_details success"
            );
            Ok(details)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "get_marketplace_skill_details failed");
            Err(AppError::Marketplace(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn resolve_skill_sources(
    names: Vec<String>,
    existing_sources: HashMap<String, String>,
) -> Result<HashMap<String, String>, AppError> {
    debug!(target: "marketplace", count = names.len(), "resolve_skill_sources called");
    let total = names.len();
    let resolved =
        marketplace::resolve_skill_sources_local_first(&names, &existing_sources).await?;
    info!(target: "marketplace", resolved = resolved.len(), total = total, "resolve_skill_sources done");
    Ok(resolved)
}

#[tauri::command]
pub async fn ai_extract_search_keywords(query: String) -> Result<Vec<String>, AppError> {
    debug!(target: "marketplace", query = %query, "ai_extract_search_keywords called");
    let config = crate::commands::ai::ensure_ai_config_pub()
        .await
        .map_err(|e| {
            error!(target: "marketplace", error = %e, "ai_extract_search_keywords config error");
            AppError::AiProvider(e)
        })?;
    let keywords = crate::core::ai_provider::extract_search_keywords(&config, &query)
        .await
        .map_err(|e| {
            error!(target: "marketplace", error = %e, "ai_extract_search_keywords failed");
            AppError::AiProvider(format!("AI keyword extraction failed: {}", e))
        })?;
    info!(
        target: "marketplace",
        count = keywords.len(),
        keywords = ?keywords,
        "ai_extract_search_keywords success"
    );
    Ok(keywords)
}

#[tauri::command]
pub async fn ai_search_with_keywords(
    keywords: Vec<String>,
) -> Result<marketplace::AiKeywordSearchResult, AppError> {
    debug!(target: "marketplace", count = keywords.len(), keywords = ?keywords, "ai_search_with_keywords called");
    let result = marketplace::ai_search_by_keywords(&keywords)
        .await
        .map_err(|e| {
            error!(target: "marketplace", error = %e, "ai_search_with_keywords failed");
            AppError::Marketplace(format!("AI marketplace search failed: {}", e))
        })?;
    info!(
        target: "marketplace",
        skills = result.skills.len(),
        keyword_map_keys = ?result.keyword_skill_map.keys().collect::<Vec<_>>(),
        "ai_search_with_keywords success"
    );
    Ok(result)
}

#[tauri::command]
pub async fn get_leaderboard_local(
    category: String,
) -> Result<marketplace::LocalFirstResult<Vec<Skill>>, AppError> {
    marketplace::get_leaderboard_local(&category)
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn search_marketplace_local(
    query: String,
    limit: Option<u32>,
) -> Result<marketplace::LocalFirstResult<Vec<Skill>>, AppError> {
    marketplace::search_marketplace_local(&query, limit)
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn get_publishers_local()
-> Result<marketplace::LocalFirstResult<Vec<OfficialPublisher>>, AppError> {
    marketplace::get_publishers_local()
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn get_publisher_repos_local(
    publisher_name: String,
) -> Result<marketplace::LocalFirstResult<Vec<marketplace::PublisherRepo>>, AppError> {
    marketplace::get_publisher_repos_local(&publisher_name)
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn get_repo_skills_local(
    source: String,
) -> Result<marketplace::LocalFirstResult<Vec<Skill>>, AppError> {
    marketplace::get_repo_skills_local(&source)
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn get_skill_detail_local(
    source: String,
    name: String,
) -> Result<marketplace::LocalFirstResult<marketplace::MarketplaceSkillDetails>, AppError> {
    marketplace::get_skill_detail_local(&source, &name)
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn ai_search_marketplace_local(
    keywords: Vec<String>,
    limit: Option<u32>,
) -> Result<marketplace::LocalFirstResult<marketplace::AiKeywordSearchResult>, AppError> {
    marketplace::ai_search_marketplace_local(&keywords, limit)
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn sync_marketplace_scope(scope: String) -> Result<(), AppError> {
    marketplace::sync_marketplace_scope_local(&scope)
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn get_marketplace_sync_states() -> Result<Vec<marketplace::SyncStateEntry>, AppError> {
    marketplace::get_marketplace_local_sync_states().map_err(AppError::Anyhow)
}

// ── Pack Commands ────────────────────────────────────────────────────

#[tauri::command]
pub async fn search_marketplace_packs(
    query: String,
    limit: Option<u32>,
) -> Result<Vec<marketplace::MarketplacePack>, AppError> {
    marketplace::search_marketplace_packs_local(&query, limit.unwrap_or(20))
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn list_marketplace_packs(
    limit: Option<u32>,
) -> Result<Vec<marketplace::MarketplacePack>, AppError> {
    marketplace::list_marketplace_packs_local(limit.unwrap_or(50)).map_err(AppError::Anyhow)
}

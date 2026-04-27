use skillstar_ai::ai_provider::{self, ApiFormat};
use skillstar_infra::error::AppError;
use skillstar_marketplace_core::remote;
use skillstar_marketplace_core::snapshot;
use skillstar_marketplace_core::{
    AiKeywordSearchResult, CuratedRegistryEntry, CuratedRegistryUpsert, LocalFirstResult,
    MarketplaceCategory, MarketplaceCategoryUpsert, MarketplacePack, MarketplaceRatingSummary,
    MarketplaceRatingSummaryUpsert, MarketplaceResult, MarketplaceReview, MarketplaceReviewUpsert,
    MarketplaceSkillCategoryAssignment, MarketplaceSkillCategoryAssignmentInput,
    MarketplaceSkillDetails, MarketplaceSkillTagAssignment, MarketplaceSkillTagAssignmentInput,
    MarketplaceSourceObservation, MarketplaceSourceSummary, MarketplaceTag, MarketplaceTagUpsert,
    MarketplaceUpdateNotification, MarketplaceUpdateNotificationUpsert, OfficialPublisher,
    PublisherRepo, PublisherRepoSkill, Skill, SyncStateEntry,
};
use std::collections::HashMap;
use tracing::{debug, error, info};

#[tauri::command]
pub async fn search_skills_sh(query: String) -> Result<MarketplaceResult, AppError> {
    debug!(target: "marketplace", query = %query, "search_skills_sh called");
    match remote::search_skills_sh(&query, 200).await {
        Ok(result) => {
            debug!(target: "marketplace", count = result.skills.len(), "search_skills_sh success");
            Ok(result)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "search_skills_sh failed");
            Err(AppError::Other(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_skills_sh_leaderboard(category: String) -> Result<Vec<Skill>, AppError> {
    debug!(target: "marketplace", category = %category, "get_skills_sh_leaderboard called");
    match remote::get_skills_sh_leaderboard(&category).await {
        Ok(result) => {
            debug!(target: "marketplace", count = result.len(), "get_skills_sh_leaderboard success");
            Ok(result)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "get_skills_sh_leaderboard failed");
            Err(AppError::Other(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_official_publishers() -> Result<Vec<OfficialPublisher>, AppError> {
    debug!(target: "marketplace", "get_official_publishers called");
    match remote::get_official_publishers().await {
        Ok(result) => {
            debug!(target: "marketplace", count = result.len(), "get_official_publishers success");
            Ok(result)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "get_official_publishers failed");
            Err(AppError::Other(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_publisher_repos(publisher_name: String) -> Result<Vec<PublisherRepo>, AppError> {
    debug!(target: "marketplace", publisher = %publisher_name, "get_publisher_repos called");
    match remote::get_publisher_repos(&publisher_name).await {
        Ok(repos) => {
            debug!(target: "marketplace", count = repos.len(), "get_publisher_repos success");
            Ok(repos)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "get_publisher_repos failed");
            Err(AppError::Other(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_publisher_repo_skills(
    publisher_name: String,
    repo_name: String,
) -> Result<Vec<PublisherRepoSkill>, AppError> {
    debug!(target: "marketplace", publisher = %publisher_name, repo = %repo_name, "get_publisher_repo_skills called");
    match remote::get_publisher_repo_skills(&publisher_name, &repo_name).await {
        Ok(skills) => {
            debug!(target: "marketplace", count = skills.len(), "get_publisher_repo_skills success");
            Ok(skills)
        }
        Err(e) => {
            error!(target: "marketplace", error = %e, "get_publisher_repo_skills failed");
            Err(AppError::Other(e.to_string()))
        }
    }
}

#[tauri::command]
pub async fn get_marketplace_skill_details(
    source: String,
    name: String,
) -> Result<MarketplaceSkillDetails, AppError> {
    debug!(target: "marketplace", source = %source, name = %name, "get_marketplace_skill_details called");
    match remote::fetch_marketplace_skill_details(&source, &name).await {
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
            Err(AppError::Other(e.to_string()))
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
    let resolved = snapshot::resolve_skill_sources_local_first(&names, &existing_sources)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    info!(target: "marketplace", resolved = resolved.len(), total = total, "resolve_skill_sources done");
    Ok(resolved)
}

#[tauri::command]
pub async fn ai_extract_search_keywords(query: String) -> Result<Vec<String>, AppError> {
    debug!(target: "marketplace", query = %query, "ai_extract_search_keywords called");
    let config = ai_provider::load_config_async().await;
    if !config.enabled {
        return Err(AppError::Other(
            "AI provider is disabled. Please enable it in Settings.".to_string(),
        ));
    }
    let resolved = ai_provider::resolve_runtime_config(&config)
        .map_err(|e| AppError::Other(format!("AI config error: {}", e)))?;
    if resolved.api_key.trim().is_empty() && !matches!(resolved.api_format, ApiFormat::Local) {
        return Err(AppError::Other(
            "AI provider is not configured. Please choose a Models provider or local model in Settings.".to_string(),
        ));
    }
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
pub async fn ai_search_with_keywords(
    keywords: Vec<String>,
) -> Result<AiKeywordSearchResult, AppError> {
    debug!(target: "marketplace", count = keywords.len(), keywords = ?keywords, "ai_search_with_keywords called");
    let result = remote::ai_search_by_keywords(&keywords)
        .await
        .map_err(|e| AppError::Other(format!("AI marketplace search failed: {}", e)))?;
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
) -> Result<LocalFirstResult<Vec<Skill>>, AppError> {
    snapshot::get_leaderboard_local(&category)
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

#[tauri::command]
pub async fn list_curated_registries() -> Result<Vec<CuratedRegistryEntry>, AppError> {
    snapshot::list_curated_registries().map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn upsert_curated_registry(
    entry: CuratedRegistryUpsert,
) -> Result<CuratedRegistryEntry, AppError> {
    snapshot::upsert_curated_registry(entry).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_source_observations(
    skill_key: String,
) -> Result<Vec<MarketplaceSourceObservation>, AppError> {
    snapshot::list_source_observations_for_skill(&skill_key)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_known_marketplace_sources() -> Result<Vec<MarketplaceSourceSummary>, AppError> {
    snapshot::list_known_marketplace_sources().map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn upsert_marketplace_category(
    category: MarketplaceCategoryUpsert,
) -> Result<MarketplaceCategory, AppError> {
    snapshot::upsert_category(category).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_categories() -> Result<Vec<MarketplaceCategory>, AppError> {
    snapshot::list_categories().map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn assign_marketplace_skill_categories(
    assignment: MarketplaceSkillCategoryAssignmentInput,
) -> Result<Vec<MarketplaceSkillCategoryAssignment>, AppError> {
    snapshot::assign_categories_to_skill(assignment).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_skill_categories(
    skill_key: String,
) -> Result<Vec<MarketplaceSkillCategoryAssignment>, AppError> {
    snapshot::list_categories_for_skill(&skill_key).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn upsert_marketplace_tag(tag: MarketplaceTagUpsert) -> Result<MarketplaceTag, AppError> {
    snapshot::upsert_tag(tag).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_tags() -> Result<Vec<MarketplaceTag>, AppError> {
    snapshot::list_tags().map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn assign_marketplace_skill_tags(
    assignment: MarketplaceSkillTagAssignmentInput,
) -> Result<Vec<MarketplaceSkillTagAssignment>, AppError> {
    snapshot::assign_tags_to_skill(assignment).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_skill_tags(
    skill_key: String,
) -> Result<Vec<MarketplaceSkillTagAssignment>, AppError> {
    snapshot::list_tags_for_skill(&skill_key).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn upsert_marketplace_rating_summary(
    summary: MarketplaceRatingSummaryUpsert,
) -> Result<MarketplaceRatingSummary, AppError> {
    snapshot::upsert_rating_summary(summary).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_rating_summaries(
    skill_key: String,
) -> Result<Vec<MarketplaceRatingSummary>, AppError> {
    snapshot::list_rating_summaries_for_skill(&skill_key)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn upsert_marketplace_review(
    review: MarketplaceReviewUpsert,
) -> Result<MarketplaceReview, AppError> {
    snapshot::upsert_review(review).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_reviews(
    skill_key: String,
) -> Result<Vec<MarketplaceReview>, AppError> {
    snapshot::list_reviews_for_skill(&skill_key).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn upsert_marketplace_update_notification(
    notification: MarketplaceUpdateNotificationUpsert,
) -> Result<MarketplaceUpdateNotification, AppError> {
    snapshot::upsert_update_notification(notification).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_update_notifications(
    include_dismissed: Option<bool>,
) -> Result<Vec<MarketplaceUpdateNotification>, AppError> {
    snapshot::list_update_notifications(include_dismissed.unwrap_or(false))
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_update_notifications_for_skill(
    skill_key: String,
    include_dismissed: Option<bool>,
) -> Result<Vec<MarketplaceUpdateNotification>, AppError> {
    snapshot::list_update_notifications_for_skill(&skill_key, include_dismissed.unwrap_or(false))
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn dismiss_marketplace_update_notification(
    skill_key: String,
    source_id: String,
) -> Result<bool, AppError> {
    snapshot::dismiss_update_notification(&skill_key, &source_id)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn search_marketplace_packs(
    query: String,
    limit: Option<u32>,
) -> Result<Vec<MarketplacePack>, AppError> {
    snapshot::search_packs_local(&query, limit.unwrap_or(20))
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_marketplace_packs(limit: Option<u32>) -> Result<Vec<MarketplacePack>, AppError> {
    snapshot::list_packs_local(limit.unwrap_or(50)).map_err(|e| AppError::Other(e.to_string()))
}

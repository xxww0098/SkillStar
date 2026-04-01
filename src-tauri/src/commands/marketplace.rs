use crate::core::{
    marketplace, marketplace_snapshot,
    skill::{OfficialPublisher, Skill},
};
use std::collections::HashMap;

#[tauri::command]
pub async fn search_skills_sh(query: String) -> Result<marketplace::MarketplaceResult, String> {
    eprintln!("[search_skills_sh] Called with query: {}", query);
    match marketplace::search_skills_sh(&query, 200).await {
        Ok(result) => {
            eprintln!(
                "[search_skills_sh] Success, got {} skills",
                result.skills.len()
            );
            Ok(result)
        }
        Err(e) => {
            eprintln!("[search_skills_sh] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_skills_sh_leaderboard(category: String) -> Result<Vec<Skill>, String> {
    eprintln!(
        "[get_skills_sh_leaderboard] Called with category: {}",
        category
    );
    match marketplace::get_skills_sh_leaderboard(&category).await {
        Ok(result) => {
            eprintln!(
                "[get_skills_sh_leaderboard] Success, got {} skills",
                result.len()
            );
            Ok(result)
        }
        Err(e) => {
            eprintln!("[get_skills_sh_leaderboard] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_official_publishers() -> Result<Vec<OfficialPublisher>, String> {
    eprintln!("[get_official_publishers] Called");
    match marketplace::get_official_publishers().await {
        Ok(result) => {
            eprintln!(
                "[get_official_publishers] Success, got {} publishers",
                result.len()
            );
            Ok(result)
        }
        Err(e) => {
            eprintln!("[get_official_publishers] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_publisher_repos(
    publisher_name: String,
) -> Result<Vec<marketplace::PublisherRepo>, String> {
    eprintln!(
        "[get_publisher_repos] Called for publisher: {}",
        publisher_name
    );
    match marketplace::get_publisher_repos(&publisher_name).await {
        Ok(repos) => {
            eprintln!("[get_publisher_repos] Success, got {} repos", repos.len());
            Ok(repos)
        }
        Err(e) => {
            eprintln!("[get_publisher_repos] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_publisher_repo_skills(
    publisher_name: String,
    repo_name: String,
) -> Result<Vec<marketplace::PublisherRepoSkill>, String> {
    eprintln!(
        "[get_publisher_repo_skills] Called for {}/{}",
        publisher_name, repo_name
    );
    match marketplace::get_publisher_repo_skills(&publisher_name, &repo_name).await {
        Ok(skills) => {
            eprintln!(
                "[get_publisher_repo_skills] Success, got {} skills",
                skills.len()
            );
            Ok(skills)
        }
        Err(e) => {
            eprintln!("[get_publisher_repo_skills] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_marketplace_skill_details(
    source: String,
    name: String,
) -> Result<marketplace::MarketplaceSkillDetails, String> {
    eprintln!(
        "[get_marketplace_skill_details] Called for {}/{}",
        source, name
    );
    match marketplace::fetch_marketplace_skill_details(&source, &name).await {
        Ok(details) => {
            eprintln!(
                "[get_marketplace_skill_details] Success — summary: {}, readme: {}, audits: {}",
                details.summary.is_some(),
                details.readme.is_some(),
                details.security_audits.len()
            );
            Ok(details)
        }
        Err(e) => {
            eprintln!("[get_marketplace_skill_details] Error: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn resolve_skill_sources(
    names: Vec<String>,
    existing_sources: HashMap<String, String>,
) -> Result<HashMap<String, String>, String> {
    eprintln!(
        "[resolve_skill_sources] Resolving {} skill name(s)",
        names.len()
    );
    let total = names.len();
    let resolved =
        marketplace_snapshot::resolve_skill_sources_local_first(&names, &existing_sources)
            .await
            .map_err(|e| e.to_string())?;
    eprintln!(
        "[resolve_skill_sources] Resolved {}/{} sources",
        resolved.len(),
        total
    );
    Ok(resolved)
}

#[tauri::command]
pub async fn ai_extract_search_keywords(query: String) -> Result<Vec<String>, String> {
    eprintln!("[ai_extract_search_keywords] Query: {}", query);
    let config = crate::commands::ai::ensure_ai_config_pub()
        .await
        .map_err(|e| {
            eprintln!("[ai_extract_search_keywords] Config error: {}", e);
            e
        })?;
    let keywords = crate::core::ai_provider::extract_search_keywords(&config, &query)
        .await
        .map_err(|e| {
            let msg = format!("AI keyword extraction failed: {}", e);
            eprintln!("[ai_extract_search_keywords] {}", msg);
            msg
        })?;
    eprintln!(
        "[ai_extract_search_keywords] ✓ Extracted {} keywords: {:?}",
        keywords.len(),
        keywords
    );
    Ok(keywords)
}

#[tauri::command]
pub async fn ai_search_with_keywords(
    keywords: Vec<String>,
) -> Result<marketplace::AiKeywordSearchResult, String> {
    eprintln!(
        "[ai_search_with_keywords] Searching {} keywords: {:?}",
        keywords.len(),
        keywords
    );
    let result = marketplace::ai_search_by_keywords(&keywords)
        .await
        .map_err(|e| {
            let msg = format!("AI marketplace search failed: {}", e);
            eprintln!("[ai_search_with_keywords] {}", msg);
            msg
        })?;
    eprintln!(
        "[ai_search_with_keywords] ✓ Found {} skills, keyword_map keys: {:?}",
        result.skills.len(),
        result.keyword_skill_map.keys().collect::<Vec<_>>()
    );
    Ok(result)
}

#[tauri::command]
pub async fn get_leaderboard_local(
    category: String,
) -> Result<marketplace_snapshot::LocalFirstResult<Vec<Skill>>, String> {
    marketplace_snapshot::get_leaderboard_local(&category)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_marketplace_local(
    query: String,
    limit: Option<u32>,
) -> Result<marketplace_snapshot::LocalFirstResult<Vec<Skill>>, String> {
    marketplace_snapshot::search_local(&query, limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_publishers_local()
-> Result<marketplace_snapshot::LocalFirstResult<Vec<OfficialPublisher>>, String> {
    marketplace_snapshot::get_publishers_local()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_publisher_repos_local(
    publisher_name: String,
) -> Result<marketplace_snapshot::LocalFirstResult<Vec<marketplace::PublisherRepo>>, String> {
    marketplace_snapshot::get_publisher_repos_local(&publisher_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_repo_skills_local(
    source: String,
) -> Result<marketplace_snapshot::LocalFirstResult<Vec<Skill>>, String> {
    marketplace_snapshot::get_repo_skills_local(&source)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_skill_detail_local(
    source: String,
    name: String,
) -> Result<marketplace_snapshot::LocalFirstResult<marketplace::MarketplaceSkillDetails>, String> {
    marketplace_snapshot::get_skill_detail_local(&source, &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_search_marketplace_local(
    keywords: Vec<String>,
    limit: Option<u32>,
) -> Result<marketplace_snapshot::LocalFirstResult<marketplace::AiKeywordSearchResult>, String> {
    marketplace_snapshot::ai_search_local(&keywords, limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_marketplace_scope(scope: String) -> Result<(), String> {
    marketplace_snapshot::sync_marketplace_scope(&scope)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_marketplace_sync_states()
-> Result<Vec<marketplace_snapshot::SyncStateEntry>, String> {
    marketplace_snapshot::get_marketplace_sync_states().map_err(|e| e.to_string())
}

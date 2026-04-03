pub use skillstar_marketplace_core::{
    AiKeywordSearchResult, MarketplaceResult, MarketplaceSkillDetails, PublisherRepo,
    PublisherRepoSkill,
};

pub use skillstar_marketplace_core::remote::{
    ai_search_by_keywords, fetch_marketplace_skill_details, get_official_publishers,
    get_publisher_repo_skills, get_publisher_repos, get_skills_sh_leaderboard, search_skills_sh,
};

pub use super::marketplace_snapshot::{LocalFirstResult, MarketplacePack, SyncStateEntry};

pub fn initialize_local_snapshot() -> anyhow::Result<()> {
    super::marketplace_snapshot::initialize()
}

pub async fn refresh_local_snapshot_startup_scopes() -> anyhow::Result<()> {
    super::marketplace_snapshot::refresh_startup_scopes_if_needed().await
}

pub async fn resolve_skill_sources_local_first(
    names: &[String],
    existing_sources: &std::collections::HashMap<String, String>,
) -> anyhow::Result<std::collections::HashMap<String, String>> {
    super::marketplace_snapshot::resolve_skill_sources_local_first(names, existing_sources).await
}

pub async fn get_leaderboard_local(
    category: &str,
) -> anyhow::Result<LocalFirstResult<Vec<super::skill::Skill>>> {
    super::marketplace_snapshot::get_leaderboard_local(category).await
}

pub async fn search_marketplace_local(
    query: &str,
    limit: Option<u32>,
) -> anyhow::Result<LocalFirstResult<Vec<super::skill::Skill>>> {
    super::marketplace_snapshot::search_local(query, limit).await
}

pub async fn get_publishers_local()
-> anyhow::Result<LocalFirstResult<Vec<super::skill::OfficialPublisher>>> {
    super::marketplace_snapshot::get_publishers_local().await
}

pub async fn get_publisher_repos_local(
    publisher_name: &str,
) -> anyhow::Result<LocalFirstResult<Vec<PublisherRepo>>> {
    super::marketplace_snapshot::get_publisher_repos_local(publisher_name).await
}

pub async fn get_repo_skills_local(
    source: &str,
) -> anyhow::Result<LocalFirstResult<Vec<super::skill::Skill>>> {
    super::marketplace_snapshot::get_repo_skills_local(source).await
}

pub async fn get_skill_detail_local(
    source: &str,
    name: &str,
) -> anyhow::Result<LocalFirstResult<MarketplaceSkillDetails>> {
    super::marketplace_snapshot::get_skill_detail_local(source, name).await
}

pub async fn ai_search_marketplace_local(
    keywords: &[String],
    limit: Option<u32>,
) -> anyhow::Result<LocalFirstResult<AiKeywordSearchResult>> {
    super::marketplace_snapshot::ai_search_local(keywords, limit).await
}

pub async fn sync_marketplace_scope_local(scope: &str) -> anyhow::Result<()> {
    super::marketplace_snapshot::sync_marketplace_scope(scope).await
}

pub fn get_marketplace_local_sync_states() -> anyhow::Result<Vec<SyncStateEntry>> {
    super::marketplace_snapshot::get_marketplace_sync_states()
}

pub fn search_marketplace_packs_local(
    query: &str,
    limit: u32,
) -> anyhow::Result<Vec<MarketplacePack>> {
    super::marketplace_snapshot::search_packs_local(query, limit)
}

pub fn list_marketplace_packs_local(limit: u32) -> anyhow::Result<Vec<MarketplacePack>> {
    super::marketplace_snapshot::list_packs_local(limit)
}

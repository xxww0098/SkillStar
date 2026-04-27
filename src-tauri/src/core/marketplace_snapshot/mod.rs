use std::collections::HashMap;

use anyhow::Result;
use skillstar_marketplace_core::snapshot::{self as core_snapshot, InstalledSkillsFuture};

use super::skill::{OfficialPublisher, Skill};

pub use skillstar_marketplace_core::{LocalFirstResult, MarketplacePack, SyncStateEntry};
pub use skillstar_marketplace_core::{MarketplaceSourceObservation, MarketplaceSourceSummary};

fn runtime_config() -> skillstar_marketplace_core::snapshot::SnapshotRuntimeConfig {
    skillstar_marketplace_core::snapshot::SnapshotRuntimeConfig::new(
        crate::core::infra::paths::marketplace_db_path(),
        crate::core::infra::paths::data_root(),
        super::installed_skill::installed_snapshot_markers,
        || -> InstalledSkillsFuture {
            Box::pin(super::installed_skill::list_installed_skills_fast())
        },
    )
}

fn configure_runtime() {
    core_snapshot::configure_runtime(runtime_config());
}

pub fn initialize() -> Result<()> {
    configure_runtime();
    core_snapshot::initialize()
}

pub async fn refresh_startup_scopes_if_needed() -> Result<()> {
    configure_runtime();
    core_snapshot::refresh_startup_scopes_if_needed().await
}

pub async fn resolve_skill_sources_local_first(
    names: &[String],
    existing_sources: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    configure_runtime();
    core_snapshot::resolve_skill_sources_local_first(names, existing_sources).await
}

pub async fn get_leaderboard_local(category: &str) -> Result<LocalFirstResult<Vec<Skill>>> {
    configure_runtime();
    core_snapshot::get_leaderboard_local(category).await
}

pub async fn search_local(query: &str, limit: Option<u32>) -> Result<LocalFirstResult<Vec<Skill>>> {
    configure_runtime();
    core_snapshot::search_local(query, limit).await
}

pub async fn get_publishers_local() -> Result<LocalFirstResult<Vec<OfficialPublisher>>> {
    configure_runtime();
    core_snapshot::get_publishers_local().await
}

pub async fn get_publisher_repos_local(
    publisher_name: &str,
) -> Result<LocalFirstResult<Vec<skillstar_marketplace_core::PublisherRepo>>> {
    configure_runtime();
    core_snapshot::get_publisher_repos_local(publisher_name).await
}

pub async fn get_repo_skills_local(source: &str) -> Result<LocalFirstResult<Vec<Skill>>> {
    configure_runtime();
    core_snapshot::get_repo_skills_local(source).await
}

pub async fn get_skill_detail_local(
    source: &str,
    name: &str,
) -> Result<LocalFirstResult<skillstar_marketplace_core::MarketplaceSkillDetails>> {
    configure_runtime();
    core_snapshot::get_skill_detail_local(source, name).await
}

pub async fn ai_search_local(
    keywords: &[String],
    limit: Option<u32>,
) -> Result<LocalFirstResult<skillstar_marketplace_core::AiKeywordSearchResult>> {
    configure_runtime();
    core_snapshot::ai_search_local(keywords, limit).await
}

pub async fn sync_marketplace_scope(scope: &str) -> Result<()> {
    configure_runtime();
    core_snapshot::sync_marketplace_scope(scope).await
}

pub fn get_marketplace_sync_states() -> Result<Vec<SyncStateEntry>> {
    configure_runtime();
    core_snapshot::get_marketplace_sync_states()
}

pub fn search_packs_local(query: &str, limit: u32) -> Result<Vec<MarketplacePack>> {
    configure_runtime();
    core_snapshot::search_packs_local(query, limit)
}

pub fn list_packs_local(limit: u32) -> Result<Vec<MarketplacePack>> {
    configure_runtime();
    core_snapshot::list_packs_local(limit)
}

pub fn list_source_observations_for_skill(
    skill_key: &str,
) -> Result<Vec<MarketplaceSourceObservation>> {
    configure_runtime();
    core_snapshot::list_source_observations_for_skill(skill_key)
}

pub fn list_known_marketplace_sources() -> Result<Vec<MarketplaceSourceSummary>> {
    configure_runtime();
    core_snapshot::list_known_marketplace_sources()
}

#[allow(dead_code)]
pub fn upsert_pack(
    pack_key: &str,
    source: &str,
    name: &str,
    description: &str,
    author: Option<&str>,
    git_url: &str,
    skill_keys: &[(String, String)],
) -> Result<()> {
    configure_runtime();
    core_snapshot::upsert_pack(
        pack_key,
        source,
        name,
        description,
        author,
        git_url,
        skill_keys,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_config_wires_marketplace_paths() {
        let _guard = crate::core::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let config = runtime_config();
        assert_eq!(
            config.db_path,
            crate::core::infra::paths::marketplace_db_path()
        );
        assert_eq!(config.data_root, crate::core::infra::paths::data_root());

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}

use std::collections::HashMap;

use anyhow::Result;
use skillstar_marketplace::snapshot::{self as core_snapshot, InstalledSkillsFuture};

fn runtime_config() -> skillstar_marketplace::snapshot::SnapshotRuntimeConfig {
    skillstar_marketplace::snapshot::SnapshotRuntimeConfig::new(
        skillstar_core::infra::paths::marketplace_db_path(),
        skillstar_core::infra::paths::data_root(),
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
            skillstar_core::infra::paths::marketplace_db_path()
        );
        assert_eq!(config.data_root, skillstar_core::infra::paths::data_root());

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}

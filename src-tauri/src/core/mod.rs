pub mod agent_profile;
pub mod ai_provider;
pub mod gh_manager;
pub mod git_ops;
pub mod installed_skill;
pub mod local_skill;
pub mod lockfile;
pub mod marketplace;
pub mod marketplace_snapshot;
pub mod path_env;
pub mod paths;
pub mod patrol;
pub mod project_manifest;
pub mod proxy;
pub mod repo_history;
pub mod repo_scanner;
pub mod security_scan;
pub mod skill;
pub mod skill_bundle;
pub mod skill_group;
pub mod skill_install;
pub mod sync;
pub mod translation_cache;

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

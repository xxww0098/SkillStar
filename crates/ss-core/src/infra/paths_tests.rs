use super::{
    agents_skills_root, app_instances_config_path, data_root, github_auth_path, home_dir, hub_root,
    instances_dir, legacy_local_skills_dir, local_skills_dir, marketplace_db_path, mcp_backups_dir,
    mcp_data_dir, mcp_library_path, mcp_lock_path, patrol_state_path, sessions_index_path,
    shellexpand_home, skill_lock_path, skill_update_lock_path, skills_backups_dir,
    skills_library_path, ssh_credentials_path, usage_refresh_locks_dir, usage_storage_lock_path,
    usage_store_dir,
};
use tempfile::TempDir;

#[test]
fn data_root_honors_override() {
    let _env_lock = crate::config::test_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let temp = TempDir::new().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
    }
    assert_eq!(data_root(), temp.path());
    assert_eq!(instances_dir(), temp.path().join("data/instances"));
    assert_eq!(
        app_instances_config_path(),
        temp.path().join("data/instances/app_instances.json")
    );
    assert_eq!(
        crate::infra::paths::team_store_path(),
        temp.path().join("state/team.json")
    );
    unsafe {
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

#[test]
fn hub_root_defaults_under_data_root() {
    let _env_lock = crate::config::test_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let temp = TempDir::new().unwrap();
    unsafe {
        std::env::remove_var("SKILLSTAR_HUB_DIR");
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
    }
    assert_eq!(hub_root(), temp.path().join("hub"));
    unsafe {
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

#[test]
fn hub_root_honors_override() {
    let _env_lock = crate::config::test_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let temp = TempDir::new().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_HUB_DIR", temp.path());
    }
    assert_eq!(hub_root(), temp.path());
    unsafe {
        std::env::remove_var("SKILLSTAR_HUB_DIR");
    }
}

#[test]
fn v3_layout_categories_resolve_under_data_root() {
    let _env_lock = crate::config::test_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let temp = TempDir::new().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        std::env::remove_var("SKILLSTAR_HUB_DIR");
    }
    assert_eq!(
        marketplace_db_path(),
        temp.path().join("cache/marketplace/marketplace.db")
    );
    assert_eq!(
        sessions_index_path(),
        temp.path().join("cache/sessions/index.json")
    );
    assert_eq!(
        github_auth_path(),
        temp.path().join("secrets/github/auth.json")
    );
    assert_eq!(
        ssh_credentials_path(),
        temp.path().join("secrets/ssh/credentials.json")
    );
    assert_eq!(
        usage_store_dir(),
        temp.path().join("secrets/accounts/usage")
    );
    assert_eq!(
        usage_storage_lock_path(),
        temp.path().join("runtime/locks/accounts/storage.lock")
    );
    assert_eq!(
        usage_refresh_locks_dir(),
        temp.path().join("runtime/locks/accounts")
    );
    assert_eq!(
        skill_update_lock_path(),
        temp.path().join("runtime/locks/skills/update.lock")
    );
    assert_eq!(
        patrol_state_path(),
        temp.path().join("state/patrol/status.json")
    );
    assert_eq!(
        legacy_local_skills_dir(),
        temp.path().join("data/skills/local")
    );
    assert_eq!(
        local_skills_dir(),
        temp.path().join("data/skills/installed")
    );
    assert_eq!(
        agents_skills_root(),
        temp.path().join("data/skills/installed")
    );
    assert_eq!(
        skills_library_path(),
        temp.path().join("data/skills/library.json")
    );
    assert_eq!(
        skills_backups_dir(),
        temp.path().join("data/skills/backups")
    );
    assert_eq!(
        skill_lock_path(),
        temp.path().join("data/skills/.skill-lock.json")
    );
    assert_eq!(
        crate::infra::paths::repo_favorites_path(),
        temp.path().join("data/skills/favorites.json")
    );
    assert_eq!(
        crate::infra::paths::groups_path(),
        temp.path().join("data/skills/groups.json")
    );
    assert_eq!(
        crate::infra::paths::repo_history_path(),
        temp.path().join("data/skills/repo_history.json")
    );
    assert_eq!(
        crate::infra::paths::projects_manifest_path(),
        temp.path().join("data/projects/projects.json")
    );
    assert_eq!(
        crate::infra::paths::shared_channels_registry_path(),
        temp.path().join("data/channels/shared_channels.json")
    );
    assert_eq!(
        crate::infra::paths::channel_subscriptions_path(),
        temp.path().join("data/channels/subscriptions.json")
    );
    assert_eq!(
        crate::infra::paths::channels_locks_dir(),
        temp.path().join("runtime/locks/channels")
    );
    assert_eq!(
        crate::infra::paths::avatars_cache_dir(),
        temp.path().join("cache/avatars")
    );
    assert_eq!(
        crate::infra::paths::github_trees_cache_dir(),
        temp.path().join("cache/github-trees")
    );
    assert_eq!(
        crate::infra::paths::skill_updates_state_path(),
        temp.path().join("state/skill_updates.json")
    );
    assert_eq!(mcp_data_dir(), temp.path().join("data/mcp"));
    assert_eq!(mcp_library_path(), temp.path().join("data/mcp/library.json"));
    assert_eq!(mcp_backups_dir(), temp.path().join("data/mcp/backups"));
    assert_eq!(
        mcp_lock_path(),
        temp.path().join("runtime/locks/mcp/library.lock")
    );
    unsafe {
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

#[test]
fn local_skills_dir_stays_in_hub_sandbox() {
    let _env_lock = crate::config::test_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let temp = TempDir::new().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_HUB_DIR", temp.path());
    }
    assert_eq!(legacy_local_skills_dir(), temp.path().join("local"));
    assert_eq!(local_skills_dir(), temp.path().join("skills"));
    assert_eq!(skills_library_path(), temp.path().join("library.json"));
    assert_eq!(skills_backups_dir(), temp.path().join("backups"));
    unsafe {
        std::env::remove_var("SKILLSTAR_HUB_DIR");
    }
}

#[test]
fn shellexpand_home_expands_tilde() {
    let home = home_dir();
    assert_eq!(
        shellexpand_home("~/foo"),
        home.join("foo").to_string_lossy()
    );
    assert_eq!(
        shellexpand_home("~\\foo"),
        home.join("foo").to_string_lossy()
    );
    assert_eq!(shellexpand_home("/absolute/path"), "/absolute/path");
    assert_eq!(shellexpand_home("relative/path"), "relative/path");
}

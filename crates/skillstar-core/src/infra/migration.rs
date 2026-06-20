//! Legacy path migration (v1 flat layout → v2 categorised layout).

use std::path::Path;
use std::sync::OnceLock;

use super::paths;

static MIGRATION_DONE: OnceLock<()> = OnceLock::new();

pub fn migrate_legacy_paths() {
    if MIGRATION_DONE.get().is_some() {
        return;
    }

    let root = paths::data_root();

    let _ = std::fs::create_dir_all(paths::config_dir());
    let _ = std::fs::create_dir_all(paths::db_dir());
    let _ = std::fs::create_dir_all(paths::logs_dir());
    let _ = std::fs::create_dir_all(paths::state_dir());
    let _ = std::fs::create_dir_all(paths::hub_root());

    migrate_file(&root.join("ai_config.json"), &paths::ai_config_path());
    migrate_file(&root.join("acp_config.json"), &paths::acp_config_path());
    migrate_file(&root.join("proxy.json"), &paths::proxy_config_path());
    migrate_file(&root.join("profiles.toml"), &paths::profiles_config_path());

    migrate_file(&root.join("marketplace.db"), &paths::marketplace_db_path());
    migrate_file(
        &root.join("marketplace.db-wal"),
        &paths::db_dir().join("marketplace.db-wal"),
    );
    migrate_file(
        &root.join("marketplace.db-shm"),
        &paths::db_dir().join("marketplace.db-shm"),
    );
    migrate_file(&root.join("patrol.json"), &paths::patrol_state_path());
    migrate_file(
        &root.join("projects.json"),
        &paths::projects_manifest_path(),
    );
    migrate_dir(&root.join("projects"), &paths::state_dir().join("projects"));
    migrate_file(&root.join("groups.json"), &paths::groups_path());
    migrate_file(&root.join("packs.json"), &paths::packs_path());
    migrate_file(&root.join("repo_history.json"), &paths::repo_history_path());
    let legacy_hub = root.join(".agents");
    if legacy_hub.is_dir() {
        migrate_dir(&legacy_hub.join("skills"), &paths::hub_skills_dir());
        migrate_dir(&legacy_hub.join("skills-local"), &paths::local_skills_dir());
        migrate_dir(&legacy_hub.join(".repos"), &paths::repos_cache_dir());
        migrate_dir(
            &legacy_hub.join(".publish-repos"),
            &paths::hub_root().join("publish"),
        );
        migrate_file(
            &legacy_hub.join(".skill-lock.json"),
            &paths::lockfile_path(),
        );

        let _ = remove_dir_if_empty(&legacy_hub);
    }

    let _ = std::fs::remove_file(root.join("security_scan_cache.json"));

    let _ = MIGRATION_DONE.set(());

    tracing::info!("Storage path migration check completed");
}

fn migrate_file(old: &Path, new: &Path) {
    if old.exists() && !new.exists() {
        if let Some(parent) = new.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::rename(old, new) {
            Ok(()) => tracing::info!("Migrated {:?} → {:?}", old, new),
            Err(e) => tracing::warn!("Failed to migrate {:?} → {:?}: {}", old, new, e),
        }
    }
}

fn migrate_dir(old: &Path, new: &Path) {
    if old.is_dir() && !new.exists() {
        if let Some(parent) = new.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::rename(old, new) {
            Ok(()) => tracing::info!("Migrated dir {:?} → {:?}", old, new),
            Err(e) => tracing::warn!("Failed to migrate dir {:?} → {:?}: {}", old, new, e),
        }
    }
}

fn remove_dir_if_empty(dir: &Path) -> std::io::Result<()> {
    if let Ok(mut entries) = std::fs::read_dir(dir)
        && entries.next().is_none()
    {
        std::fs::remove_dir(dir)?;
        tracing::info!("Removed empty legacy dir {:?}", dir);
    }
    Ok(())
}

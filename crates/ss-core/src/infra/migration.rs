//! Storage path migration (v1 flat layout → v2 categorised → v3
//! purpose-based layout, D-087). The mapping table and classification rules
//! live in `docs/storage-layout.md` (the layout SSOT).

use std::path::Path;
use std::sync::OnceLock;

use super::paths;

static MIGRATION_DONE: OnceLock<()> = OnceLock::new();

pub fn migrate_legacy_paths() {
    MIGRATION_DONE.get_or_init(migrate_paths);
}

fn migrate_paths() {
    let root = paths::data_root();

    let _ = std::fs::create_dir_all(paths::config_dir());
    let _ = std::fs::create_dir_all(paths::logs_dir());
    let _ = std::fs::create_dir_all(paths::state_dir());
    create_private_dir(&paths::secrets_dir());
    let _ = std::fs::create_dir_all(paths::cache_dir());
    let _ = std::fs::create_dir_all(paths::runtime_locks_dir());

    // v1 flat-root files → their (already re-pointed) v3 homes.
    migrate_file(&root.join("ai_config.json"), &paths::ai_config_path());
    migrate_file(&root.join("proxy.json"), &paths::proxy_config_path());
    migrate_file(&root.join("profiles.toml"), &paths::profiles_config_path());

    // SQLite main/WAL/SHM cannot be moved independently. Leave legacy
    // marketplace caches intact and rebuild at the new path on refresh.
    migrate_file(&root.join("patrol.json"), &paths::patrol_state_path());
    migrate_file(
        &root.join("projects.json"),
        &paths::projects_manifest_path(),
    );
    migrate_dir(&root.join("projects"), &paths::state_dir().join("projects"));
    migrate_file(&root.join("groups.json"), &paths::groups_path());
    migrate_file(&root.join("repo_history.json"), &paths::repo_history_path());

    // v2 → v3 re-homing (D-087; see the mapping table in storage-layout.md).
    // Credentials → secrets/ (modes survive the rename; the directory is 0700).
    migrate_file(
        &root.join("state/github_auth.json"),
        &paths::github_auth_path(),
    );
    migrate_file(
        &root.join("state/ssh_credentials.json"),
        &paths::ssh_credentials_path(),
    );
    // Usage store (encrypted tokens inside) → secrets/; its locks → runtime/.
    migrate_file(
        &root.join("config/usage/.storage.lock"),
        &paths::usage_storage_lock_path(),
    );
    // Per-entry move (not a dir rename): the storage-lock step above may have
    // already created the target directory.
    migrate_dir_contents(
        &root.join("config/usage/locks"),
        &paths::usage_refresh_locks_dir(),
        &[],
    );
    migrate_dir_contents(
        &root.join("config/usage"),
        &paths::usage_store_dir(),
        &[".storage.lock", "locks"],
    );
    // External CLI links still point here. Moving custody requires a domain
    // transaction that also repairs those links; keep existing catalogs live.
    if root.join("accounts").is_dir() {
        create_private_dir(&root.join("accounts"));
    }
    // Rebuildable caches → cache/.
    migrate_file(
        &root.join("sessions/index.json"),
        &paths::sessions_index_path(),
    );
    let _ = remove_dir_if_empty(&root.join("sessions"));
    // State file re-homed one level down; its lock → runtime/.
    migrate_file(&root.join("state/patrol.json"), &paths::patrol_state_path());
    migrate_file(
        &root.join("state/skill-update.lock"),
        &paths::skill_update_lock_path(),
    );
    // Multi-instance profiles & manifest → data/.
    migrate_dir(&root.join("instances"), &paths::instances_dir());
    migrate_file(
        &root.join("config/app_instances.json"),
        &paths::app_instances_config_path(),
    );
    migrate_v1_hub(&root);

    let _ = std::fs::remove_file(root.join("security_scan_cache.json"));

    tracing::info!("Storage path migration check completed");
}

/// v1 kept its hub under `<data>/.agents`. Everything there is SkillStar's
/// own pre-D-081 state, so its skills move to the legacy hub — where
/// `legacy_cleanup` turns them into reinstalls — never into the canonical
/// `~/.skillstar/data/skills/installed`.
fn migrate_v1_hub(root: &Path) {
    let legacy_hub = root.join(".agents");
    if legacy_hub.is_dir() {
        migrate_dir(
            &legacy_hub.join("skills"),
            &paths::legacy_hub_root().join("skills"),
        );
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

/// Move every entry of `old` into `new` (per entry, target wins), then
/// drop `old` when empty. Entries named in `skip` stay behind.
fn migrate_dir_contents(old: &Path, new: &Path, skip: &[&str]) {
    let Ok(entries) = std::fs::read_dir(old) else {
        return;
    };
    let _ = std::fs::create_dir_all(new);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if skip.contains(&name) {
            continue;
        }
        let destination = new.join(name);
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            migrate_dir(&entry.path(), &destination);
        } else {
            migrate_file(&entry.path(), &destination);
        }
    }
    let _ = remove_dir_if_empty(old);
}

/// Create `dir` (and parents) and tighten it to 0700 on Unix — used for the
/// secrets category root. Best effort: failures are ignored like the rest of
/// the migration prelude.
fn create_private_dir(dir: &Path) {
    if std::fs::create_dir_all(dir).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn migrates_v2_layout_to_v3_and_is_idempotent() {
        let _env_lock = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", root);
            std::env::remove_var("SKILLSTAR_HUB_DIR");
        }

        write(&root.join("config/usage/subscriptions.json"), "{}");
        write(&root.join("config/usage/locks/catalog-cursor.lock"), "");
        write(&root.join("config/usage/.storage.lock"), "");
        write(&root.join("accounts/codex/external-1.json"), "{}");
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            root.join("accounts/codex/external-1.json"),
            root.join("cli-auth.json"),
        )
        .unwrap();
        write(&root.join("state/github_auth.json"), "{}");
        write(&root.join("state/ssh_credentials.json"), "{}");
        write(&root.join("state/patrol.json"), "{}");
        write(&root.join("state/skill-update.lock"), "");
        for base in [root.to_path_buf(), root.join("db")] {
            for name in ["marketplace.db", "marketplace.db-wal", "marketplace.db-shm"] {
                write(&base.join(name), "legacy-cache");
            }
        }
        write(&root.join("sessions/index.json"), "{}");
        write(&root.join("instances/cursor/work/.marker"), "");
        write(&root.join("config/app_instances.json"), "{}");

        migrate_paths();
        assert!(!paths::marketplace_db_path().exists());
        write(&paths::marketplace_db_path(), "current-cache");
        migrate_paths();

        // secrets/
        assert!(
            root.join("secrets/accounts/usage/subscriptions.json")
                .exists()
        );
        assert!(root.join("accounts/codex/external-1.json").exists());
        assert!(root.join("secrets/github/auth.json").exists());
        assert!(root.join("secrets/ssh/credentials.json").exists());
        // runtime/locks/
        assert!(
            root.join("runtime/locks/accounts/catalog-cursor.lock")
                .exists()
        );
        assert!(root.join("runtime/locks/accounts/storage.lock").exists());
        assert!(root.join("runtime/locks/skills/update.lock").exists());
        // cache/
        assert!(root.join("cache/marketplace/marketplace.db").exists());
        assert!(root.join("cache/sessions/index.json").exists());
        // state/ + data/
        assert!(root.join("state/patrol/status.json").exists());
        assert!(root.join("data/instances/cursor/work/.marker").exists());
        assert!(root.join("data/instances/app_instances.json").exists());

        // Sources are gone once fully migrated.
        assert!(!root.join("config/usage/subscriptions.json").exists());
        assert!(!root.join("config/usage/locks/catalog-cursor.lock").exists());
        assert!(!root.join("secrets/accounts/cli/codex").exists());
        #[cfg(unix)]
        {
            assert_eq!(
                std::fs::read_to_string(root.join("cli-auth.json")).unwrap(),
                "{}"
            );
            std::fs::write(root.join("cli-auth.json"), "rotated").unwrap();
            assert_eq!(
                std::fs::read_to_string(paths::cli_custody_dir("codex").join("external-1.json"))
                    .unwrap(),
                "rotated"
            );
        }
        for base in [root.to_path_buf(), root.join("db")] {
            for name in ["marketplace.db", "marketplace.db-wal", "marketplace.db-shm"] {
                assert_eq!(
                    std::fs::read_to_string(base.join(name)).unwrap(),
                    "legacy-cache"
                );
            }
        }
        assert_eq!(
            std::fs::read_to_string(paths::marketplace_db_path()).unwrap(),
            "current-cache"
        );
        assert!(!root.join("cache/marketplace/marketplace.db-wal").exists());
        assert!(!root.join("cache/marketplace/marketplace.db-shm").exists());
        assert!(!root.join("sessions/index.json").exists());
        assert!(!root.join("state/patrol.json").exists());
        assert!(!root.join("instances/cursor/work/.marker").exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = root
                .join("secrets")
                .metadata()
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700, "secrets/ must be private");
            assert_eq!(
                root.join("accounts")
                    .metadata()
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700,
                "legacy custody must remain private"
            );
        }

        // Exercise the migration again, not the process-local OnceLock guard.
        migrate_paths();
        assert!(
            root.join("secrets/accounts/usage/subscriptions.json")
                .exists()
        );
        assert!(root.join("cache/marketplace/marketplace.db").exists());

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn v1_hub_skills_land_in_the_legacy_hub_not_the_canonical_root() {
        let _env_lock = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("data");
        let previous_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", &root);
            std::env::remove_var("SKILLSTAR_HUB_DIR");
            std::env::set_var("HOME", temp.path().join("home"));
        }
        write(
            &root.join(".agents/skills/alpha/SKILL.md"),
            "---\nname: alpha\n---\n",
        );

        migrate_v1_hub(&root);
        migrate_v1_hub(&root);

        assert!(root.join("hub/skills/alpha/SKILL.md").is_file());
        assert!(!paths::hub_skills_dir().join("alpha").exists());
        assert!(!root.join(".agents").exists());

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
            match previous_home {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
        }
    }
}

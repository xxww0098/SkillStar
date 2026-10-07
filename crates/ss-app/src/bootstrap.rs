//! Startup shared by the CLI and the GPUI shell.
//!
//! skillstar mcp serve must not call this. Marketplace snapshot init opens
//! SQLite and is not part of the stdio handshake.

use ss_marketplace::snapshot::{self as snapshot, InstalledSkillsFuture};

/// Migrations, one-time legacy cleanup, and marketplace snapshot wiring.
/// Failures in snapshot init are logged; they must not block CLI or GUI startup.
pub fn prepare_process() {
    ss_core::infra::migration::migrate_legacy_paths();
    ss_skills::storage_migration::migrate_local_skills();
    ss_skills::storage_migration::migrate_installed_skills();
    ss_skills::legacy_cleanup::run_once();
    // Repair, not a read: Skill listing no longer writes links on every call.
    ss_skills::local_skill::reconcile_hub_symlinks();
    if let Err(err) = init_marketplace_snapshot() {
        tracing::error!(target: "marketplace_snapshot", "init failed: {err}");
    }
}

fn runtime_config() -> snapshot::SnapshotRuntimeConfig {
    snapshot::SnapshotRuntimeConfig::new(
        ss_core::infra::paths::marketplace_db_path(),
        ss_core::infra::paths::data_root(),
        ss_skills::installed_skill::installed_snapshot_markers,
        || -> InstalledSkillsFuture {
            Box::pin(ss_skills::installed_skill::list_installed_skills())
        },
    )
}

pub fn init_marketplace_snapshot() -> anyhow::Result<()> {
    snapshot::configure_runtime(runtime_config());
    snapshot::initialize()
}

pub async fn refresh_marketplace_startup() -> anyhow::Result<()> {
    snapshot::configure_runtime(runtime_config());
    snapshot::refresh_startup_scopes_if_needed().await
}

/// Background work that exists only while the GUI process is alive.
pub fn spawn_gui_background(handle: &tokio::runtime::Handle) {
    handle.spawn(async {
        if let Err(err) = refresh_marketplace_startup().await {
            tracing::error!(target: "marketplace_snapshot", "startup refresh failed: {err}");
        }
    });
    crate::channel_wake::spawn(handle);
    crate::skill_wake::spawn(handle);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_config_wires_marketplace_paths() {
        let temp = tempfile::tempdir().unwrap();
        let prev = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let config = runtime_config();
        assert_eq!(config.db_path, ss_core::infra::paths::marketplace_db_path());
        assert_eq!(config.data_root, ss_core::infra::paths::data_root());

        unsafe {
            match prev {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
    }
}

//! Reading and writing the v4 provider store, plus the safety envelope around
//! the migration into it.
//!
//! ## Why a corrupted store is now an error
//!
//! v3's reader answered every failure — unreadable file, malformed JSON — with
//! an empty store, so that "the application can always start with a valid
//! state". The cost of that was invisible and severe: a store that failed to
//! parse looked exactly like a first run, and the very next write replaced the
//! user's providers and bindings with the empty store that had been handed out.
//! One transient read error was enough to destroy a configuration.
//!
//! v4 returns [`StoreError::Corrupted`] instead and leaves the file untouched.
//! The command layer turns it into a state the user can act on — open the file,
//! restore a backup, or reset — because which of those is right depends on
//! facts only the user has.
//!
//! ## Why migration aborts when the backup fails
//!
//! v3 logged a warning and migrated anyway. A migration without a backup is an
//! irreversible rewrite of the file that holds the user's credentials, and the
//! situation where the backup fails (no disk space, no permission) is precisely
//! the situation where the write is most likely to go wrong too. Migrating here
//! only ever happens on top of two backups: a rolling one and a permanent
//! `model_providers.v3.json` that survives backup pruning so the "undo" button
//! in the migration report can still work months later.

use super::binding::{ProvidersStoreV4, STORE_VERSION_V4};
use super::migrate::{ExtractedCatalog, MigrationOutcome, MigrationReport, migrate_v3_to_v4};
use super::presets::get_all_presets_flat;
use super::store::migrate_store_if_needed;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::warn;

/// Why the store could not be read.
///
/// Split from a generic error because the caller must treat "no file yet" and
/// "the file is there but unreadable" differently, and a stringly-typed error
/// makes that distinction a substring match.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The file exists but could not be parsed. The file is left as-is.
    #[error("provider store at {path} is corrupted: {detail}")]
    Corrupted { path: PathBuf, detail: String },
    /// The file exists but could not be read at all (permissions, I/O).
    #[error("provider store at {path} could not be read: {detail}")]
    Unreadable { path: PathBuf, detail: String },
    /// Migration refused to run because it could not first take a backup.
    #[error("refusing to migrate {path} without a backup: {detail}")]
    BackupFailed { path: PathBuf, detail: String },
    /// The store was written but did not read back as expected. The original
    /// has been restored from the backup.
    #[error("post-write verification failed for {path}: {detail}")]
    VerifyFailed { path: PathBuf, detail: String },
}

/// The permanent, never-pruned copy of the pre-v4 store.
pub fn v3_backup_path(store_path: &Path) -> PathBuf {
    store_path.with_file_name("model_providers.v3.json")
}

/// Read a v4 store from disk without migrating anything.
///
/// Returns `Ok(None)` when the file does not exist or is not at v4 — the caller
/// decides whether that means "first run" or "needs migration".
pub fn read_store_v4(path: &Path) -> Result<Option<ProvidersStoreV4>, StoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).map_err(|e| StoreError::Unreadable {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    let text = text.trim_start_matches('\u{FEFF}');

    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| StoreError::Corrupted {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;

    match value.get("version").and_then(|v| v.as_u64()) {
        Some(v) if v == STORE_VERSION_V4 as u64 => {
            let store: ProvidersStoreV4 =
                serde_json::from_value(value).map_err(|e| StoreError::Corrupted {
                    path: path.to_path_buf(),
                    detail: e.to_string(),
                })?;
            Ok(Some(store))
        }
        _ => Ok(None),
    }
}

/// Write a v4 store atomically: temp file, then rename.
pub fn write_store_v4(store: &ProvidersStoreV4, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(store).context("failed to serialize v4 store")?;
    let temp = path.with_extension("json.v4tmp");
    std::fs::write(&temp, json.as_bytes())
        .with_context(|| format!("failed to write {}", temp.display()))?;
    std::fs::rename(&temp, path)
        .with_context(|| format!("failed to rename {} to {}", temp.display(), path.display()))?;
    Ok(())
}

/// What [`load_or_migrate_store_v4`] produced.
#[derive(Debug)]
pub struct LoadedStore {
    pub store: ProvidersStoreV4,
    /// `Some` only on the run that performed the migration. The UI shows it
    /// once and then it is gone; it is not persisted state.
    pub report: Option<MigrationReport>,
    /// Catalogs lifted out of the store, for the caller to persist to the cache
    /// directory. Empty on every run after the migrating one.
    pub catalogs: Vec<ExtractedCatalog>,
}

/// Load the store, migrating v1/v2/v3 → v4 if needed.
///
/// The v1→v2→v3 links are reused verbatim from [`super::store`]; this only adds
/// the final link and the safety envelope. Keeping the old links matters for a
/// user who has not opened SkillStar since the v1 store — dropping them would
/// mean their providers simply disappear.
pub fn load_or_migrate_store_v4(path: &Path) -> Result<LoadedStore, StoreError> {
    // Already v4? Nothing to do.
    if let Some(store) = read_store_v4(path)? {
        return Ok(LoadedStore {
            store,
            report: None,
            catalogs: Vec::new(),
        });
    }

    if !path.exists() {
        return Ok(LoadedStore {
            store: ProvidersStoreV4::default(),
            report: None,
            catalogs: Vec::new(),
        });
    }

    // The file is there and is not v4. Before touching anything, confirm it
    // parses at all — otherwise we would "migrate" a corrupted file into an
    // empty one, which is exactly the v3 failure this module exists to end.
    let text = std::fs::read_to_string(path).map_err(|e| StoreError::Unreadable {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{FEFF}')).map_err(|e| {
        StoreError::Corrupted {
            path: path.to_path_buf(),
            detail: e.to_string(),
        }
    })?;

    // Step 0 — two backups, and abort if either fails.
    take_migration_backups(path)?;

    // Steps 1-5 — run the v1→v2→v3 chain, then the pure v3→v4 function.
    let v3 = migrate_store_if_needed(path).map_err(|e| StoreError::Corrupted {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    let MigrationOutcome {
        store,
        report,
        catalogs,
    } = migrate_v3_to_v4(v3, &get_all_presets_flat());

    // Step 6 — write, then read back and verify.
    write_store_v4(&store, path).map_err(|e| StoreError::VerifyFailed {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    verify_or_restore(path, &store)?;

    Ok(LoadedStore {
        store,
        report: Some(report),
        catalogs,
    })
}

/// Take the rolling backup and the permanent v3 copy. Either failure aborts.
fn take_migration_backups(path: &Path) -> Result<(), StoreError> {
    crate::tool_sync::create_rolling_backup(path).map_err(|e| StoreError::BackupFailed {
        path: path.to_path_buf(),
        detail: format!("rolling backup: {e}"),
    })?;

    let permanent = v3_backup_path(path);
    // Never overwrite an existing v3 snapshot: on a second migration attempt
    // the current file may already be partially migrated, and clobbering the
    // good copy with it would destroy the only route back.
    if !permanent.exists() {
        std::fs::copy(path, &permanent).map_err(|e| StoreError::BackupFailed {
            path: path.to_path_buf(),
            detail: format!("permanent v3 copy at {}: {e}", permanent.display()),
        })?;
    }
    Ok(())
}

/// Read the freshly written store back and compare. On mismatch, put the
/// backup back where it was and report failure rather than leaving the user
/// with a file neither version can read.
fn verify_or_restore(path: &Path, expected: &ProvidersStoreV4) -> Result<(), StoreError> {
    let readback = match read_store_v4(path) {
        Ok(Some(store)) => store,
        Ok(None) => {
            restore_from_v3_backup(path);
            return Err(StoreError::VerifyFailed {
                path: path.to_path_buf(),
                detail: "written store did not read back as v4".to_string(),
            });
        }
        Err(e) => {
            restore_from_v3_backup(path);
            return Err(StoreError::VerifyFailed {
                path: path.to_path_buf(),
                detail: e.to_string(),
            });
        }
    };

    if &readback != expected {
        restore_from_v3_backup(path);
        return Err(StoreError::VerifyFailed {
            path: path.to_path_buf(),
            detail: "written store differs from what was serialized".to_string(),
        });
    }
    Ok(())
}

fn restore_from_v3_backup(path: &Path) {
    let permanent = v3_backup_path(path);
    if permanent.exists()
        && let Err(e) = std::fs::copy(&permanent, path)
    {
        warn!(
            "failed to restore {} from {}: {e}",
            path.display(),
            permanent.display()
        );
    }
}

/// Load the store from the default path, migrating if needed.
pub fn load_store() -> Result<LoadedStore, StoreError> {
    load_or_migrate_store_v4(&super::store::flat_store_path())
}

/// The full startup path: load, migrate, and repair what is already on disk.
///
/// Migrating the store is only half the job. The other half is that
/// `~/.codex/config.toml` may already contain `wire_api = "chat"` tables this
/// version can no longer produce — and which Codex can no longer parse, taking
/// the whole file down with them. So on the run that migrates, and only then,
/// every binding is re-projected: unwritable Codex entries are dropped from
/// both the store and the file, and everything that survives is rewritten.
///
/// Ordering matters. The catalogs are persisted *before* the re-sync, because
/// the OpenCode writer reads them to build its model blocks; doing it the other
/// way round would write a correct file with the model metadata stripped out.
///
/// After a repair the store is written again — the dropped entries are a store
/// change, and leaving them only in memory would resurrect them on restart.
pub fn load_store_and_repair(path: &Path) -> Result<LoadedStore, StoreError> {
    let mut loaded = load_or_migrate_store_v4(path)?;
    let Some(report) = loaded.report.as_mut() else {
        // Not the migrating run. Nothing on disk is stale.
        return Ok(loaded);
    };

    super::catalog_cache::persist_extracted(&loaded.catalogs, &mut report.warnings);
    crate::tool_sync::repair_agent_configs(&mut loaded.store, report);

    write_store_v4(&loaded.store, path).map_err(|e| StoreError::VerifyFailed {
        path: path.to_path_buf(),
        detail: format!("persisting the post-repair store: {e}"),
    })?;
    Ok(loaded)
}

/// Persist a v4 store to the default path.
pub fn save_store(store: &ProvidersStoreV4) -> Result<()> {
    write_store_v4(store, &super::store::flat_store_path())
}

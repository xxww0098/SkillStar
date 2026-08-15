//! Tests for the v4 store's safety envelope.
//!
//! These are the tests that stand between a user and a lost configuration, so
//! they assert on the *file on disk* after each operation rather than only on
//! return values. A migration that returns an error but has already overwritten
//! the file has still lost the data.

use crate::providers::binding::{AgentBinding, BindingEntry, ProvidersStoreV4, STORE_VERSION_V4};
use crate::providers::provider::Provider;
use crate::providers::store_v4::{
    LoadedStore, StoreError, load_or_migrate_store_v4, read_store_v4, v3_backup_path,
    write_store_v4,
};
use std::path::{Path, PathBuf};

/// A scratch directory that removes itself. Tests must never touch the real
/// `$HOME` — every path here is under the temp dir.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "skillstar-store-v4-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        Self(dir)
    }

    fn store_path(&self) -> PathBuf {
        self.0.join("model_providers.json")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_raw(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write fixture");
}

const V3_FIXTURE: &str = r#"{
  "version": 3,
  "providers": [
    {
      "id": "p1",
      "name": "Relay",
      "base_url_openai": "https://relay.example.com/v1",
      "base_url_anthropic": "",
      "models_url": "",
      "api_key": "sk-live-key",
      "models": ["m1"],
      "default_model": "m1",
      "sort_index": 0,
      "codex_wire_api": "chat",
      "codex_auth_mode": "third_party"
    }
  ],
  "tool_activations": {
    "codex": {
      "entries": [{ "provider_id": "p1", "model": "m1", "last_sync_at": 1700000000 }],
      "active_index": 0
    }
  }
}"#;

// ---------------------------------------------------------------------------
// G6 — a corrupted store is an error, and the file survives
// ---------------------------------------------------------------------------

#[test]
fn corrupted_store_returns_error_and_keeps_file() {
    let scratch = Scratch::new("corrupt");
    let path = scratch.store_path();
    let original = "{ this is not json";
    write_raw(&path, original);

    let err = load_or_migrate_store_v4(&path).expect_err("must not silently succeed");
    assert!(
        matches!(err, StoreError::Corrupted { .. }),
        "expected Corrupted, got {err:?}"
    );

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        original,
        "the corrupted file must be left exactly as it was — v3 replaced it \
         with an empty store, which is how one bad read destroyed a config"
    );
    assert!(
        !v3_backup_path(&path).exists(),
        "nothing was migrated, so nothing should have been backed up"
    );
}

#[test]
fn read_store_v4_reports_corruption_rather_than_returning_empty() {
    let scratch = Scratch::new("readcorrupt");
    let path = scratch.store_path();
    write_raw(&path, "{\"version\": 4, \"providers\": \"not an array\"}");

    let err = read_store_v4(&path).expect_err("must not degrade to an empty store");
    assert!(matches!(err, StoreError::Corrupted { .. }), "{err:?}");
}

#[test]
fn a_missing_store_is_a_first_run_not_an_error() {
    let scratch = Scratch::new("missing");
    let loaded = load_or_migrate_store_v4(&scratch.store_path()).expect("first run is fine");
    assert!(loaded.store.providers.is_empty());
    assert!(loaded.report.is_none());
    assert_eq!(loaded.store.version, STORE_VERSION_V4);
}

// ---------------------------------------------------------------------------
// R-1 — backups, and what happens without them
// ---------------------------------------------------------------------------

#[test]
fn migration_takes_both_backups_before_writing() {
    let scratch = Scratch::new("backups");
    let path = scratch.store_path();
    write_raw(&path, V3_FIXTURE);

    let loaded = load_or_migrate_store_v4(&path).expect("migration succeeds");
    assert!(loaded.report.is_some(), "a migrating run must report");

    let permanent = v3_backup_path(&path);
    assert!(permanent.exists(), "the permanent v3 copy must exist");
    assert_eq!(
        std::fs::read_to_string(&permanent).unwrap(),
        V3_FIXTURE,
        "the permanent copy must be the pre-migration bytes, verbatim — it is \
         what the report's undo button reads"
    );

    let rolling: Vec<_> = std::fs::read_dir(&scratch.0)
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .contains("model_providers.json.bak.")
        })
        .collect();
    assert_eq!(rolling.len(), 1, "one rolling backup as well");
}

#[test]
fn migration_aborts_when_the_backup_cannot_be_written() {
    let scratch = Scratch::new("nobackup");
    let path = scratch.store_path();
    write_raw(&path, V3_FIXTURE);

    // Make the directory read-only so the backup copy fails while the source
    // file is still perfectly readable — the exact split that v3 handled by
    // warning and migrating anyway.
    let mut perms = std::fs::metadata(&scratch.0).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o500);
    }
    std::fs::set_permissions(&scratch.0, perms).unwrap();

    let result = load_or_migrate_store_v4(&path);

    // Restore permissions before asserting so the scratch dir can be removed.
    let mut perms = std::fs::metadata(&scratch.0).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o700);
    }
    std::fs::set_permissions(&scratch.0, perms).unwrap();

    let err = result.expect_err("no backup means no migration");
    assert!(
        matches!(err, StoreError::BackupFailed { .. }),
        "expected BackupFailed, got {err:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        V3_FIXTURE,
        "the v3 file must still be intact and runnable"
    );
}

#[test]
fn a_second_migration_does_not_clobber_the_permanent_v3_copy() {
    let scratch = Scratch::new("secondrun");
    let path = scratch.store_path();
    write_raw(&path, V3_FIXTURE);

    load_or_migrate_store_v4(&path).expect("first migration");
    // Simulate a later run that finds a non-v4 file again (a partial write, a
    // hand edit): the good v3 snapshot must not be replaced by it.
    write_raw(&path, "{\"version\": 3, \"providers\": [], \"tool_activations\": {}}");
    load_or_migrate_store_v4(&path).expect("second migration");

    assert_eq!(
        std::fs::read_to_string(v3_backup_path(&path)).unwrap(),
        V3_FIXTURE,
        "the only route back must not be overwritten by a degraded copy"
    );
}

// ---------------------------------------------------------------------------
// Round trip
// ---------------------------------------------------------------------------

#[test]
fn migration_round_trips_through_disk() {
    let scratch = Scratch::new("roundtrip");
    let path = scratch.store_path();
    write_raw(&path, V3_FIXTURE);

    let LoadedStore { store, report, .. } = load_or_migrate_store_v4(&path).expect("migrate");
    let report = report.expect("report");

    // The migrated store is what is now on disk, and reading it back is a
    // no-op rather than a second migration.
    let reread = read_store_v4(&path).expect("read").expect("is v4");
    assert_eq!(reread, store);

    let again = load_or_migrate_store_v4(&path).expect("reload");
    assert!(
        again.report.is_none(),
        "an already-v4 store must not migrate a second time"
    );
    assert_eq!(again.store, store);

    // And the substance survived.
    assert_eq!(store.providers.len(), 1);
    assert_eq!(
        store.providers[0].credential.literal_secret(),
        Some("sk-live-key")
    );
    assert_eq!(
        store.bindings["codex"].entries[0].last_sync_at_ms,
        Some(1_700_000_000_000)
    );
    assert!(!report.needs_user_attention(), "nothing contestable happened");
}

#[test]
fn write_then_read_preserves_every_v4_only_field() {
    let scratch = Scratch::new("v4fields");
    let path = scratch.store_path();

    let mut provider = Provider::new("p1", "Relay");
    provider.endpoints.openai_chat = Some("https://relay.example.com/v1".to_string());
    provider.credential = crate::providers::credential::Credential::single_key("k1", "sk-x");
    provider.caps.responses_api = crate::providers::provider::Tri::Yes;

    let mut binding = AgentBinding::single(BindingEntry::new("p1", "m1"));
    binding.roles.insert(
        "fast".to_string(),
        crate::providers::binding::ModelRef::new("p1", "m-small"),
    );

    let store = ProvidersStoreV4 {
        version: STORE_VERSION_V4,
        providers: vec![provider],
        bindings: [("omp".to_string(), binding)].into_iter().collect(),
    };

    write_store_v4(&store, &path).expect("write");
    let read = read_store_v4(&path).expect("read").expect("is v4");

    assert_eq!(read, store, "roles, caps and credential variants all survive");
}

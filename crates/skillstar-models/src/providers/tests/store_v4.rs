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

    // Occupy the permanent snapshot path as a directory so `fs::copy` fails
    // while the v3 source stays readable — the split v3 handled by warning
    // and migrating anyway. A parent-dir readonly bit is not enough: Windows
    // still creates files inside a "readonly" folder.
    std::fs::create_dir(v3_backup_path(&path)).expect("block permanent backup path");

    let err = load_or_migrate_store_v4(&path).expect_err("no backup means no migration");
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
    write_raw(
        &path,
        "{\"version\": 3, \"providers\": [], \"tool_activations\": {}}",
    );
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
    assert!(
        !report.needs_user_attention(),
        "nothing contestable happened"
    );
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

    assert_eq!(
        read, store,
        "roles, caps and credential variants all survive"
    );
}

// ---------------------------------------------------------------------------
// A v4 file must never reach the v1 parser
// ---------------------------------------------------------------------------

#[test]
fn the_v3_reader_refuses_a_v4_file_instead_of_emptying_it() {
    // The failure this guards against is silent and total. `migrate_store_if_needed`
    // reaches its v1 arm by exclusion — anything that is not v3 and not v2 is
    // assumed to be v1 — and every field of the v1 struct is `#[serde(default)]`.
    // So a v4 file *parses successfully* as a v1 store with four empty buckets,
    // and the migration then writes that empty store back over the user's real
    // configuration. One downgraded launch would be enough.
    let scratch = Scratch::new("v4-not-v1");
    let path = scratch.store_path();

    let store = ProvidersStoreV4 {
        version: STORE_VERSION_V4,
        providers: vec![Provider::new("p1", "Relay")],
        bindings: [(
            "omp".to_string(),
            AgentBinding::single(BindingEntry::new("p1", "m1")),
        )]
        .into_iter()
        .collect(),
    };
    write_store_v4(&store, &path).expect("write");
    let before = std::fs::read_to_string(&path).expect("read back");

    let err = crate::providers::store::migrate_store_if_needed(&path)
        .expect_err("a store from the future must be an error, not an empty parse");

    let message = format!("{err:#}");
    assert!(
        message.contains("version 4"),
        "the error must name the version it could not handle: {message}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read after"),
        before,
        "the file must be left exactly as it was — nothing is worth overwriting it with"
    );
}

#[test]
fn a_migrated_store_survives_a_restart_with_every_field_intact() {
    // The round-trip that matters in production: migrate once, then start again
    // and confirm the second launch sees the same store rather than re-running
    // a migration (or, worse, a v1 parse) over its own output.
    let scratch = Scratch::new("restart");
    let path = scratch.store_path();
    write_raw(&path, V3_FIXTURE);

    let first = load_or_migrate_store_v4(&path).expect("first launch");
    assert!(first.report.is_some(), "the first launch migrates");
    let after_migration = std::fs::read_to_string(&path).expect("read");

    let second = load_or_migrate_store_v4(&path).expect("second launch");

    assert!(
        second.report.is_none(),
        "the second launch must not migrate"
    );
    assert_eq!(second.store, first.store);
    assert!(
        second.catalogs.is_empty(),
        "catalogs are extracted once; a second pass would re-report them as new"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        after_migration,
        "a launch that changes nothing must not rewrite the file"
    );
}

#[test]
fn a_store_from_an_unknown_future_version_is_refused_rather_than_migrated() {
    let scratch = Scratch::new("v99");
    let path = scratch.store_path();
    write_raw(
        &path,
        r#"{ "version": 99, "providers": [], "bindings": {} }"#,
    );

    // Not v4, so `read_store_v4` declines it; it must then be refused rather
    // than fed to the v1 parser, and the file must be left alone.
    let err = load_or_migrate_store_v4(&path).expect_err("refuse");
    assert!(
        matches!(err, StoreError::Corrupted { .. }),
        "an unreadable-but-present store is a state the user must resolve: {err}"
    );
}

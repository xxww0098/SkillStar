//! Tests for the typed `model_gateway.json` container `store::doc`
//! (spec slice 03).
//!
//! The container is `pub(crate)` and `lib.rs` must keep its public surface
//! frozen (decision D3), so this test target compiles the module source
//! directly instead of importing it:
//!
//! ```ignore
//! #[path = "../src/store/doc.rs"]
//! mod doc;
//! ```
//!
//! Everything asserted here is file behavior: the typed path writes the
//! same document the legacy `Value` path writes, a missing file reads as
//! defaults without being created, a broken file refuses the strict open
//! while the lenient one reads defaults, and owner rows survive with
//! duplicate and missing ids untouched. Key order is not a contract (D11),
//! so every "same file" assertion compares parse-back values.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use serde_json::{Value, json};
use skillstar_gateway::save_group;

#[path = "../src/store/doc.rs"]
mod doc;

use doc::{DocStoreError, ModelGatewayDoc};

#[test]
fn a_typed_round_trip_sorts_keys_the_way_the_value_path_did() {
    let _lock = lock_gateway_env();
    let root = scratch("doc-roundtrip");
    let _env = EnvRestore::sandbox(&root);
    install_fixture(&root);
    let fixture = read_doc(&root);

    // The legacy value path: one full-file pretty rewrite that keeps every
    // key. Re-saving the stored members of the stored group mutates nothing,
    // so the output is the fixture through the value-shaped pipeline.
    save_group("team", &["probe/m1", "probe/m2"]).unwrap();
    let via_value = read_doc(&root);

    // The typed path: open and save with no lens in between. If `rest` or
    // `extra` dropped a key, or a known field fabricated one the file never
    // had, this parse-back would drift from the value path's.
    fs::write(
        gateway_file(&root),
        include_bytes!("fixtures/gateway/handwritten.json"),
    )
    .unwrap();
    ModelGatewayDoc::open().unwrap().save().unwrap();
    let via_typed = read_doc(&root);

    assert_eq!(via_typed, via_value);
    // Neither path moved any content: both are the fixture, re-sorted.
    assert_eq!(via_typed, fixture);
}

#[test]
fn a_missing_file_reads_as_defaults_and_is_not_created() {
    let _lock = lock_gateway_env();
    let root = scratch("doc-missing");
    let _env = EnvRestore::sandbox(&root);
    let file = gateway_file(&root);
    assert!(!file.exists());

    assert_eq!(ModelGatewayDoc::open_lenient(), ModelGatewayDoc::default());
    let strict = ModelGatewayDoc::open().unwrap();
    assert_eq!(strict, ModelGatewayDoc::default());

    assert!(!file.exists(), "opening must not create model_gateway.json");
}

#[test]
fn a_broken_file_refuses_open_but_lenient_reads_defaults() {
    let _lock = lock_gateway_env();
    let root = scratch("doc-broken");
    let _env = EnvRestore::sandbox(&root);

    // Unparseable bytes, a top-level array, and a top-level number: the
    // strict open refuses all three, the lenient open reads defaults, and
    // neither path touches the bytes.
    for broken in ["{ not json", "[]", "3"] {
        fs::write(gateway_file(&root), broken).unwrap();
        assert_eq!(ModelGatewayDoc::open(), Err(DocStoreError::Parse));
        assert_eq!(ModelGatewayDoc::open_lenient(), ModelGatewayDoc::default());
        assert_eq!(fs::read(gateway_file(&root)).unwrap(), broken.as_bytes());
    }
}

#[test]
fn owner_rows_keep_duplicate_and_missing_ids_as_they_are() {
    let _lock = lock_gateway_env();
    let root = scratch("doc-rows");
    let _env = EnvRestore::sandbox(&root);
    fs::write(
        gateway_file(&root),
        br#"{
            "groups": [
                {"id": "team", "members": ["probe/m1"]},
                {"id": "team", "members": ["probe/m2"], "note": "same id twice"},
                {"members": ["probe/m1"], "note": "no id at all"}
            ],
            "providers": [
                {"id": "probe", "routing": "rotate"},
                {"id": "probe", "affinity": "session"}
            ]
        }"#,
    )
    .unwrap();

    ModelGatewayDoc::open().unwrap().save().unwrap();

    // First-match-by-id is the callers' lookup rule (slices 04-05); the
    // container keeps every row, in order, without deduplicating ids and
    // without dropping or reshaping the row that has none.
    assert_eq!(
        read_doc(&root)["groups"],
        json!([
            {"id": "team", "members": ["probe/m1"]},
            {"id": "team", "members": ["probe/m2"], "note": "same id twice"},
            {"members": ["probe/m1"], "note": "no id at all"},
        ])
    );
    assert_eq!(
        read_doc(&root)["providers"],
        json!([
            {"id": "probe", "routing": "rotate"},
            {"id": "probe", "affinity": "session"},
        ])
    );
}

/// The tree-01 fixture, plus the catalog cache it references. Copied from
/// `tests/gateway_roundtrip.rs` (the sandbox helpers below too).
fn install_fixture(root: &Path) {
    fs::write(
        gateway_file(root),
        include_bytes!("fixtures/gateway/handwritten.json"),
    )
    .unwrap();
    let cache = skillstar_gateway::models_dev_cache_path();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(cache, CATALOG).unwrap();
}

/// Catalog cache for the sandbox: `probe/m2` is listed so `save_group`
/// members and `save_model_name` ids resolve if a test needs them.
const CATALOG: &[u8] = br#"{
    "probe": {
        "models": {
            "m1": {"id": "m1", "reasoning_options": [{"type": "effort", "values": ["low", "medium", "high"]}]},
            "m2": {"id": "m2"}
        }
    }
}"#;

fn read_doc(root: &Path) -> Value {
    serde_json::from_slice(&fs::read(gateway_file(root)).unwrap()).unwrap()
}

fn gateway_file(root: &Path) -> PathBuf {
    root.join("data").join("config").join("model_gateway.json")
}

fn lock_gateway_env() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn scratch(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("skillstar-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

struct EnvRestore {
    saved: Vec<(&'static str, Option<OsString>)>,
    root: PathBuf,
}

impl EnvRestore {
    fn sandbox(root: &Path) -> Self {
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(data.join("config")).unwrap();
        let pairs = [
            ("HOME", Some(home.to_string_lossy().into_owned())),
            ("USERPROFILE", Some(home.to_string_lossy().into_owned())),
            (
                "SKILLSTAR_TOOL_SYNC_HOME",
                Some(home.to_string_lossy().into_owned()),
            ),
            (
                "SKILLSTAR_DATA_DIR",
                Some(data.to_string_lossy().into_owned()),
            ),
        ];
        let saved = pairs
            .into_iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
                (key, previous)
            })
            .collect();
        Self {
            saved,
            root: root.to_path_buf(),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, previous) in self.saved.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

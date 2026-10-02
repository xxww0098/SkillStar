//! Behavior lock for `model_gateway.json` reads and writes (spec slice 01).
//!
//! These tests pin today's file semantics before any tree surgery (slices
//! 02-05): each of the five writers keeps every foreign key and row field,
//! profile saves drop unknown row fields, saves are idempotent, a broken
//! file refuses writes without touching the bytes, and a missing file reads
//! as defaults without being created. Key order is not a contract (D11:
//! serde_json without `preserve_order` rewrites keys alphabetically), so
//! every "same file" assertion compares parse-back values, never raw bytes —
//! except a file that cannot parse at all, where the bytes are the only
//! witness.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use serde_json::{Value, json};
use skillstar_gateway::{
    AffinityMode, GroupRule, ProfileAgent, RouteMode, RouteOwner, SaveGroupError, SaveListenError,
    SaveModelNameError, SaveProfileError, SaveRoutingError, SavedGroup, catalog_serves,
    expand_group, listen_label, model_efforts, model_shown, models_dev_cache_path, profile_names,
    routing_state, save_group, save_listen, save_model_name, save_profile, save_routing,
    stored_classifier, stored_group_ids, stored_groups, stored_model_names, stored_rules,
};

/// Catalog cache for the sandbox: `probe/m1` lists effort levels so a saved
/// `model_efforts` subset can narrow them, and `probe/m2` is listed so
/// `save_model_name` accepts it.
const CATALOG: &[u8] = br#"{
    "probe": {
        "models": {
            "m1": {"id": "m1", "reasoning_options": [{"type": "effort", "values": ["low", "medium", "high"]}]},
            "m2": {"id": "m2"}
        }
    }
}"#;

/// The five writers, in the crossing order. Each owns one field cluster:
/// group rows, provider/group `routing` + `affinity`, `profiles`,
/// `model_names`, `listen`.
fn run_five_writers() {
    save_group("team", &["probe/m2", "probe/m1"]).unwrap();
    save_routing(
        RouteOwner::Provider,
        "probe",
        RouteMode::Rotate,
        AffinityMode::Session,
    )
    .unwrap();
    save_routing(
        RouteOwner::Group,
        "group/team",
        RouteMode::Usage,
        AffinityMode::Turn,
    )
    .unwrap();
    save_profile(
        "work",
        &[
            ProfileAgent {
                id: "codex".to_string(),
                model_ref: "probe/m2".to_string(),
            },
            ProfileAgent {
                id: "gemini".to_string(),
                model_ref: "probe/m1".to_string(),
            },
        ],
    )
    .unwrap();
    save_model_name("probe/m2", "Probe Two").unwrap();
    save_listen("lan").unwrap();
}

#[test]
fn a_field_write_keeps_every_foreign_key_and_row_field() {
    let _lock = lock_gateway_env();
    let root = scratch("cross");
    let _env = EnvRestore::sandbox(&root);
    install_fixture(&root);

    // Anchors so a fixture edit cannot silently weaken what follows.
    let before = read_doc(&root);
    assert_eq!(before["providers"][0]["note"], "handwritten provider note");
    assert_eq!(before["profiles"][0]["note"], "handwritten profile note");
    assert_eq!(before["profiles"][1]["x-extra"], "keep-me");
    assert_eq!(before["x-custom"]["whatever"][0], "also");

    run_five_writers();

    // Every writer kept every other writer's cluster, every handwritten
    // row field, and every bystander top-level key. Profile rows are the
    // documented exception: save_profile rebuilds each row from
    // name + agents only (see
    // profile_save_drops_unknown_fields_on_profile_rows_today).
    assert_eq!(
        read_doc(&root),
        json!({
            "providers": [{
                "affinity": "session",
                "family": "probe-family",
                "id": "probe",
                "note": "handwritten provider note",
                "routing": "rotate",
                "tier": 2,
            }],
            "groups": [{
                "affinity": "turn",
                "classifier": "probe/m1",
                "family": "team-family",
                "id": "team",
                "members": ["probe/m2", "probe/m1"],
                "note": "handwritten group note",
                "rules": [{"images": true, "use": "probe/m2"}],
                "routing": "usage",
            }],
            "profiles": [
                {
                    "agents": [
                        {"id": "codex", "model_ref": "probe/m2"},
                        {"id": "gemini", "model_ref": "probe/m1"},
                    ],
                    "name": "work",
                },
                {
                    "agents": [{"id": "opencode", "model_ref": "probe/m2"}],
                    "name": "personal",
                },
            ],
            "model_names": {"probe/m1": "Probe One", "probe/m2": "Probe Two"},
            "model_efforts": {"probe/m1": ["low", "high"]},
            "visible": {"codex": ["probe", "team"]},
            "listen": "lan",
            "redact": true,
            "redact_personal": true,
            "redact_words": true,
            "redact_word_list": ["acme-corp"],
            "redact_rule_list": [
                {"kind": "token", "prefix": "sk-live-", "regex": "[a-z0-9]{8,}"},
            ],
            "vision": "probe/m2",
            "x-custom": {"whatever": ["also", "kept"]},
        })
    );

    // The reader side of the same file, which slices 04-05 must preserve.
    assert_eq!(
        routing_state(RouteOwner::Provider, "probe").unwrap(),
        (RouteMode::Rotate, AffinityMode::Session)
    );
    assert_eq!(
        routing_state(RouteOwner::Group, "group/team").unwrap(),
        (RouteMode::Usage, AffinityMode::Turn)
    );
    assert_eq!(
        stored_groups(),
        vec![SavedGroup {
            id: "team".to_string(),
            members: vec!["probe/m2".to_string(), "probe/m1".to_string()],
        }]
    );
    assert_eq!(stored_group_ids(), vec!["team".to_string()]);
    assert_eq!(
        profile_names(),
        vec!["work".to_string(), "personal".to_string()]
    );
    assert_eq!(
        stored_model_names().get("probe/m2").map(String::as_str),
        Some("Probe Two")
    );
    assert_eq!(listen_label(), "lan");
    assert_eq!(
        model_efforts("probe/m1"),
        vec!["low".to_string(), "high".to_string()]
    );
    assert_eq!(
        stored_rules("group/team"),
        vec![GroupRule {
            use_member: "probe/m2".to_string(),
            images: true,
            ..GroupRule::default()
        }]
    );
    assert_eq!(stored_classifier("group/team"), "probe/m1");
    assert!(model_shown("codex", "probe/m1"));
    assert!(!model_shown("codex", "other/m1"));
    assert!(catalog_serves("group/team"));
    assert!(!catalog_serves("group/ghost"));
}

#[test]
fn profile_save_drops_unknown_fields_on_profile_rows_today() {
    // D6 record: write_profiles rebuilds every row with json!, so unknown
    // fields die on the saved row and on bystander rows alike. Keeping them
    // would be a behavior change, not a refactor.
    let _lock = lock_gateway_env();
    let root = scratch("profile-drop");
    let _env = EnvRestore::sandbox(&root);
    install_fixture(&root);

    save_profile(
        "work",
        &[ProfileAgent {
            id: "codex".to_string(),
            model_ref: "probe/m2".to_string(),
        }],
    )
    .unwrap();

    let rows = read_doc(&root)["profiles"].as_array().unwrap().clone();
    assert_eq!(row_keys(&rows[0]), vec!["agents", "name"]);
    assert!(rows[0].get("note").is_none());
    // The bystander row is rebuilt too: its handwritten key dies with it.
    assert_eq!(row_keys(&rows[1]), vec!["agents", "name"]);
    assert!(rows[1].get("x-extra").is_none());
}

#[test]
fn saving_twice_is_byte_idempotent() {
    let _lock = lock_gateway_env();
    let root = scratch("idempotent");
    let _env = EnvRestore::sandbox(&root);
    install_fixture(&root);

    run_five_writers();
    let first = read_doc(&root);
    run_five_writers();
    let second = read_doc(&root);

    // Key order is not a contract (D11): compare parse-back values.
    assert_eq!(first, second);
}

#[test]
fn a_broken_file_refuses_the_write_and_keeps_the_bytes() {
    let _lock = lock_gateway_env();
    let root = scratch("broken");
    let _env = EnvRestore::sandbox(&root);
    install_fixture(&root);

    // Bytes that cannot parse, then bytes that parse but are not an object:
    // every writer refuses and the file is left exactly as it was. The
    // bytes are compared raw because there is nothing to parse back.
    for broken in ["{ not json", "[]"] {
        fs::write(gateway_file(&root), broken).unwrap();
        assert!(matches!(
            save_group("team", &["probe/m1"]),
            Err(SaveGroupError::Store)
        ));
        assert!(matches!(
            save_routing(
                RouteOwner::Provider,
                "probe",
                RouteMode::Rotate,
                AffinityMode::Session
            ),
            Err(SaveRoutingError::Store)
        ));
        assert!(matches!(
            save_profile("work", &[]),
            Err(SaveProfileError::Store)
        ));
        assert!(matches!(
            save_model_name("probe/m1", "Probe Two"),
            Err(SaveModelNameError::Store)
        ));
        assert!(matches!(save_listen("lan"), Err(SaveListenError::Store)));
        assert!(matches!(
            save_listen("loopback"),
            Err(SaveListenError::Store)
        ));
        assert_eq!(fs::read(gateway_file(&root)).unwrap(), broken.as_bytes());
    }

    // group.rs additionally refuses when `groups` is not an array, where the
    // other writers would happily round-trip the file.
    fs::write(gateway_file(&root), br#"{"groups": 3}"#).unwrap();
    assert!(matches!(
        save_group("team", &["probe/m1"]),
        Err(SaveGroupError::Store)
    ));
    assert_eq!(fs::read(gateway_file(&root)).unwrap(), br#"{"groups": 3}"#);
}

#[test]
fn a_missing_file_reads_as_defaults_and_is_not_created() {
    let _lock = lock_gateway_env();
    let root = scratch("missing");
    let _env = EnvRestore::sandbox(&root);
    let file = gateway_file(&root);
    assert!(!file.exists());

    assert_eq!(listen_label(), "loopback");
    assert_eq!(
        routing_state(RouteOwner::Provider, "probe").unwrap(),
        (RouteMode::Smart, AffinityMode::Auto)
    );
    assert_eq!(
        routing_state(RouteOwner::Group, "group/team").unwrap(),
        (RouteMode::Smart, AffinityMode::Auto)
    );
    assert!(stored_groups().is_empty());
    assert!(stored_group_ids().is_empty());
    assert!(expand_group("group/team", &[]).is_empty());
    assert!(profile_names().is_empty());
    assert!(stored_model_names().is_empty());
    assert!(model_efforts("probe/m1").is_empty());
    assert!(stored_rules("group/team").is_empty());
    assert!(stored_classifier("group/team").is_empty());
    assert!(model_shown("codex", "probe/m1"));
    assert!(!catalog_serves("group/team"));

    assert!(!file.exists(), "a read must not create model_gateway.json");
}

fn install_fixture(root: &Path) {
    fs::write(
        gateway_file(root),
        include_bytes!("fixtures/gateway/handwritten.json"),
    )
    .unwrap();
    let cache = models_dev_cache_path();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(cache, CATALOG).unwrap();
}

/// Sorted key names of one profile row (serde_json maps are BTreeMaps, so
/// the order is alphabetical and stable).
fn row_keys(row: &Value) -> Vec<&str> {
    row.as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

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

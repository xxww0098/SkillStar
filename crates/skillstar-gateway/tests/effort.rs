//! Effort is fitted to the catalog. A fixed member level wins. An unknown model
//! keeps the effort it sent. The Claude bridge still maps `xhigh` on its own.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use serde_json::{Value, json};
use skillstar_gateway::{
    apply_upstream_effort, bridge_effort_arg, model_efforts, models_dev_cache_path, save_group,
    save_model_name,
};

#[test]
fn effort_clamped_to_catalog() {
    let _lock = lock_gateway_env();
    let root = scratch("effort-clamp");
    let _env = EnvRestore::sandbox(&root);
    let cache = write_catalog(&root);
    save_model_name("probe/m1", "实验").unwrap();

    let inbound = br#"{"model":"probe/m1","reasoning_effort":"medium"}"#;
    let outbound = apply_upstream_effort(inbound, "");
    let doc: Value = serde_json::from_slice(&outbound).unwrap();
    assert_eq!(doc["reasoning_effort"], "high");
    assert_eq!(doc["model"], "probe/m1");
    assert!(!String::from_utf8(outbound).unwrap().contains("实验"));
    assert_eq!(fs::read(&cache).unwrap(), catalog_bytes());
    assert_eq!(model_efforts("probe/m1"), vec!["low".to_string(), "high".to_string()]);
    assert_eq!(model_efforts("实验"), Vec::<String>::new());
}

#[test]
fn effort_unknown_model_passes_through() {
    let _lock = lock_gateway_env();
    let root = scratch("effort-unknown");
    let _env = EnvRestore::sandbox(&root);
    let cache = write_catalog(&root);
    let inbound = br#"{"model":"missing/nope","reasoning_effort":"low"}"#;
    let outbound = apply_upstream_effort(inbound, "");
    assert_eq!(outbound, inbound);
    assert_eq!(fs::read(cache).unwrap(), catalog_bytes());
    assert!(models_dev_cache_path().is_file());
}

#[test]
fn effort_member_fixed_wins() {
    let _lock = lock_gateway_env();
    let root = scratch("effort-fixed");
    let _env = EnvRestore::sandbox(&root);
    let cache = write_catalog(&root);
    save_group("fast", &["probe/m1:high"]).unwrap();

    let inbound = br#"{"model":"group/fast","reasoning_effort":"low"}"#;
    let outbound = apply_upstream_effort(inbound, "");
    let doc: Value = serde_json::from_slice(&outbound).unwrap();
    assert_eq!(doc["reasoning_effort"], "high");
    assert_eq!(doc["model"], "group/fast");
    assert_eq!(fs::read(cache).unwrap(), catalog_bytes());

    let direct = apply_upstream_effort(br#"{"model":"probe/m1","reasoning_effort":"low"}"#, "probe/m1:high");
    let direct: Value = serde_json::from_slice(&direct).unwrap();
    assert_eq!(direct["reasoning_effort"], "high");
    assert_eq!(direct["model"], "probe/m1");
}

#[test]
fn effort_claude_bridge_xhigh_unchanged() {
    let _lock = lock_gateway_env();
    let root = scratch("effort-bridge");
    let _env = EnvRestore::sandbox(&root);
    assert_eq!(bridge_effort_arg("xhigh"), "max");
    assert_eq!(bridge_effort_arg("high"), "high");

    let inbound = br#"{"model":"probe/m1","reasoning_effort":"xhigh"}"#;
    let outbound = apply_upstream_effort(inbound, "");
    assert_eq!(outbound, inbound);
}

#[test]
fn effort_subset_narrows_offered_levels() {
    let _lock = lock_gateway_env();
    let root = scratch("effort-subset-list");
    let _env = EnvRestore::sandbox(&root);
    write_wide_catalog();
    write_efforts(&json!({"model_efforts": {"probe/m1": ["max", "high"]}}));
    assert_eq!(
        model_efforts("probe/m1"),
        vec!["high".to_string(), "max".to_string()]
    );
    assert_eq!(model_efforts("实验"), Vec::<String>::new());
}

#[test]
fn effort_subset_fits_unfixed_request() {
    let _lock = lock_gateway_env();
    let root = scratch("effort-subset-fit");
    let _env = EnvRestore::sandbox(&root);
    write_wide_catalog();
    write_efforts(&json!({"model_efforts": {"probe/m1": ["high"]}}));
    let outbound = apply_upstream_effort(br#"{"model":"probe/m1","reasoning_effort":"low"}"#, "");
    let doc: Value = serde_json::from_slice(&outbound).unwrap();
    assert_eq!(doc["reasoning_effort"], "high");
    assert_eq!(doc["model"], "probe/m1");
    assert!(!String::from_utf8(outbound).unwrap().contains("https://"));
}

#[test]
fn effort_subset_empty_uses_catalog() {
    let _lock = lock_gateway_env();
    let root = scratch("effort-subset-empty");
    let _env = EnvRestore::sandbox(&root);
    write_catalog(&root);
    write_efforts(&json!({"model_efforts": {"probe/m1": []}}));
    let outbound = apply_upstream_effort(br#"{"model":"probe/m1","reasoning_effort":"medium"}"#, "");
    let doc: Value = serde_json::from_slice(&outbound).unwrap();
    assert_eq!(doc["reasoning_effort"], "high");
    assert_eq!(model_efforts("probe/m1"), vec!["low".to_string(), "high".to_string()]);
}

#[test]
fn effort_member_fixed_still_wins() {
    let _lock = lock_gateway_env();
    let root = scratch("effort-subset-fixed");
    let _env = EnvRestore::sandbox(&root);
    write_wide_catalog();
    write_efforts(&json!({"model_efforts": {"probe/m1": ["high"]}}));
    save_group("fast", &["probe/m1:max"]).unwrap();
    let outbound = apply_upstream_effort(br#"{"model":"group/fast","reasoning_effort":"low"}"#, "");
    let doc: Value = serde_json::from_slice(&outbound).unwrap();
    assert_eq!(doc["reasoning_effort"], "max");
    assert_eq!(doc["model"], "group/fast");
    let again = fs::read_to_string(gateway_file()).unwrap();
    assert!(again.contains("\"model_efforts\""), "{again}");
    assert!(again.contains("probe/m1:max"), "{again}");
}

fn wide_catalog() -> &'static [u8] {
    br#"{"probe":{"models":{"m1":{"id":"m1","url":"https://vendor.example/m","reasoning_options":[{"type":"effort","values":["low","high","max"]}]}}}}"#
}

fn write_wide_catalog() {
    let cache = models_dev_cache_path();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, wide_catalog()).unwrap();
}

fn gateway_file() -> PathBuf {
    models_dev_cache_path()
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .unwrap()
        .join("config")
        .join("model_gateway.json")
}

fn write_efforts(doc: &Value) {
    let path = gateway_file();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(doc).unwrap()).unwrap();
}

fn catalog_bytes() -> &'static [u8] {
    br#"{"probe":{"models":{"m1":{"id":"m1","reasoning_options":[{"type":"effort","values":["low","high"]}]}}}}"#
}

fn write_catalog(root: &Path) -> PathBuf {
    let cache = models_dev_cache_path();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, catalog_bytes()).unwrap();
    let _ = root;
    cache
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
    let path = std::env::temp_dir().join(format!("skillstar-{label}-{}-{nanos}", std::process::id()));
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
            ("SKILLSTAR_TOOL_SYNC_HOME", Some(home.to_string_lossy().into_owned())),
            ("SKILLSTAR_DATA_DIR", Some(data.to_string_lossy().into_owned())),
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

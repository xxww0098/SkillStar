//! A display name changes the picker label. The upstream model field stays the id.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use serde_json::Value;
use skillstar_gateway::{
    Protocol, model_label, models_dev_cache_path, save_model_name, stored_model_names, upstream_body,
};

#[test]
fn rename_changes_label_only() {
    let _lock = lock_gateway_env();
    let root = scratch("rename-label");
    let _env = EnvRestore::sandbox(&root);
    let cache = models_dev_cache_path();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    let catalog = br#"{"probe":{"models":{"m1":{"id":"m1"}}},"openai":{"models":{"gpt-test":{}}}}"#;
    fs::write(&cache, catalog).unwrap();
    fs::write(gateway_file(&root), br#"{"profiles":[{"name":"work"}]}"#).unwrap();

    save_model_name("probe/m1", "实验").unwrap();
    assert_eq!(fs::read(&cache).unwrap(), catalog);
    assert_eq!(model_label("probe/m1", &stored_model_names()), "实验");
    assert_eq!(model_label("openai/gpt-test", &stored_model_names()), "openai/gpt-test");

    let doc: Value = serde_json::from_slice(&fs::read(gateway_file(&root)).unwrap()).unwrap();
    assert_eq!(doc["model_names"]["probe/m1"], "实验");
    assert_eq!(doc["profiles"][0]["name"], "work");

    let kept = fs::read(gateway_file(&root)).unwrap();
    assert!(save_model_name("probe/m1", "").is_err());
    assert!(save_model_name("probe/m1", "实\n验").is_err());
    assert!(save_model_name("group/fast", "实验").is_err());
    assert_eq!(fs::read(gateway_file(&root)).unwrap(), kept);
}

#[test]
fn rename_upstream_model_field_unchanged() {
    let _lock = lock_gateway_env();
    let root = scratch("rename-upstream");
    let _env = EnvRestore::sandbox(&root);
    let cache = models_dev_cache_path();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, br#"{"probe":{"models":{"m1":{"id":"m1"}}}}"#).unwrap();
    save_model_name("probe/m1", "实验").unwrap();

    let inbound = br#"{"model":"probe/m1","messages":[{"role":"user","content":"hi"}]}"#;
    let outbound = upstream_body(Protocol::Chat, inbound).unwrap();
    let doc: Value = serde_json::from_slice(&outbound).unwrap();
    assert_eq!(doc["model"], "probe/m1");
    let text = String::from_utf8(outbound).unwrap();
    assert!(!text.contains("实验"), "{text}");
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

//! Profiles store agent ids and model refs, then call the existing writer.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use serde_json::Value;
use skillstar_gateway::{
    ApplyProfileError, ProfileAgent, SaveProfileError, apply_gateway, apply_profile, profile_names,
    save_profile,
};

#[test]
fn profile_apply_calls_existing_writers() {
    let _lock = lock_gateway_env();
    let root = scratch("profile-apply");
    let _env = EnvRestore::sandbox(&root);
    let home = root.join("home");
    let original = "{\n  \"model\": \"user-kept-model\"\n}\n";
    let opencode = home.join(".config/opencode/opencode.json");
    fs::create_dir_all(opencode.parent().unwrap()).unwrap();
    fs::write(&opencode, original).unwrap();

    let work = [
        agent("opencode", "openai/gpt-test"),
        agent("pi", "group/fast"),
    ];
    let home_profile = [
        agent("opencode", "group/fast"),
        agent("pi", "openai/gpt-test"),
    ];
    save_profile("work", &work).unwrap();
    save_profile("home", &home_profile).unwrap();
    assert_eq!(profile_names(), vec!["work".to_string(), "home".to_string()]);

    let gateway = gateway_file(&root);
    let before = fs::read(&gateway).unwrap();
    let applied = apply_profile("work").unwrap();
    assert_eq!(applied.applied, vec!["opencode".to_string(), "pi".to_string()]);
    assert!(applied.skipped.is_empty());
    assert_eq!(fs::read(&gateway).unwrap(), before, "apply does not rewrite the gateway file");
    let opencode_work = fs::read_to_string(&opencode).unwrap();
    let pi_work = fs::read_to_string(home.join(".pi/agent/settings.json")).unwrap();
    assert!(opencode_work.contains("openai/gpt-test"), "{opencode_work}");
    assert!(pi_work.contains("group/fast"), "{pi_work}");
    assert!(!opencode_work.contains("api.openai.com"));
    assert!(!pi_work.contains("sk-"));

    apply_profile("home").unwrap();
    let opencode_home = fs::read_to_string(&opencode).unwrap();
    let pi_home = fs::read_to_string(home.join(".pi/agent/settings.json")).unwrap();
    assert!(opencode_home.contains("group/fast"), "{opencode_home}");
    assert!(pi_home.contains("openai/gpt-test"), "{pi_home}");

    apply_gateway("opencode", "").unwrap();
    assert_eq!(fs::read_to_string(&opencode).unwrap(), original);
}

#[test]
fn profile_skips_unmanaged_without_write() {
    let _lock = lock_gateway_env();
    let root = scratch("profile-skip");
    let _env = EnvRestore::sandbox(&root);
    let home = root.join("home");

    save_profile(
        "mixed",
        &[agent("opencode", "openai/gpt-test"), agent("goose", "openai/gpt-test")],
    )
    .unwrap();
    let applied = apply_profile("mixed").unwrap();
    assert_eq!(applied.applied, vec!["opencode".to_string()]);
    assert_eq!(applied.skipped, vec!["goose".to_string()]);
    assert!(home.join(".config/opencode/opencode.json").is_file());
    assert!(!home.join(".config/goose").exists());
    assert!(!home.join(".goose").exists());

    let missing = apply_profile("missing").unwrap_err();
    assert_eq!(missing, ApplyProfileError::Missing);
    assert_eq!(missing.to_string(), "profile_missing");
}

#[test]
fn profile_is_not_a_library_sync() {
    let _lock = lock_gateway_env();
    let root = scratch("profile-library");
    let _env = EnvRestore::sandbox(&root);
    let home = root.join("home");
    let gateway = gateway_file(&root);

    save_profile("work", &[agent("opencode", "openai/gpt-test")]).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&gateway).unwrap()).unwrap();
    let keys: Vec<_> = doc.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, vec!["profiles".to_string()]);
    let profile = &doc["profiles"][0];
    let mut profile_keys: Vec<_> = profile.as_object().unwrap().keys().cloned().collect();
    profile_keys.sort();
    assert_eq!(profile_keys, vec!["agents".to_string(), "name".to_string()]);
    let mut agent_keys: Vec<_> = profile["agents"][0].as_object().unwrap().keys().cloned().collect();
    agent_keys.sort();
    assert_eq!(agent_keys, vec!["id".to_string(), "model_ref".to_string()]);
    assert!(doc.get("library").is_none());
    assert!(doc.get("mcp").is_none());
    assert!(doc.get("webdav").is_none());
    assert!(doc.get("skills").is_none());
    assert!(!gateway.parent().unwrap().join("profiles.json").exists());

    let before = fs::read(&gateway).unwrap();
    let long_name = "n".repeat(65);
    assert_eq!(
        save_profile(&long_name, &[agent("opencode", "openai/gpt-test")]).unwrap_err(),
        SaveProfileError::Name
    );
    assert_eq!(
        save_profile("secret", &[agent("opencode", "https://api.openai.com/v1")]).unwrap_err(),
        SaveProfileError::Store
    );
    assert_eq!(fs::read(&gateway).unwrap(), before);

    apply_profile("work").unwrap();
    assert!(!home.join("skills").exists());
    assert!(!home.join(".mcp").exists());
    assert!(!home.join("webdav").exists());
    let after: Value = serde_json::from_slice(&fs::read(&gateway).unwrap()).unwrap();
    assert!(after.get("library").is_none());
    assert!(after.get("mcp").is_none());
}

fn agent(id: &str, model_ref: &str) -> ProfileAgent {
    ProfileAgent {
        id: id.to_string(),
        model_ref: model_ref.to_string(),
    }
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
        fs::create_dir_all(&data).unwrap();
        let addr = PathBuf::from("127.0.0.1:21847");
        let pairs = [
            ("HOME", home.as_path()),
            ("USERPROFILE", home.as_path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", home.as_path()),
            ("SKILLSTAR_DATA_DIR", data.as_path()),
            ("SKILLSTAR_GATEWAY_ADDR", addr.as_path()),
        ];
        let saved = pairs
            .into_iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe { std::env::set_var(key, value) };
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

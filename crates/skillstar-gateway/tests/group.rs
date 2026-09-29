use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use serde_json::{Value, json};
use skillstar_gateway::{
    RouteMode, RouteOwner, SaveGroupError, ServedModel, expand_group, save_group, stored_route_mode,
};

#[test]
fn group_expands_members() {
    let _lock = lock_gateway_env();
    let root = scratch("expand");
    let _env = EnvRestore::sandbox(&root);
    let path = gateway_path(&root);
    let raw = fs::read(fixture_path("outer.json")).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &raw).unwrap();

    assert_eq!(
        expand_group("group/outer", &[]),
        vec!["a/m".to_string(), "b/m".to_string(), "c/m".to_string()],
        "outer lists a/m, then inner, and inner already has c/m"
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        raw,
        "expand does not rewrite the file"
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, "keep"),
        RouteMode::Order
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Group, "inner"),
        RouteMode::Rotate
    );

    save_group("extra", &["z/m"]).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["providers"][0]["routing"], "order");
    assert_eq!(doc["providers"][0]["note"], "stay");
    assert_eq!(doc["groups"][0]["members"], json!(["b/m", "c/m"]));
    assert_eq!(doc["groups"][0]["routing"], "rotate");
    assert_eq!(doc["groups"][1]["id"], "outer");
    assert_eq!(doc["groups"][1]["members"][2], "c/m");
    assert_eq!(doc["groups"][2]["id"], "extra");
    assert_eq!(doc["groups"][2]["members"], json!(["z/m"]));
    assert_key_table_untouched(&root);
}

#[test]
fn group_rejects_cycle() {
    let _lock = lock_gateway_env();
    let root = scratch("cycle");
    let _env = EnvRestore::sandbox(&root);
    let path = gateway_path(&root);

    assert_eq!(
        save_group("solo", &["group/solo"]),
        Err(SaveGroupError::Cycle)
    );
    assert!(!path.exists(), "a rejected save does not create the file");

    save_group("inner", &["a/m"]).unwrap();
    save_group("outer", &["b/m", "group/inner"]).unwrap();
    let saved = fs::read(&path).unwrap();
    assert_eq!(
        save_group("inner", &["a/m", "group/outer"]),
        Err(SaveGroupError::Cycle)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        saved,
        "a cycle leaves the file untouched"
    );
    assert_eq!(expand_group("group/inner", &[]), vec!["a/m".to_string()]);
    assert_eq!(
        expand_group("group/outer", &[]),
        vec!["b/m".to_string(), "a/m".to_string()]
    );
    assert_key_table_untouched(&root);
}

#[test]
fn group_rejects_depth_9() {
    let _lock = lock_gateway_env();
    let root = scratch("depth");
    let _env = EnvRestore::sandbox(&root);
    let path = gateway_path(&root);

    save_group("g0", &["a/m"]).unwrap();
    let mut previous = "g0".to_string();
    for level in 1..=8 {
        let id = format!("g{level}");
        let member = format!("group/{previous}");
        save_group(&id, &[&member]).unwrap();
        previous = id;
    }
    assert_eq!(
        expand_group("group/g8", &[]),
        vec!["a/m".to_string()],
        "eight levels still reach the model"
    );
    let saved = fs::read(&path).unwrap();
    let member = format!("group/{previous}");
    assert_eq!(save_group("g9", &[&member]), Err(SaveGroupError::TooDeep));
    assert_eq!(
        fs::read(&path).unwrap(),
        saved,
        "the ninth level does not write"
    );
    assert_eq!(expand_group("group/g8", &[]), vec!["a/m".to_string()]);
    assert_key_table_untouched(&root);
}

#[test]
fn group_auto_same_name_is_not_persisted_until_edit() {
    let _lock = lock_gateway_env();
    let root = scratch("auto");
    let _env = EnvRestore::sandbox(&root);
    let path = gateway_path(&root);
    let fixture = fixture("same-name.json");
    let models = served(&fixture);

    assert_eq!(
        expand_group(fixture["group"].as_str().unwrap(), &models),
        strings(&fixture["members"])
    );
    assert_eq!(
        expand_group(fixture["apart"]["group"].as_str().unwrap(), &models),
        strings(&fixture["apart"]["members"])
    );
    assert!(!path.exists(), "a derived group is not written");

    save_group("auto-claude-opus-5-5", &["claude/claude-opus-5-5"]).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["groups"].as_array().unwrap().len(), 1);
    assert_eq!(doc["groups"][0]["id"], "auto-claude-opus-5-5");
    assert_eq!(
        doc["groups"][0]["members"],
        json!(["claude/claude-opus-5-5"])
    );
    assert!(doc["groups"][0].get("auto").is_none());
    assert_eq!(
        expand_group(fixture["group"].as_str().unwrap(), &models),
        vec!["claude/claude-opus-5-5".to_string()],
        "the saved group wins over the derived one"
    );
    assert_eq!(
        expand_group(fixture["apart"]["group"].as_str().unwrap(), &[]),
        Vec::<String>::new(),
        "the variant nobody edited stays off disk"
    );
    assert_eq!(
        expand_group(fixture["apart"]["group"].as_str().unwrap(), &models),
        strings(&fixture["apart"]["members"])
    );
    assert_key_table_untouched(&root);
}

#[test]
fn group_cuts_a_written_loop() {
    let _lock = lock_gateway_env();
    let root = scratch("loop");
    let _env = EnvRestore::sandbox(&root);
    let path = gateway_path(&root);
    write_gateway(
        &path,
        &json!({
            "groups": [
                {"id": "p", "members": ["group/q", "a/m"]},
                {"id": "q", "members": ["group/p", "b/m"]}
            ]
        }),
    );
    assert_eq!(
        expand_group("group/p", &[]),
        vec!["b/m".to_string(), "a/m".to_string()]
    );
    assert_key_table_untouched(&root);
}

fn fixture(name: &str) -> Value {
    serde_json::from_slice(&fs::read(fixture_path(name)).unwrap()).unwrap()
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie/group")
        .join(name)
}

fn served(fixture: &Value) -> Vec<ServedModel<'_>> {
    fixture["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|model| ServedModel {
            id: model["id"].as_str().unwrap(),
            model: model["model"].as_str().unwrap(),
            provider_id: model["provider_id"].as_str().unwrap(),
        })
        .collect()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect()
}

fn gateway_path(root: &Path) -> PathBuf {
    root.join("data").join("config").join("model_gateway.json")
}

fn write_gateway(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn assert_key_table_untouched(root: &Path) {
    assert!(!root.join("data").join("model_providers.json").exists());
    assert!(
        !root
            .join("data")
            .join("config")
            .join("model_providers.json")
            .exists()
    );
}

/// `config_dir()` reads the process-wide `SKILLSTAR_DATA_DIR`.
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
        fs::create_dir_all(&data).unwrap();
        let pairs = [
            ("HOME", home.as_path()),
            ("USERPROFILE", home.as_path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", home.as_path()),
            ("SKILLSTAR_DATA_DIR", data.as_path()),
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

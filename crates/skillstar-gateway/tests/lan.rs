//! LAN listen keeps the address written for agents on loopback.

use std::ffi::OsString;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use serde_json::Value;
use skillstar_gateway::{
    ADDR_ENV, CodexRoute, REFUSED_PORT, ServeError, ServeOptions, apply_agent, apply_gateway,
    published_origin, resolve_addr, save_listen, serve,
};

#[test]
fn lan_off_listens_loopback() {
    let _lock = lock_gateway_env();
    let root = scratch("lan-off");
    let _env = EnvRestore::sandbox(&root, None);
    assert_eq!(resolve_addr().unwrap(), addr("127.0.0.1:21847"));
    assert_eq!(published_origin(), "http://127.0.0.1:21847");

    save_listen("loopback").unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(gateway_file(&root)).unwrap()).unwrap();
    assert!(doc.get("listen").is_none());
    assert_eq!(resolve_addr().unwrap(), addr("127.0.0.1:21847"));
}

#[test]
fn lan_on_listens_unspecified_and_publishes_loopback() {
    let _lock = lock_gateway_env();
    let root = scratch("lan-on");
    let _env = EnvRestore::sandbox(&root, Some("127.0.0.1:3499"));
    fs::write(
        gateway_file(&root),
        b"{\"profiles\":[{\"name\":\"work\",\"agents\":[]}]}",
    )
    .unwrap();

    save_listen("lan").unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(gateway_file(&root)).unwrap()).unwrap();
    assert_eq!(doc["listen"], "lan");
    assert_eq!(doc["profiles"][0]["name"], "work");
    assert_eq!(resolve_addr().unwrap(), addr("0.0.0.0:3499"));
    assert_eq!(published_origin(), "http://127.0.0.1:3499");
}

#[test]
fn lan_refuses_3425() {
    let _lock = lock_gateway_env();
    let root = scratch("lan-port");
    let _env = EnvRestore::sandbox(&root, Some(&format!("127.0.0.1:{REFUSED_PORT}")));
    save_listen("lan").unwrap();

    let resolved = resolve_addr().unwrap_err();
    assert!(matches!(resolved, ServeError::RefusedPort));
    assert!(matches!(ServeOptions::from_env(), Err(ServeError::RefusedPort)));
    let bound = serve(ServeOptions::bind(addr(&format!("0.0.0.0:{REFUSED_PORT}")))).unwrap_err();
    assert!(matches!(bound, ServeError::RefusedPort));
}

#[test]
fn lan_agent_file_stays_loopback() {
    let _lock = lock_gateway_env();
    let root = scratch("lan-file");
    let _env = EnvRestore::sandbox(&root, Some("127.0.0.1:21847"));
    let home = root.join("home");
    save_listen("lan").unwrap();
    assert_eq!(resolve_addr().unwrap(), addr("0.0.0.0:21847"));

    apply_gateway("opencode", "openai/gpt-test").unwrap();
    let opencode = fs::read_to_string(home.join(".config/opencode/opencode.json")).unwrap();
    assert!(opencode.contains("http://127.0.0.1:21847"), "{opencode}");
    assert!(!opencode.contains("0.0.0.0"), "{opencode}");

    let codex = home.join(".codex");
    fs::create_dir_all(&codex).unwrap();
    fs::write(codex.join("config.toml"), "model = \"user\"\n").unwrap();
    apply_agent("codex", CodexRoute::Api, &published_origin(), &home).unwrap();
    let toml = fs::read_to_string(codex.join("config.toml")).unwrap();
    assert!(toml.contains("http://127.0.0.1:21847"), "{toml}");
    assert!(!toml.contains("0.0.0.0"), "{toml}");
}

fn addr(raw: &str) -> SocketAddr {
    raw.parse().unwrap()
}

fn gateway_file(root: &Path) -> PathBuf {
    let path = root.join("data").join("config").join("model_gateway.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    path
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
    fn sandbox(root: &Path, addr: Option<&str>) -> Self {
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let mut pairs = vec![
            ("HOME", Some(home.to_string_lossy().into_owned())),
            ("USERPROFILE", Some(home.to_string_lossy().into_owned())),
            ("SKILLSTAR_TOOL_SYNC_HOME", Some(home.to_string_lossy().into_owned())),
            ("SKILLSTAR_DATA_DIR", Some(data.to_string_lossy().into_owned())),
        ];
        pairs.push((ADDR_ENV, addr.map(str::to_string)));
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

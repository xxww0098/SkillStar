//! The LAN inbound gate and the install-level gateway key: pure-function
//! behavior, lazy creation, failure shapes, and zero change on loopback. The
//! sandbox pattern copies tests/lan.rs; the real `$HOME` is never written.

use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use skillstar_gateway::{
    ADDR_ENV, SaveListenError, ServeError, ServeOptions, check_inbound, gateway_key, save_listen,
    serve,
};

fn lock_gateway_env() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn scratch(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
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
    fn sandbox(root: &Path, addr: Option<&str>) -> Self {
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let mut pairs = vec![
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

fn key_file(root: &Path) -> PathBuf {
    root.join("data").join("config").join("gateway.key")
}

/// Make the key impossible to generate or read: gateway.key becomes a
/// directory.
fn break_key(root: &Path) {
    let path = key_file(root);
    let _ = fs::remove_file(&path);
    fs::create_dir_all(&path).unwrap();
}

fn addr(raw: &str) -> SocketAddr {
    raw.parse().unwrap()
}

fn lan_peer() -> SocketAddr {
    addr("192.168.1.9:51000")
}

#[test]
fn check_inbound_loopback_peer_passes_without_a_key() {
    let _lock = lock_gateway_env();
    let root = scratch("access-loopback");
    let _env = EnvRestore::sandbox(&root, None);
    // Loopback passes without reading or creating the key file: any bearer,
    // including none, goes through.
    for authorization in ["", "Bearer skillstar-codex", "Bearer wrong"] {
        assert!(
            check_inbound(
                &addr("127.0.0.1:52000"),
                authorization,
                "",
                "",
                ""
            ),
            "loopback should pass: {authorization}"
        );
    }
    assert!(!key_file(&root).exists(), "loopback must not create the key file");
}

#[test]
fn check_inbound_non_loopback_is_rejected_without_a_key() {
    let _lock = lock_gateway_env();
    let root = scratch("access-no-key");
    let _env = EnvRestore::sandbox(&root, None);
    assert!(!check_inbound(&lan_peer(), "Bearer anything", "", "", ""));
    assert!(!key_file(&root).exists(), "the gate only reads; it must not create the key");
}

#[test]
fn check_inbound_non_loopback_accepts_any_slot() {
    let _lock = lock_gateway_env();
    let root = scratch("access-slots");
    let _env = EnvRestore::sandbox(&root, None);
    let key = gateway_key().unwrap();

    // Each slot alone, carrying the key, passes.
    assert!(check_inbound(&lan_peer(), &format!("Bearer {key}"), "", "", ""));
    // An Authorization without the Bearer prefix is still the same slot.
    assert!(check_inbound(&lan_peer(), &key, "", "", ""));
    assert!(check_inbound(&lan_peer(), "", &key, "", ""));
    assert!(check_inbound(&lan_peer(), "", "", &key, ""));
    assert!(check_inbound(&lan_peer(), "", "", "", &key));
    // One matching slot is enough; a placeholder in the other slots is fine.
    assert!(check_inbound(&lan_peer(), "Bearer skillstar", &key, "", ""));

    // Nothing matches: rejected.
    assert!(!check_inbound(&lan_peer(), "Bearer skillstar", "", "", ""));
    assert!(!check_inbound(&lan_peer(), "", "skillstar", "skillstar", "skillstar"));
    assert!(!check_inbound(&lan_peer(), "", "", "", ""));
    // Half of the key does not pass.
    assert!(!check_inbound(&lan_peer(), &key[..key.len() - 1], "", "", ""));
}

#[test]
fn gateway_key_is_lazy_stable_and_private() {
    let _lock = lock_gateway_env();
    let root = scratch("access-lazy");
    let _env = EnvRestore::sandbox(&root, None);
    let path = key_file(&root);
    assert!(!path.exists(), "lazy: no file before the first call");

    let first = gateway_key().unwrap();
    assert!(first.len() >= 64, "hex of >=32 bytes: {}", first.len());
    assert_eq!(fs::read_to_string(&path).unwrap(), first, "the file is the key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "a fresh key must be 0600");
    }
    let second = gateway_key().unwrap();
    assert_eq!(first, second, "the second read returns the same value");
    assert_eq!(fs::read_to_string(&path).unwrap(), first);
}

#[test]
fn save_listen_lan_creates_the_key_and_fails_without_one() {
    let _lock = lock_gateway_env();
    let root = scratch("access-save");
    let _env = EnvRestore::sandbox(&root, Some("127.0.0.1:3498"));
    save_listen("lan").unwrap();
    assert!(key_file(&root).exists(), "switching to lan lazily creates the key");

    // A sandbox with no key and no way to make one: gateway.key is a
    // directory, so it can be neither read nor written.
    break_key(&root);
    let error = save_listen("lan").unwrap_err();
    assert_eq!(error, SaveListenError::Key);
    assert_eq!(error.to_string(), "listen_key");
    // Loopback needs no key and still saves.
    save_listen("loopback").unwrap();
}

#[test]
fn serve_lan_without_a_key_returns_lan_needs_key() {
    let _lock = lock_gateway_env();
    let root = scratch("access-serve-lan");
    let _env = EnvRestore::sandbox(&root, None);
    break_key(&root);
    let error = serve(ServeOptions::bind(addr("0.0.0.0:0"))).unwrap_err();
    assert!(matches!(error, ServeError::LanNeedsKey), "{error}");
    assert!(error.to_string().contains("密钥"), "{}", error);
}

#[test]
fn serve_loopback_still_answers_without_a_key() {
    let _lock = lock_gateway_env();
    let root = scratch("access-serve-loopback");
    let _env = EnvRestore::sandbox(&root, None);
    break_key(&root);
    // With no usable key, loopback still serves: the local GET surface is
    // untouched by the LAN gate.
    let (tx, rx) = std::sync::mpsc::channel();
    let options = ServeOptions::bind(addr("127.0.0.1:0")).on_bound(tx);
    let stop = options.stop_handle();
    let handle = thread::spawn(move || serve(options));
    let bound = rx.recv_timeout(Duration::from_secs(20)).unwrap();

    let mut sock = TcpStream::connect(bound).unwrap();
    let header = format!(
        "GET /api/hello HTTP/1.1\r\nHost: {bound}\r\nConnection: close\r\n\r\n"
    );
    sock.write_all(header.as_bytes()).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let mut head = String::new();
    let mut buf = [0u8; 1024];
    loop {
        match sock.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                head.push_str(&String::from_utf8_lossy(&buf[..n]));
                if head.contains("\r\n\r\n") {
                    break;
                }
            }
            Err(error) => panic!("{error}"),
        }
    }
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(head.contains("skillstar"), "{head}");

    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn error_paths_do_not_leak_the_key() {
    let _lock = lock_gateway_env();
    let root = scratch("access-debug");
    let _env = EnvRestore::sandbox(&root, None);
    let key = gateway_key().unwrap();
    break_key(&root);

    let listen_error = save_listen("lan").unwrap_err();
    let serve_error = serve(ServeOptions::bind(addr("0.0.0.0:0"))).unwrap_err();
    for text in [
        format!("{listen_error}"),
        format!("{listen_error:?}"),
        format!("{serve_error}"),
        format!("{serve_error:?}"),
    ] {
        assert!(!text.contains(&key), "error output leaked the key: {text}");
    }
}

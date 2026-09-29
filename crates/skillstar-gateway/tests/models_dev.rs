//! models.dev cache path, and the directories a refresh must not write.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use skillstar_gateway::{MODELS_DEV_URL, models_dev_load, models_dev_sync};

const FIXTURE: &[u8] = br#"{"openai":{"id":"openai","models":{"gpt-test":{"id":"gpt-test"}}}}"#;

fn gate() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "skillstar-models-dev-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct EnvRestore {
    saved: Vec<(String, Option<std::ffi::OsString>)>,
}

impl EnvRestore {
    fn set(pairs: &[(&str, &std::ffi::OsStr)]) -> Self {
        let saved = pairs
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe { std::env::set_var(key, value) };
                ((*key).to_string(), previous)
            })
            .collect();
        Self { saved }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, previous) in self.saved.drain(..) {
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(&key, value),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}

fn real_cache_before_sandbox() -> (PathBuf, Option<Vec<u8>>) {
    let root = std::env::var_os("SKILLSTAR_DATA_DIR").filter(|value| !value.is_empty());
    let path = match root {
        Some(dir) => PathBuf::from(dir).join("cache/gateway-catalog/models.dev.json"),
        None => {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".skillstar/cache/gateway-catalog/models.dev.json")
        }
    };
    let bytes = fs::read(&path).ok();
    (path, bytes)
}

fn with_sandbox(label: &str, body: impl FnOnce(&Path, &Path)) {
    let _gate = gate();
    let (real_path, real_bytes) = real_cache_before_sandbox();
    let root = Tmp::new(label);
    let home = root.path.join("home");
    let data = root.path.join("data");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data).unwrap();
    let _env = EnvRestore::set(&[
        ("HOME", home.as_os_str()),
        ("USERPROFILE", home.as_os_str()),
        ("SKILLSTAR_TOOL_SYNC_HOME", home.as_os_str()),
        ("SKILLSTAR_DATA_DIR", data.as_os_str()),
    ]);
    body(&data, &root.path);
    assert_eq!(
        fs::read(&real_path).ok(),
        real_bytes,
        "refresh wrote {}",
        real_path.display()
    );
}

struct Fake {
    url: String,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Fake {
    fn serve(status: u16, body: &'static [u8]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let join = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => reply(stream, status, body),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url: format!("http://127.0.0.1:{port}/api.json"),
            stop,
            join: Some(join),
        }
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn reply(mut stream: TcpStream, status: u16, body: &[u8]) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut ignore = [0_u8; 1024];
    let _ = stream.read(&mut ignore);
    let header = format!(
        "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn closed_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}/api.json")
}

#[test]
fn models_dev_cache_path() {
    with_sandbox("path", |data, _root| {
        assert_eq!(MODELS_DEV_URL, "https://models.dev/api.json");
        let path = skillstar_gateway::models_dev_cache_path();
        assert_eq!(
            path,
            data.join("cache").join("gateway-catalog").join("models.dev.json")
        );
        assert!(models_dev_load().is_empty());
        assert!(!path.exists());
        let fake = Fake::serve(200, FIXTURE);
        models_dev_sync(&fake.url).unwrap();
        assert_eq!(fs::read(&path).unwrap(), FIXTURE);
        assert_eq!(models_dev_load(), FIXTURE);
    });
}

#[test]
fn models_dev_does_not_touch_provider_meta() {
    with_sandbox("meta", |data, _root| {
        let providers = data.join("config").join("model_providers.json");
        fs::create_dir_all(providers.parent().unwrap()).unwrap();
        let store = br#"{"version":4,"providers":[{"id":"openai","meta":{"model_catalog":[{"id":"keep"}]}}]}"#;
        fs::write(&providers, store).unwrap();
        let old = data.join("cache").join("model_catalog").join("openai.json");
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::write(&old, b"{\"id\":\"keep\"}\n").unwrap();
        let store_bytes = fs::read(&providers).unwrap();
        let old_bytes = fs::read(&old).unwrap();
        let store_mtime = fs::metadata(&providers).unwrap().modified().unwrap();
        let old_mtime = fs::metadata(&old).unwrap().modified().unwrap();

        let fake = Fake::serve(200, FIXTURE);
        models_dev_sync(&fake.url).unwrap();

        assert_eq!(fs::read(&providers).unwrap(), store_bytes);
        assert_eq!(fs::read(&old).unwrap(), old_bytes);
        assert_eq!(fs::metadata(&providers).unwrap().modified().unwrap(), store_mtime);
        assert_eq!(fs::metadata(&old).unwrap().modified().unwrap(), old_mtime);
        assert!(!data.join("cache").join("model_catalog").join("models.dev.json").exists());
        assert!(skillstar_gateway::models_dev_cache_path().is_file());
    });
}

#[test]
fn models_dev_respects_data_dir() {
    with_sandbox("data-dir", |data, root| {
        let fake = Fake::serve(200, FIXTURE);
        models_dev_sync(&fake.url).unwrap();
        let first = skillstar_gateway::models_dev_cache_path();
        assert!(first.starts_with(data));
        let kept = fs::read(&first).unwrap();

        let other = root.join("other-data");
        fs::create_dir_all(&other).unwrap();
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &other) };
        assert_eq!(
            skillstar_gateway::models_dev_cache_path(),
            other.join("cache").join("gateway-catalog").join("models.dev.json")
        );
        assert!(models_dev_load().is_empty());
        assert!(!skillstar_gateway::models_dev_cache_path().exists());
        assert_eq!(fs::read(&first).unwrap(), kept);

        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", data) };
        let broken = Fake::serve(500, b"nope");
        assert!(models_dev_sync(&broken.url).is_err());
        assert_eq!(fs::read(&first).unwrap(), kept);
        assert!(models_dev_sync(&closed_url()).is_err());
        assert_eq!(fs::read(&first).unwrap(), kept);
        let junk = Fake::serve(200, b"[]");
        assert!(models_dev_sync(&junk.url).is_err());
        assert_eq!(fs::read(skillstar_gateway::models_dev_cache_path()).unwrap(), kept);
    });
}

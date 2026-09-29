//! Alma's local API while it runs. A closed port writes nothing and is success.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;
use skillstar_gateway::apply_gateway;

const REF: &str = "deepseek/pro";
const URL: &str = "http://127.0.0.1:21847/v1";

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
            "skillstar-alma-{label}-{}-{nanos}",
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

fn gate() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn with_home(label: &str, body: impl FnOnce(&Path, &Path)) {
    let _gate = gate();
    let root = Tmp::new(label);
    let home = root.path.join("home");
    let data = root.path.join("data");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data).unwrap();
    let addr = std::ffi::OsString::from("127.0.0.1:21847");
    let timeout = std::ffi::OsString::from("1000");
    let _env = EnvRestore::set(&[
        ("HOME", home.as_os_str()),
        ("USERPROFILE", home.as_os_str()),
        ("SKILLSTAR_TOOL_SYNC_HOME", home.as_os_str()),
        ("SKILLSTAR_DATA_DIR", data.as_os_str()),
        ("SKILLSTAR_GATEWAY_ADDR", addr.as_os_str()),
        ("SKILLSTAR_ALMA_URL", std::ffi::OsStr::new("")),
        ("SKILLSTAR_ALMA_TIMEOUT_MS", timeout.as_os_str()),
    ]);
    body(&home, &data);
}

fn set_alma(url: &str, timeout_ms: &str) {
    unsafe {
        std::env::set_var("SKILLSTAR_ALMA_URL", url);
        std::env::set_var("SKILLSTAR_ALMA_TIMEOUT_MS", timeout_ms);
    }
}

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        out.push((rel, fs::read(&path).unwrap_or_default()));
    }
}

struct Hit {
    method: String,
    path: String,
    body: Vec<u8>,
}

struct State {
    providers: Vec<Value>,
    settings: Value,
    hits: Vec<Hit>,
    next_id: u32,
}

struct Fake {
    port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Fake {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(State {
            providers: vec![serde_json::json!({
                "id": "own",
                "name": "My OpenAI",
                "type": "openai",
                "baseURL": "https://api.openai.com/v1",
                "enabled": true,
                "apiKey": "encrypted"
            })],
            settings: serde_json::json!({
                "general": { "theme": "dark" },
                "chat": { "defaultModel": "own:gpt-4o", "temperature": 1 }
            }),
            hits: Vec::new(),
            next_id: 1,
        }));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let state_thread = Arc::clone(&state);
        let stop_thread = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            while !stop_thread.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((sock, _)) => serve(sock, &state_thread),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            port,
            state,
            stop,
            thread: Some(thread),
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut sock: TcpStream, state: &Mutex<State>) {
    let _ = sock.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match sock.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                let Some(pos) = buf.windows(4).position(|window| window == b"\r\n\r\n") else {
                    continue;
                };
                let head = String::from_utf8_lossy(&buf[..pos]).into_owned();
                let mut body = buf[pos + 4..].to_vec();
                let need = content_length(&head);
                while body.len() < need {
                    match sock.read(&mut tmp) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => body.extend_from_slice(&tmp[..n]),
                    }
                }
                body.truncate(need);
                let mut start = head.split("\r\n").next().unwrap_or("").split_whitespace();
                let method = start.next().unwrap_or("").to_string();
                let path = start.next().unwrap_or("").to_string();
                let payload = answer(state, &method, &path, &body);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = sock.write_all(header.as_bytes());
                let _ = sock.write_all(&payload);
                break;
            }
        }
    }
}

fn answer(state: &Mutex<State>, method: &str, path: &str, body: &[u8]) -> Vec<u8> {
    let mut state = state.lock().unwrap();
    state.hits.push(Hit {
        method: method.to_string(),
        path: path.to_string(),
        body: body.to_vec(),
    });
    let parsed: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    if path == "/api/providers" && method == "GET" {
        return serde_json::to_vec(&state.providers).unwrap();
    }
    if path == "/api/providers" && method == "POST" {
        state.next_id += 1;
        let id = format!("p{}", state.next_id);
        let mut provider = parsed;
        if let Some(object) = provider.as_object_mut() {
            object.insert("id".to_string(), Value::String(id));
        }
        state.providers.push(provider.clone());
        return serde_json::to_vec(&provider).unwrap();
    }
    if path == "/api/settings" && method == "GET" {
        return serde_json::to_vec(&state.settings).unwrap();
    }
    if path == "/api/settings" && method == "PUT" {
        state.settings = parsed;
        return serde_json::to_vec(&state.settings).unwrap();
    }
    if let Some(id) = path.strip_prefix("/api/providers/") {
        if let Some(id) = id.strip_suffix("/models")
            && method == "PUT"
        {
            if let Some(provider) = state.providers.iter_mut().find(|provider| {
                provider.get("id").and_then(Value::as_str) == Some(id)
            }) && let Some(object) = provider.as_object_mut()
            {
                if let Some(models) = parsed.get("models") {
                    object.insert("models".to_string(), models.clone());
                }
                if let Some(known) = parsed.get("availableModels") {
                    object.insert("availableModels".to_string(), known.clone());
                }
            }
            return b"{}".to_vec();
        }
        if method == "PUT"
            && let Some(provider) = state
                .providers
                .iter_mut()
                .find(|provider| provider.get("id").and_then(Value::as_str) == Some(id))
            && let Some(object) = provider.as_object_mut()
            && let Some(fields) = parsed.as_object()
        {
            for (key, value) in fields {
                object.insert(key.clone(), value.clone());
            }
            return b"{}".to_vec();
        }
        if method == "PUT" {
            return b"{}".to_vec();
        }
        if method == "DELETE" {
            state
                .providers
                .retain(|provider| provider.get("id").and_then(Value::as_str) != Some(id));
            return b"{}".to_vec();
        }
    }
    b"{}".to_vec()
}

fn content_length(head: &str) -> usize {
    head.split("\r\n")
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn skillstar(providers: &[Value]) -> &Value {
    providers
        .iter()
        .find(|provider| provider.get("name").and_then(Value::as_str) == Some("skillstar"))
        .expect("skillstar provider")
}

#[test]
fn alma_live_sets_provider_and_default_model() {
    with_home("live", |home, data| {
        let before_home = snapshot(home);
        let before_data = snapshot(data);
        let fake = Fake::start();
        set_alma(&fake.url(), "1000");
        apply_gateway("alma", REF).unwrap();
        let state = fake.state.lock().unwrap();
        let provider = skillstar(&state.providers);
        let id = provider.get("id").and_then(Value::as_str).unwrap();
        assert_ne!(id, "own");
        assert_eq!(provider.get("type").and_then(Value::as_str), Some("openai"));
        assert_eq!(provider.get("baseURL").and_then(Value::as_str), Some(URL));
        assert_eq!(
            provider.get("apiKey").and_then(Value::as_str),
            Some("skillstar-alma")
        );
        assert_eq!(provider.get("enabled").and_then(Value::as_bool), Some(true));
        assert_eq!(
            provider.get("models").and_then(Value::as_array).map(|rows| rows.len()),
            Some(1)
        );
        let want = format!("{id}:{REF}");
        assert_eq!(state.settings["chat"]["defaultModel"].as_str(), Some(want.as_str()));
        assert_eq!(state.settings["chat"]["temperature"].as_u64(), Some(1));
        assert_eq!(state.settings["general"]["theme"].as_str(), Some("dark"));
        let own = state
            .providers
            .iter()
            .find(|provider| provider.get("id").and_then(Value::as_str) == Some("own"))
            .unwrap();
        assert_eq!(
            own.get("baseURL").and_then(Value::as_str),
            Some("https://api.openai.com/v1")
        );
        assert!(
            state
                .hits
                .iter()
                .all(|hit| !String::from_utf8_lossy(&hit.body).contains("magpie"))
        );
        assert!(state.hits.iter().any(|hit| hit.method == "POST" && hit.path == "/api/providers"));
        drop(state);
        assert_eq!(snapshot(home), before_home);
        assert_eq!(snapshot(data), before_data);

        apply_gateway("alma", "").unwrap();
        let state = fake.state.lock().unwrap();
        assert!(
            state
                .providers
                .iter()
                .all(|provider| provider.get("name").and_then(Value::as_str) != Some("skillstar"))
        );
        assert_eq!(state.providers.len(), 1);
        assert_eq!(state.settings["chat"]["defaultModel"].as_str(), Some(""));
        assert_eq!(state.settings["general"]["theme"].as_str(), Some("dark"));
        assert_eq!(snapshot(home), before_home);
    });
}

#[test]
fn alma_down_is_ok_and_writes_nothing() {
    with_home("down", |home, data| {
        apply_gateway("alma", REF).unwrap();
        assert!(snapshot(home).is_empty());
        assert!(snapshot(data).is_empty());

        set_alma("http://127.0.0.1:1", "200");
        apply_gateway("alma", REF).unwrap();
        assert!(snapshot(home).is_empty());
        assert!(snapshot(data).is_empty());

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            while !stop_thread.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut sock, _)) => {
                        let _ = sock.set_read_timeout(Some(Duration::from_millis(50)));
                        let mut buf = [0u8; 1024];
                        while !stop_thread.load(std::sync::atomic::Ordering::Relaxed) {
                            let _ = sock.read(&mut buf);
                            thread::sleep(Duration::from_millis(20));
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        set_alma(&format!("http://127.0.0.1:{port}"), "200");
        let started = Instant::now();
        apply_gateway("alma", REF).unwrap();
        let elapsed = started.elapsed();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", port));
        let _ = thread.join();
        assert!(elapsed < Duration::from_millis(900), "{elapsed:?}");
        assert!(snapshot(home).is_empty());
        assert!(snapshot(data).is_empty());
    });
}

#[test]
fn alma_does_not_touch_files() {
    with_home("files", |home, data| {
        let hanako = home.join(".hanako").join("provider-catalog.json");
        fs::create_dir_all(hanako.parent().unwrap()).unwrap();
        fs::write(&hanako, b"keep-hanako").unwrap();
        fs::write(home.join("sentinel.txt"), b"keep-sentinel").unwrap();
        let before_home = snapshot(home);
        let before_data = snapshot(data);
        let fake = Fake::start();
        set_alma(&fake.url(), "1000");
        apply_gateway("alma", REF).unwrap();
        assert_eq!(snapshot(home), before_home);
        assert_eq!(snapshot(data), before_data);
        assert_eq!(fs::read(&hanako).unwrap(), b"keep-hanako");
        let state = fake.state.lock().unwrap();
        assert!(state.hits.iter().any(|hit| hit.method == "POST"));
        assert_eq!(
            skillstar(&state.providers)
                .get("baseURL")
                .and_then(Value::as_str),
            Some(URL)
        );
        assert!(!URL.contains(":3425"));
        assert!(URL.ends_with("/v1"));
    });
}

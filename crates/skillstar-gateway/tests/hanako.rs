//! OpenHanako uses its local API while it runs, and files when it does not.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use skillstar_gateway::apply_gateway;

const REF: &str = "deepseek/pro";
const URL: &str = "http://127.0.0.1:21847/v1";
const TOKEN: &str = "hanako-test-token";

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
            "skillstar-hanako-{label}-{}-{nanos}",
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
    let wrong = root.path.join("wrong-hana");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&wrong).unwrap();
    let addr = std::ffi::OsString::from("127.0.0.1:21847");
    let _env = EnvRestore::set(&[
        ("HOME", home.as_os_str()),
        ("USERPROFILE", home.as_os_str()),
        ("SKILLSTAR_TOOL_SYNC_HOME", home.as_os_str()),
        ("SKILLSTAR_DATA_DIR", data.as_os_str()),
        ("SKILLSTAR_GATEWAY_ADDR", addr.as_os_str()),
        ("HANA_HOME", wrong.as_os_str()),
    ]);
    body(&home, &wrong);
}

fn plant_agent(home: &Path, yaml: &str) {
    let dir = home.join(".hanako");
    let agent = dir.join("agents").join("main");
    fs::create_dir_all(&agent).unwrap();
    fs::create_dir_all(dir.join("user")).unwrap();
    fs::write(
        dir.join("user").join("preferences.json"),
        "{\"primaryAgent\":\"main\"}\n",
    )
    .unwrap();
    fs::write(agent.join("config.yaml"), yaml).unwrap();
}

struct Hit {
    method: String,
    path: String,
    auth: String,
    body: Vec<u8>,
}

struct Fake {
    port: u16,
    hits: Arc<Mutex<Vec<Hit>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Fake {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hits_thread = Arc::clone(&hits);
        let stop_thread = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            while !stop_thread.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((sock, _)) => record(sock, &hits_thread),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            port,
            hits,
            stop,
            thread: Some(thread),
        }
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

fn record(mut sock: TcpStream, hits: &Mutex<Vec<Hit>>) {
    let _ = sock.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    loop {
        match sock.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if let Some(pos) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
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
                    let mut lines = head.split("\r\n");
                    let mut start = lines.next().unwrap_or("").split_whitespace();
                    let method = start.next().unwrap_or("").to_string();
                    let path = start.next().unwrap_or("").to_string();
                    let auth = head
                        .split("\r\n")
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("authorization")
                                .then(|| value.trim().to_string())
                        })
                        .unwrap_or_default();
                    let identity = path == "/api/server/identity";
                    hits.lock().unwrap().push(Hit {
                        method,
                        path,
                        auth,
                        body,
                    });
                    let response = if identity {
                        let payload = b"{\"serverId\":\"test\"}";
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            payload.len()
                        )
                        .into_bytes()
                        .into_iter()
                        .chain(payload.iter().copied())
                        .collect::<Vec<_>>()
                    } else {
                        b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    };
                    let _ = sock.write_all(&response);
                    break;
                }
            }
        }
    }
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

fn catalog(home: &Path) -> PathBuf {
    home.join(".hanako").join("provider-catalog.json")
}

fn yaml(home: &Path) -> PathBuf {
    home.join(".hanako")
        .join("agents")
        .join("main")
        .join("config.yaml")
}

#[test]
fn hanako_live_uses_local_api() {
    with_home("live", |home, wrong| {
        let prior = "theme: kept\n";
        plant_agent(home, prior);
        let fake = Fake::start();
        let info = home.join(".hanako").join("server-info.json");
        fs::write(
            &info,
            format!("{{\"port\":{},\"token\":\"{TOKEN}\"}}\n", fake.port),
        )
        .unwrap();
        apply_gateway("hanako", REF).unwrap();
        assert_eq!(fs::read_to_string(yaml(home)).unwrap(), prior);
        assert!(!catalog(home).exists());
        assert!(!wrong.join("provider-catalog.json").exists());
        let hits = fake.hits.lock().unwrap();
        let methods: Vec<_> = hits
            .iter()
            .map(|hit| format!("{} {}", hit.method, hit.path))
            .collect();
        assert!(
            methods.iter().any(|line| line == "GET /api/server/identity"),
            "{methods:?}"
        );
        assert!(
            methods.iter().any(|line| line == "PUT /api/config"),
            "{methods:?}"
        );
        assert!(
            methods
                .iter()
                .any(|line| line == "PUT /api/agents/main/config"),
            "{methods:?}"
        );
        assert!(hits.iter().all(|hit| hit.auth == format!("Bearer {TOKEN}")));
        let config = hits
            .iter()
            .find(|hit| hit.path == "/api/config")
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&config.body).unwrap();
        assert_eq!(
            parsed["providers"]["skillstar"]["base_url"].as_str(),
            Some(URL)
        );
        assert_eq!(
            parsed["providers"]["skillstar"]["api_key"].as_str(),
            Some("skillstar-hanako")
        );
        assert_eq!(
            parsed["providers"]["skillstar"]["models"][0]["id"].as_str(),
            Some(REF)
        );
        let chat = hits
            .iter()
            .find(|hit| hit.path == "/api/agents/main/config")
            .unwrap();
        let chat: serde_json::Value = serde_json::from_slice(&chat.body).unwrap();
        assert_eq!(chat["models"]["chat"]["provider"].as_str(), Some("skillstar"));
        assert_eq!(chat["models"]["chat"]["id"].as_str(), Some(REF));
        assert!(!String::from_utf8_lossy(&config.body).contains("magpie"));
    });
}

#[test]
fn hanako_stopped_writes_files() {
    with_home("stopped", |home, wrong| {
        let err = apply_gateway("hanako", REF).unwrap_err();
        assert!(err.to_string().contains("no agent"), "{err}");
        assert!(!home.join(".hanako").exists(), "wrote before an agent existed");
        plant_agent(home, "theme: kept\n");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        fs::write(
            home.join(".hanako").join("server-info.json"),
            format!("{{\"port\":{port},\"token\":\"{TOKEN}\"}}\n"),
        )
        .unwrap();
        let plugin = home
            .join(".hanako")
            .join("provider-plugins")
            .join("skillstar")
            .join("providers");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(plugin.join("skillstar.json"), "{\"models\":[]}\n").unwrap();
        apply_gateway("hanako", REF).unwrap();
        let text = fs::read_to_string(catalog(home)).unwrap();
        assert!(text.contains(&format!("\"base_url\": \"{URL}\"")), "{text}");
        assert!(text.contains("\"api_key\": \"skillstar-hanako\""), "{text}");
        assert!(!text.contains("api.openai.com"), "{text}");
        assert!(!text.contains("magpie"), "{text}");
        assert!(!text.contains("theme"), "{text}");
        let agent = fs::read_to_string(yaml(home)).unwrap();
        assert!(agent.contains("provider: \"skillstar\""), "{agent}");
        assert!(agent.contains("id: \"deepseek/pro\""), "{agent}");
        assert!(!agent.contains("theme"), "{agent}");
        assert!(!plugin.join("skillstar.json").exists());
        assert!(!wrong.join("provider-catalog.json").exists());
        apply_gateway("hanako", "").unwrap();
        assert_eq!(fs::read_to_string(yaml(home)).unwrap(), "theme: kept\n");
    });
}

#[test]
fn hanako_url_is_loopback_v1() {
    with_home("url", |home, _wrong| {
        plant_agent(home, "theme: kept\n");
        apply_gateway("hanako", REF).unwrap();
        let text = fs::read_to_string(catalog(home)).unwrap();
        assert!(text.contains(&format!("\"base_url\": \"{URL}\"")), "{text}");
        assert!(text.contains("/v1"), "{text}");
        assert!(!text.contains(":3425"), "{text}");
        assert!(!text.contains("api.anthropic.com"), "{text}");
    });
}

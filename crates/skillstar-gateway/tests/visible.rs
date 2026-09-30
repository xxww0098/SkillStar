//! An agent's visible list narrows what it is shown. A hidden id still answers.
//! Routing does not read the list, and a saved model ref stays in its field.

use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, mpsc};
use std::thread;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use skillstar_gateway::{
    RouteCandidate, RouteMode, RouteOwner, ServeOptions, apply_gateway, route_mode, serve,
    shown_model_ids, stored_route_mode,
};

#[test]
fn visible_missing_agent_sees_all() {
    let _lock = lock_gateway_env();
    let root = scratch("visible-missing");
    let _env = EnvRestore::sandbox(&root);
    write_catalog();
    write_gateway(&json!({
        "visible": {"opencode": ["relay"]},
        "providers": [{"id": "relayco", "family": "relay"}],
        "groups": [{"id": "fast", "family": "relay", "members": ["relayco/m1"]}]
    }));
    assert_eq!(
        shown_model_ids("pi"),
        vec![
            "openai/gpt-test".to_string(),
            "relayco/m1".to_string(),
            "group/fast".to_string(),
        ]
    );
}

#[test]
fn visible_empty_list_sees_all() {
    let _lock = lock_gateway_env();
    let root = scratch("visible-empty");
    let _env = EnvRestore::sandbox(&root);
    write_catalog();
    write_gateway(&json!({
        "visible": {"opencode": []},
        "providers": [{"id": "relayco", "family": "relay"}]
    }));
    assert_eq!(
        shown_model_ids("opencode"),
        vec!["openai/gpt-test".to_string(), "relayco/m1".to_string()]
    );
}

#[test]
fn visible_filters_models_list_and_written_catalog() {
    let _lock = lock_gateway_env();
    let root = scratch("visible-filter");
    let env = EnvRestore::sandbox(&root);
    write_catalog();
    write_gateway(&json!({
        "visible": {"opencode": ["relay"]},
        "providers": [{"id": "relayco", "family": "relay"}],
        "groups": [{"id": "fast", "family": "other", "members": ["relayco/m1"]}]
    }));
    let gateway = Gateway::open(br#"{"ok":true}"#.to_vec());
    let (status, body) = gateway.send(
        "GET",
        "/v1/models",
        "Authorization: Bearer skillstar-opencode\r\n",
    );
    assert_eq!(status, 200);
    let ids = model_ids(&body);
    assert_eq!(ids, vec!["relayco/m1".to_string()]);
    assert!(!String::from_utf8_lossy(&body).contains("https://"));
    assert!(!String::from_utf8_lossy(&body).contains("sk-"));

    let (status, body) = gateway.send("GET", "/v1/models", "User-Agent: pi\r\n");
    assert_eq!(status, 200);
    assert_eq!(
        model_ids(&body),
        vec![
            "openai/gpt-test".to_string(),
            "relayco/m1".to_string(),
            "group/fast".to_string(),
        ]
    );

    apply_gateway("opencode", "openai/gpt-test").unwrap();
    let text = fs::read_to_string(env.home().join(".config/opencode/opencode.json")).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["model"], "skillstar/openai/gpt-test");
    let models = doc["provider"]["skillstar"]["models"].as_object().unwrap();
    assert_eq!(models.keys().cloned().collect::<Vec<_>>(), vec!["relayco/m1"]);
    assert!(!text.contains("https://"));
    assert!(!text.contains("sk-"));
    gateway.stop();
}

#[test]
fn visible_hidden_model_still_answers() {
    let _lock = lock_gateway_env();
    let root = scratch("visible-hidden");
    let _env = EnvRestore::sandbox(&root);
    write_catalog();
    write_gateway(&json!({
        "visible": {"opencode": ["relay"]},
        "providers": [{"id": "relayco", "family": "relay"}]
    }));
    let gateway = Gateway::open(br#"{"ok":true}"#.to_vec());
    let before = gateway.outbound_hits();
    let (status, _) = gateway.send(
        "POST",
        "/v1/chat/completions",
        "Authorization: Bearer skillstar-opencode\r\n",
    );
    assert_eq!(status, 200);
    let seen = gateway.next_hit();
    assert_eq!(seen.path, "/v1/chat/completions");
    let posted: Value = serde_json::from_slice(&seen.body).unwrap();
    assert_eq!(posted["model"], "openai/gpt-test");
    assert!(gateway.outbound_hits() > before);

    let (status, body) = gateway.send_body(
        "POST",
        "/v1/chat/completions",
        "Authorization: Bearer skillstar-opencode\r\n",
        br#"{"model":"missing/nope"}"#,
    );
    assert_eq!(status, 404);
    assert!(String::from_utf8_lossy(&body).contains("missing/nope"));
    gateway.stop();
}

#[test]
fn visible_does_not_change_saved_ref_or_route() {
    let _lock = lock_gateway_env();
    let root = scratch("visible-route");
    let env = EnvRestore::sandbox(&root);
    write_catalog();
    write_gateway(&json!({
        "visible": {"opencode": ["relay"]},
        "providers": [{"id": "relayco", "family": "relay", "routing": "rotate"}],
        "groups": [{"id": "fast", "members": ["relayco/m1"], "routing": "order"}]
    }));
    assert_eq!(
        stored_route_mode(RouteOwner::Provider, "relayco"),
        RouteMode::Rotate
    );
    assert_eq!(
        stored_route_mode(RouteOwner::Group, "fast"),
        RouteMode::Order
    );
    let candidates = [
        RouteCandidate {
            id: "relayco/m1",
            allowance: None,
        },
        RouteCandidate {
            id: "openai/gpt-test",
            allowance: None,
        },
    ];
    let (order, turn) = route_mode(RouteMode::Order, &candidates, 3);
    assert_eq!(order, vec!["relayco/m1".to_string(), "openai/gpt-test".to_string()]);
    assert_eq!(turn, 3);

    apply_gateway("opencode", "openai/gpt-test").unwrap();
    let text = fs::read_to_string(env.home().join(".config/opencode/opencode.json")).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["model"], "skillstar/openai/gpt-test");
    let again = fs::read_to_string(skillstar_core_gateway()).unwrap();
    assert!(again.contains("\"routing\": \"rotate\""), "{again}");
    assert!(again.contains("\"visible\""), "{again}");
}

fn skillstar_core_gateway() -> PathBuf {
    // cache/gateway-catalog/models.dev.json → data root → config/model_gateway.json
    skillstar_gateway::models_dev_cache_path()
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .unwrap()
        .join("config")
        .join("model_gateway.json")
}

fn catalog_bytes() -> &'static [u8] {
    br#"{"openai":{"models":{"gpt-test":{"id":"gpt-test","url":"https://vendor.example/m"}}},"relayco":{"models":{"m1":{"id":"m1"}}}}"#
}

fn write_catalog() {
    let cache = skillstar_gateway::models_dev_cache_path();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, catalog_bytes()).unwrap();
}

fn write_gateway(doc: &Value) {
    let path = skillstar_core_gateway();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(doc).unwrap()).unwrap();
}

fn model_ids(body: &[u8]) -> Vec<String> {
    let doc: Value = serde_json::from_slice(body).unwrap();
    doc["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_string())
        .collect()
}

struct Seen {
    path: String,
    body: Vec<u8>,
}

struct Gateway {
    addr: SocketAddr,
    fake: SocketAddr,
    hits: mpsc::Receiver<Seen>,
    stop_flag: skillstar_gateway::Stop,
    handle: Option<thread::JoinHandle<Result<(), skillstar_gateway::ServeError>>>,
}

impl Gateway {
    fn open(upstream_response: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let fake = listener.local_addr().unwrap();
        let (tx, hits) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let Ok((mut sock, _)) = listener.accept() else {
                    break;
                };
                sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
                let msg = read_http(&mut sock);
                let _ = tx.send(Seen {
                    path: request_target(&msg.start),
                    body: msg.body,
                });
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    upstream_response.len()
                );
                let _ = sock.write_all(header.as_bytes());
                let _ = sock.write_all(&upstream_response);
            }
        });
        let (bound_tx, bound_rx) = mpsc::channel();
        let options = ServeOptions::bind("127.0.0.1:0".parse().unwrap())
            .upstream(format!("http://{fake}"))
            .on_bound(bound_tx);
        let stop_flag = options.stop_handle();
        let handle = thread::spawn(move || serve(options));
        let addr = bound_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        Self {
            addr,
            fake,
            hits,
            stop_flag,
            handle: Some(handle),
        }
    }

    fn send(&self, method: &str, path: &str, extra: &str) -> (u16, Vec<u8>) {
        let body: &[u8] = if method == "POST" {
            br#"{"model":"openai/gpt-test"}"#
        } else {
            b""
        };
        self.send_body(method, path, extra, body)
    }

    fn send_body(&self, method: &str, path: &str, extra: &str, body: &[u8]) -> (u16, Vec<u8>) {
        let mut sock = TcpStream::connect(self.addr).unwrap();
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n{extra}\r\n",
            addr = self.addr,
            len = body.len(),
        );
        sock.write_all(head.as_bytes()).unwrap();
        sock.write_all(body).unwrap();
        let msg = read_http(&mut sock);
        (status_code(&msg.start), msg.body)
    }

    fn outbound_hits(&self) -> usize {
        let needle = format!("http://{}", self.fake);
        skillstar_gateway::outbound_log()
            .iter()
            .filter(|url| url.contains(&needle))
            .count()
    }

    fn next_hit(&self) -> Seen {
        self.hits.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    fn stop(&self) {
        self.stop_flag.stop();
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.stop_flag.stop();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct HttpMsg {
    start: String,
    body: Vec<u8>,
}

fn read_http(sock: &mut TcpStream) -> HttpMsg {
    sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        if let Some(msg) = split_http(&buf, false) {
            return msg;
        }
        match sock.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(error)
                if error.kind() == std::io::ErrorKind::TimedOut
                    || error.kind() == std::io::ErrorKind::WouldBlock =>
            {
                break;
            }
            Err(error) => panic!("{error}"),
        }
    }
    split_http(&buf, true).unwrap_or_else(|| panic!("no http message in {buf:?}"))
}

fn split_http(buf: &[u8], eof: bool) -> Option<HttpMsg> {
    let split = buf.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..split]).ok()?;
    let mut length = None;
    for line in head.split("\r\n").skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().ok();
        }
    }
    let rest = &buf[split + 4..];
    let length = match length {
        Some(length) => length,
        None if eof => rest.len(),
        None => return None,
    };
    if rest.len() < length {
        return None;
    }
    Some(HttpMsg {
        start: head.lines().next()?.to_string(),
        body: rest[..length].to_vec(),
    })
}

fn status_code(start: &str) -> u16 {
    start.split_whitespace().nth(1).unwrap().parse().unwrap()
}

fn request_target(start: &str) -> String {
    start.lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
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
    let path = std::env::temp_dir().join(format!(
        "skillstar-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

struct EnvRestore {
    saved: Vec<(&'static str, Option<OsString>)>,
    root: PathBuf,
    home: PathBuf,
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
            ("SKILLSTAR_GATEWAY_ADDR", Some("127.0.0.1:21847".to_string())),
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
            home,
        }
    }

    fn home(&self) -> &Path {
        &self.home
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

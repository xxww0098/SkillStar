//! One request per route on `skillstar_gateway::serve`.
//!
//! `POST /v1/chat/completions` stays in `serve_chat_fixture_matches_translate`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use skillstar_gateway::{
    Protocol, ServeError, ServeOptions, outbound_body, outbound_log, serve, upstream_body,
};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct IsolatedDataDir {
    previous: Option<std::ffi::OsString>,
    dir: std::path::PathBuf,
}

impl IsolatedDataDir {
    fn new() -> Self {
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        let dir =
            std::env::temp_dir().join(format!("skillstar-gateway-surface-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &dir) };
        Self { previous, dir }
    }
}

impl Drop for IsolatedDataDir {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Seen {
    path: String,
    body: Vec<u8>,
}

struct Gateway {
    addr: SocketAddr,
    fake: SocketAddr,
    hits: mpsc::Receiver<Seen>,
    stop: skillstar_gateway::Stop,
    handle: Option<thread::JoinHandle<Result<(), ServeError>>>,
}

impl Gateway {
    fn open(upstream_response: Vec<u8>) -> Self {
        Self::open_status(200, upstream_response)
    }

    fn open_status(status: u16, upstream_response: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let fake = listener.local_addr().unwrap();
        let (tx, hits) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let Ok((mut sock, _)) = listener.accept() else {
                    break;
                };
                sock.set_read_timeout(Some(Duration::from_secs(20))).ok();
                let msg = read_http(&mut sock);
                let _ = tx.send(Seen {
                    path: request_target(&msg.start),
                    body: msg.body,
                });
                let header = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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
        let stop = options.stop_handle();
        let handle = thread::spawn(move || serve(options));
        let addr = bound_rx
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|error| panic!("listener did not bind: {error}"));
        Self {
            addr,
            fake,
            hits,
            stop,
            handle: Some(handle),
        }
    }

    fn send(&self, method: &str, path: &str, extra_headers: &str, body: &[u8]) -> (u16, Vec<u8>) {
        let mut sock = TcpStream::connect(self.addr).unwrap();
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n{extra_headers}\r\n",
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
        outbound_log()
            .iter()
            .filter(|url| url.contains(&needle) || url.contains("api.openai.com"))
            .count()
    }

    fn assert_no_upstream(&self, before: usize) {
        assert_eq!(
            self.outbound_hits(),
            before,
            "this request noted an outbound URL"
        );
        if let Ok(seen) = self.hits.recv_timeout(Duration::from_millis(200)) {
            panic!(
                "fake upstream accepted {} {}",
                seen.path,
                String::from_utf8_lossy(&seen.body)
            );
        }
    }

    fn next_hit(&self) -> Seen {
        self.hits
            .recv_timeout(Duration::from_secs(20))
            .expect("fake upstream was not contacted")
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.stop.stop();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn with_gateway(response: &[u8], test: impl FnOnce(&Gateway)) {
    let _lock = env_lock();
    let _data = IsolatedDataDir::new();
    let gateway = Gateway::open(response.to_vec());
    test(&gateway);
}

fn with_status(status: u16, response: &[u8], test: impl FnOnce(&Gateway)) {
    let _lock = env_lock();
    let _data = IsolatedDataDir::new();
    let gateway = Gateway::open_status(status, response.to_vec());
    test(&gateway);
}

struct HttpMsg {
    start: String,
    body: Vec<u8>,
}

fn read_http(sock: &mut TcpStream) -> HttpMsg {
    sock.set_read_timeout(Some(Duration::from_secs(20))).ok();
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
    start
        .split_whitespace()
        .nth(1)
        .unwrap_or_else(|| panic!("status line {start}"))
        .parse()
        .unwrap()
}

fn request_target(start: &str) -> String {
    start
        .split_whitespace()
        .nth(1)
        .unwrap_or_else(|| panic!("request line {start}"))
        .to_string()
}

fn json_body(body: &[u8]) -> Value {
    serde_json::from_slice(body)
        .unwrap_or_else(|error| panic!("json {error}: {}", String::from_utf8_lossy(body)))
}

fn error_message(body: &[u8]) -> String {
    json_body(body)["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

fn fixture(group: &str, name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie/translate")
        .join(group)
        .join(name);
    let mut bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    if bytes.ends_with(b"\n") {
        bytes.pop();
        if bytes.ends_with(b"\r") {
            bytes.pop();
        }
    }
    bytes
}

#[test]
fn hello_name_is_skillstar() {
    with_gateway(b"{}", |gateway| {
        let before = gateway.outbound_hits();
        let (status, body) = gateway.send("GET", "/api/hello", "", b"");
        assert_eq!(status, 200, "hello");
        assert_eq!(
            json_body(&body),
            json!({"name": "skillstar", "version": "dev"})
        );

        let (status, body) = gateway.send("HEAD", "/api/hello", "", b"");
        assert_eq!(status, 200, "head hello");
        assert!(body.is_empty(), "HEAD /api/hello carries no body");

        let (status, body) = gateway.send("GET", "/", "", b"");
        assert_eq!(status, 200, "info");
        let info = json_body(&body);
        assert_eq!(info["name"], "skillstar");
        assert_eq!(info["version"], "dev");
        assert_eq!(info["models"], 0);
        assert!(info.get("window").is_none(), "info has no window field");
        let apis: Vec<&str> = info["apis"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect();
        assert_eq!(
            apis,
            [
                "/v1/chat/completions",
                "/v1/responses",
                "/v1/messages",
                "/v1beta/models/{model}:generateContent",
                "/v1/images/generations",
                "/v1/images/edits",
            ]
        );
        assert!(apis.iter().all(|path| !path.contains("quotas")));
        gateway.assert_no_upstream(before);
    });
}

#[test]
fn quotas_route_is_absent() {
    with_gateway(b"{}", |gateway| {
        let before = gateway.outbound_hits();
        for path in ["/v1/magpie/quotas", "/v1/skillstar/quotas", "/no/such"] {
            let (status, body) = gateway.send("GET", path, "", b"");
            assert_eq!(status, 404, "{path}");
            let message = error_message(&body);
            assert!(
                message.contains("skillstar serves"),
                "{path} body {message}"
            );
            assert!(!message.contains("quotas"), "{path} advertises quotas");
            assert_eq!(json_body(&body)["error"]["type"], "not_found_error");
        }
        gateway.assert_no_upstream(before);
    });
}

#[test]
fn model_lists_are_empty() {
    with_gateway(b"{}", |gateway| {
        let before = gateway.outbound_hits();
        for path in ["/v1/models", "/models"] {
            let (status, body) = gateway.send("GET", path, "", b"");
            assert_eq!(status, 200, "{path}");
            assert_eq!(
                json_body(&body),
                json!({"object": "list", "data": [], "has_more": false})
            );
        }
        for path in ["/v1beta/models", "/backend-api/codex/models"] {
            let (status, body) = gateway.send("GET", path, "", b"");
            assert_eq!(status, 200, "{path}");
            assert_eq!(json_body(&body), json!({"models": []}));
        }
        let (status, body) = gateway.send("GET", "/v1/models/openai/gpt-4o", "", b"");
        assert_eq!(status, 404);
        assert!(error_message(&body).contains("openai/gpt-4o"));
        let (status, _) = gateway.send("GET", "/backend-api/codex", "", b"");
        assert_eq!(status, 404, "codex path without the trailing slash");
        gateway.assert_no_upstream(before);
    });
}

#[test]
fn chat_alias_matches_fixture() {
    let inbound = fixture("chat-passthrough", "inbound.json");
    let upstream_response = fixture("chat-passthrough", "upstream_response.json");
    let expected_upstream = upstream_body(Protocol::Chat, &inbound).unwrap();
    let expected_outbound = outbound_body(Protocol::Chat, &upstream_response).unwrap();
    with_gateway(&upstream_response, |gateway| {
        let (status, outbound) = gateway.send("POST", "/chat/completions", "", &inbound);
        assert_eq!(status, 200);
        let seen = gateway.next_hit();
        assert_eq!(seen.path, "/v1/chat/completions");
        assert_eq!(seen.body, expected_upstream);
        assert_eq!(outbound, expected_outbound);
    });
}

#[test]
fn messages_fixture_matches_translate() {
    let inbound = fixture("anthropic-tool-call", "inbound.json");
    let upstream_response = fixture("anthropic-tool-call", "upstream_response.json");
    let expected_upstream = upstream_body(Protocol::Anthropic, &inbound).unwrap();
    let expected_outbound = outbound_body(Protocol::Anthropic, &upstream_response).unwrap();
    assert_eq!(
        expected_upstream,
        fixture("anthropic-tool-call", "upstream_request.json")
    );
    assert_eq!(
        expected_outbound,
        fixture("anthropic-tool-call", "outbound.json")
    );
    with_gateway(&upstream_response, |gateway| {
        for path in ["/v1/messages", "/messages"] {
            let (status, outbound) = gateway.send("POST", path, "", &inbound);
            assert_eq!(status, 200, "{path}");
            let seen = gateway.next_hit();
            assert_eq!(seen.path, "/v1/chat/completions", "{path}");
            assert_eq!(seen.body, expected_upstream, "{path}");
            assert_eq!(outbound, expected_outbound, "{path}");
        }
    });
}

#[test]
fn raw_routes_forward_the_body() {
    let body = br#"{"model":"m1","ping":true}"#;
    let upstream = br#"{"ok":true}"#;
    with_gateway(upstream, |gateway| {
        let paths = [
            "/v1/responses",
            "/responses",
            "/v1/images/generations",
            "/images/generations",
            "/v1/images/edits",
            "/images/edits",
            "/v1beta/models/gemini-2.0-flash:generateContent",
            "/v1beta/models/llama3:8b:streamGenerateContent",
            "/backend-api/codex/responses",
            "/backend-api/codex/responses/compact",
        ];
        for path in paths {
            let (status, outbound) = gateway.send("POST", path, "", body);
            assert_eq!(status, 200, "{path}");
            assert_eq!(outbound, upstream, "{path}");
            let seen = gateway.next_hit();
            let expected_path = if let Some(rest) = path.strip_prefix("/backend-api/codex") {
                format!("/v1{rest}")
            } else {
                path.to_string()
            };
            assert_eq!(seen.path, expected_path, "{path}");
            if path == "/backend-api/codex/responses" {
                assert_eq!(
                    serde_json::from_slice::<Value>(&seen.body).unwrap(),
                    json!({
                        "model": "m1",
                        "ping": true,
                        "prompt_cache_key": "skillstar-chatgpt",
                        "store": false,
                        "stream": true,
                    }),
                    "{path}"
                );
            } else {
                assert_eq!(seen.body, body, "{path}");
            }
            let logged = outbound_log();
            let expected_url = format!("http://{}{expected_path}", gateway.fake);
            assert!(
                logged.iter().any(|url| url == &expected_url),
                "{path} missing from outbound log {logged:?}"
            );
            assert!(
                logged.iter().all(|url| !url.contains("api.openai.com")),
                "forward used the configured origin, not api.openai.com: {logged:?}"
            );
        }
    });
}

#[test]
fn codex_responses_is_posted_as_a_public_responses_body() {
    let inbound = br#"{
        "model": "gpt-6-sol-fast",
        "temperature": 0.2,
        "service_tier": "flex",
        "max_output_tokens": 16,
        "session_id": "sess/1",
        "prompt_cache_key": "keep",
        "input": [
            {"type": "message", "role": "system", "content": "rules"},
            {"role": "system", "content": "bare"},
            {"role": "user", "content": "hi"},
            {"type": "function_call", "role": "system", "name": "lookup"}
        ]
    }"#;
    with_gateway(br#"{"ok":true}"#, |gateway| {
        let (status, outbound) =
            gateway.send("POST", "/backend-api/codex/responses", "", inbound);
        assert_eq!(status, 200);
        assert_eq!(outbound, br#"{"ok":true}"#);
        let seen = gateway.next_hit();
        assert_eq!(seen.path, "/v1/responses");
        assert_eq!(
            serde_json::from_slice::<Value>(&seen.body).unwrap(),
            json!({
                "model": "gpt-6-sol",
                "service_tier": "priority",
                "prompt_cache_key": "keep",
                "store": false,
                "stream": true,
                "input": [
                    {"type": "message", "role": "developer", "content": "rules"},
                    {"role": "developer", "content": "bare"},
                    {"role": "user", "content": "hi"},
                    {"type": "function_call", "role": "system", "name": "lookup"}
                ]
            })
        );
        let logged = outbound_log();
        assert!(
            logged.iter().all(|url| !url.contains("api.openai.com")),
            "the public Responses shape still posts to the configured origin: {logged:?}"
        );
    });
}

#[test]
fn codex_plan_limit_names_the_usage_settings_page() {
    let limit = br#"{"error":{"message":"slow down","code":"subscription_sharing_usage_limit_exceeded"}}"#;
    with_status(429, limit, |gateway| {
        let (status, body) =
            gateway.send("POST", "/backend-api/codex/responses", "", br#"{"model":"m"}"#);
        assert_eq!(status, 429);
        assert_eq!(
            json_body(&body)["error"]["message"],
            "slow down — manage usage at https://chatgpt.com/settings/usage"
        );

        let (status, body) = gateway.send("POST", "/v1/responses", "", br#"{"model":"m"}"#);
        assert_eq!(status, 429);
        assert_eq!(json_body(&body)["error"]["message"], "slow down");
    });
}

#[test]
fn codex_other_errors_pass_through() {
    let other = br#"{"error":{"code":"rate_limit_exceeded","message":"later"}}"#;
    with_status(429, other, |gateway| {
        let (status, body) =
            gateway.send("POST", "/backend-api/codex/responses", "", br#"{"model":"m"}"#);
        assert_eq!(status, 429);
        assert_eq!(json_body(&body), json_body(other));
    });
}

#[test]
fn count_tokens_is_local() {
    with_gateway(br#"{"ok":true}"#, |gateway| {
        let before = gateway.outbound_hits();
        let body = br#"{"model":"anthropic/claude","messages":[]}"#;
        let (status, response) = gateway.send("POST", "/v1/messages/count_tokens", "", body);
        assert_eq!(status, 200);
        assert_eq!(json_body(&response)["input_tokens"], body.len() / 4);

        let (status, response) =
            gateway.send("POST", "/v1beta/models/gemini-2.0:countTokens", "", body);
        assert_eq!(status, 200);
        assert_eq!(json_body(&response)["totalTokens"], body.len() / 4);
        let (status, response) =
            gateway.send("POST", "/v1beta/models/openai/gpt:countTokens", "", body);
        assert_eq!(status, 200);
        assert_eq!(json_body(&response)["totalTokens"], body.len() / 4);

        let (status, response) = gateway.send("POST", "/v1/messages/count_tokens", "", b"not-json");
        assert_eq!(status, 400);
        assert_eq!(
            json_body(&response)["error"]["type"],
            "invalid_request_error"
        );
        gateway.assert_no_upstream(before);
    });
}

#[test]
fn claude_callback_route_answers() {
    with_gateway(b"{}", |gateway| {
        let before = gateway.outbound_hits();
        let (status, body) =
            gateway.send("POST", "/_skillstar/claude-mcp/missing-token", "", b"{}");
        assert_eq!(status, 404);
        assert_eq!(
            String::from_utf8_lossy(&body),
            "unknown or expired Claude run"
        );
        gateway.assert_no_upstream(before);
    });
}

#[test]
fn chat_invalid_json_still_forwards() {
    let body = b"not-json";
    let upstream = br#"{"id":"x"}"#;
    with_gateway(upstream, |gateway| {
        let (status, outbound) = gateway.send("POST", "/v1/chat/completions", "", body);
        assert_eq!(status, 200);
        assert_eq!(outbound, upstream);
        let seen = gateway.next_hit();
        assert_eq!(seen.path, "/v1/chat/completions");
        assert_eq!(seen.body, body);
    });
}

#[test]
fn slash_model_stays_local() {
    with_gateway(br#"{"ok":true}"#, |gateway| {
        let before = gateway.outbound_hits();
        let posts = [
            (
                "/v1/chat/completions",
                br#"{"model":"openai/gpt-4o"}"#.as_slice(),
            ),
            (
                "/v1/messages",
                br#"{"model":"anthropic/claude"}"#.as_slice(),
            ),
            ("/v1/responses", br#"{"model":"openai/gpt-4o"}"#.as_slice()),
            (
                "/v1/images/generations",
                br#"{"model":"openai/gpt-image"}"#.as_slice(),
            ),
            (
                "/v1beta/models/openai/gpt:generateContent",
                br#"{"contents":[]}"#.as_slice(),
            ),
        ];
        for (path, body) in posts {
            let (status, response) = gateway.send("POST", path, "", body);
            assert_eq!(status, 404, "{path}");
            let message = error_message(&response);
            assert!(
                message.contains("skillstar knows no model"),
                "{path}: {message}"
            );
            assert!(
                !message.to_ascii_lowercase().contains("magpie"),
                "{message}"
            );
        }
        let (status, response) =
            gateway.send("POST", "/v1beta/models/gemini-2.0", "", br#"{"x":1}"#);
        assert_eq!(status, 404);
        assert!(
            error_message(&response).contains("expected /v1beta/models/{model}:generateContent")
        );
        let (status, response) =
            gateway.send("POST", "/v1beta/models/gemini-2.0:nope", "", br#"{"x":1}"#);
        assert_eq!(status, 404);
        assert!(error_message(&response).contains("unknown method nope"));
        gateway.assert_no_upstream(before);
    });
}

#[test]
fn codex_slash_model_does_not_reach_openai() {
    with_gateway(br#"{"ok":true}"#, |gateway| {
        let before = gateway.outbound_hits();
        let (status, body) = gateway.send(
            "POST",
            "/backend-api/codex/responses",
            "",
            br#"{"model":"openai/gpt-4o"}"#,
        );
        assert_eq!(status, 404);
        let message = error_message(&body);
        assert!(message.contains("skillstar knows no model"));
        assert!(message.contains("openai/gpt-4o"));

        let (status, body) = gateway.send(
            "POST",
            "/backend-api/codex/responses/compact",
            "",
            br#"{"model":"openai/gpt-4o"}"#,
        );
        assert_eq!(status, 400);
        let message = error_message(&body);
        assert!(message.contains("/responses/compact is not supported for skillstar models"));
        assert!(!message.contains("Magpie") && !message.contains("magpie"));
        gateway.assert_no_upstream(before);
    });
}

#[test]
fn codex_websocket_is_426() {
    with_gateway(br#"{"ok":true}"#, |gateway| {
        let before = gateway.outbound_hits();
        for upgrade in ["Upgrade: websocket\r\n", "Upgrade: WebSocket\r\n"] {
            let (status, body) = gateway.send("GET", "/backend-api/codex/responses", upgrade, b"");
            assert_eq!(status, 426, "{upgrade}");
            let text = String::from_utf8_lossy(&body);
            assert!(text.contains("skillstar speaks HTTP"), "{text}");
            assert!(!text.to_ascii_lowercase().contains("magpie"));
        }
        let (status, _) = gateway.send(
            "GET",
            "/backend-api/codex/models",
            "Upgrade: websocket\r\n",
            b"",
        );
        assert_eq!(status, 426, "upgrade is answered before the model list");
        let (status, _) = gateway.send("GET", "/backend-api/codex", "Upgrade: websocket\r\n", b"");
        assert_eq!(status, 404, "upgrade on the unsuffixed codex path");
        gateway.assert_no_upstream(before);
    });
}

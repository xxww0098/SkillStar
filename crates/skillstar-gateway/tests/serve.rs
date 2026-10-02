use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::Duration;

use skillstar_gateway::{
    ADDR_ENV, DEFAULT_ADDR, HEADER_READ_TIMEOUT, IDLE_TIMEOUT, PLACEHOLDER_BEARER, Protocol,
    REFUSED_PORT, ServeError, ServeOptions, clear_recent_calls, outbound_body, recent_calls,
    resolve_addr, serve, upstream_body,
};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn start(
    options: ServeOptions,
) -> (
    SocketAddr,
    skillstar_gateway::Stop,
    thread::JoinHandle<Result<(), ServeError>>,
) {
    let (tx, rx) = mpsc::channel();
    let options = options.on_bound(tx);
    let stop = options.stop_handle();
    let handle = thread::spawn(move || serve(options));
    let addr = match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(addr) => addr,
        Err(mpsc::RecvTimeoutError::Disconnected) => match handle.join() {
            Ok(Err(error)) => panic!("listener did not bind: {error}"),
            Ok(Ok(())) => panic!("listener returned before binding"),
            Err(_) => panic!("listener thread panicked before binding"),
        },
        Err(error) => panic!("listener did not bind: {error}"),
    };
    (addr, stop, handle)
}

/// Keep the chat forward off the developer's real `proxy.json`.
struct IsolatedDataDir {
    previous: Option<std::ffi::OsString>,
    dir: std::path::PathBuf,
}

impl IsolatedDataDir {
    fn new() -> Self {
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        let dir =
            std::env::temp_dir().join(format!("skillstar-gateway-serve-{}", std::process::id()));
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

/// Keep the chat forward off the developer's real `proxy.json`.
struct LedgerSandbox {
    previous: Option<std::ffi::OsString>,
    dir: std::path::PathBuf,
}

impl LedgerSandbox {
    /// A `SKILLSTAR_DATA_DIR` unique to this instance, so one test's ledger
    /// reads see only that test's turns. Served turns append to whatever the
    /// variable says at the moment, so every test that flips it — these
    /// included — holds `env_lock`.
    fn new(label: &str) -> Self {
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "skillstar-gateway-{label}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &dir) };
        Self { previous, dir }
    }

    fn usage_lines(&self) -> Vec<serde_json::Value> {
        let path = self.dir.join("gateway").join("usage.jsonl");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        text.lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|error| panic!("a bad ledger line ({error}): {line}"))
            })
            .collect()
    }
}

impl Drop for LedgerSandbox {
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

/// One canned upstream reply.
struct Reply {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

/// A fake upstream that answers `count` requests with the same reply.
fn fake_upstream(count: usize, answer: Reply) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for _ in 0..count {
            let Ok((mut sock, _)) = listener.accept() else {
                break;
            };
            let _ = read_message(&mut sock);
            let header = format!(
                "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                answer.status,
                answer.content_type,
                answer.body.len()
            );
            sock.write_all(header.as_bytes()).unwrap();
            sock.write_all(&answer.body).unwrap();
        }
    });
    addr
}

/// POST one turn, return the response head and body.
fn post_turn(
    addr: SocketAddr,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (String, Vec<u8>) {
    let mut sock = TcpStream::connect(addr).unwrap();
    let mut head = format!("POST {path} HTTP/1.1\r\nHost: {addr}\r\n");
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str(&format!(
        "Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    sock.write_all(head.as_bytes()).unwrap();
    sock.write_all(body).unwrap();
    read_http(&mut sock)
}

fn read_message(sock: &mut TcpStream) -> Vec<u8> {
    read_http(sock).1
}

/// Read one HTTP response: its head text and its body.
fn read_http(sock: &mut TcpStream) -> (String, Vec<u8>) {
    sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        if let Some(parts) = split_http(&buf) {
            return parts;
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
    split_http(&buf).unwrap_or_else(|| panic!("no http body in {buf:?}"))
}

fn split_http(buf: &[u8]) -> Option<(String, Vec<u8>)> {
    let split = buf.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..split]).ok()?.to_string();
    let mut length = None;
    for line in head.split("\r\n").skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().ok();
        }
    }
    let length = length?;
    let body = &buf[split + 4..];
    if body.len() < length {
        return None;
    }
    Some((head, body[..length].to_vec()))
}

fn status_of(head: &str) -> u16 {
    head.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie/translate/chat-passthrough")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn serve_binds_default_port() {
    let _guard = env_lock();
    let _data = IsolatedDataDir::new();
    let previous = std::env::var(ADDR_ENV).ok();
    unsafe {
        std::env::remove_var(ADDR_ENV);
    }

    assert_eq!(DEFAULT_ADDR, "127.0.0.1:21847");
    assert_eq!(PLACEHOLDER_BEARER, "skillstar");
    assert_eq!(HEADER_READ_TIMEOUT, Duration::from_secs(30));
    assert_eq!(IDLE_TIMEOUT, Duration::from_secs(5 * 60));
    assert_eq!(resolve_addr().unwrap(), "127.0.0.1:21847".parse().unwrap());

    let (addr, stop, handle) = start(ServeOptions::from_env().unwrap());
    assert_eq!(addr, "127.0.0.1:21847".parse().unwrap());
    TcpStream::connect(addr).expect("default port accepts a connection");
    stop.stop();
    handle.join().unwrap().unwrap();

    if let Some(previous) = previous {
        unsafe { std::env::set_var(ADDR_ENV, previous) };
    }
}

#[test]
fn serve_refuses_magpie_port() {
    let _guard = env_lock();
    let previous = std::env::var(ADDR_ENV).ok();
    unsafe {
        std::env::set_var(ADDR_ENV, format!("127.0.0.1:{REFUSED_PORT}"));
    }

    let resolved = resolve_addr().unwrap_err();
    assert!(matches!(resolved, ServeError::RefusedPort));

    let held = TcpListener::bind(format!("127.0.0.1:{REFUSED_PORT}"));
    let refused = serve(ServeOptions::bind(
        format!("127.0.0.1:{REFUSED_PORT}").parse().unwrap(),
    ))
    .unwrap_err();
    assert!(
        matches!(refused, ServeError::RefusedPort),
        "refusing the port must happen before bind, got {refused}"
    );
    if let Ok(held) = &held {
        assert_eq!(held.local_addr().unwrap().port(), REFUSED_PORT);
    }

    unsafe {
        match previous {
            Some(previous) => std::env::set_var(ADDR_ENV, previous),
            None => std::env::remove_var(ADDR_ENV),
        }
    }
}

#[test]
fn serve_reports_address_in_use() {
    let (addr, stop, handle) = start(ServeOptions::bind("127.0.0.1:0".parse().unwrap()));
    let error = serve(ServeOptions::bind(addr)).unwrap_err();
    assert!(matches!(error, ServeError::Busy));
    assert_eq!(error.to_string(), "地址已被占用");
    TcpStream::connect(addr).expect("the first listener is still there");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn serve_chat_fixture_matches_translate() {
    // Turns now append to the ledger, so every test that points
    // SKILLSTAR_DATA_DIR somewhere holds the env lock while it serves.
    let _guard = env_lock();
    let _data = IsolatedDataDir::new();
    let inbound = fixture("inbound.json");
    let upstream_response = fixture("upstream_response.json");
    let expected_upstream = upstream_body(Protocol::Chat, &inbound).unwrap();
    let expected_outbound = outbound_body(Protocol::Chat, &upstream_response).unwrap();
    assert_eq!(expected_upstream, fixture("upstream_request.json"));
    assert_eq!(expected_outbound, fixture("outbound.json"));

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let fake_addr = listener.local_addr().unwrap();
    let (seen_tx, seen_rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let body = read_message(&mut sock);
        seen_tx.send(body).unwrap();
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            upstream_response.len()
        );
        sock.write_all(header.as_bytes()).unwrap();
        sock.write_all(&upstream_response).unwrap();
    });

    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );
    let mut sock = TcpStream::connect(addr).unwrap();
    let header = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        inbound.len()
    );
    sock.write_all(header.as_bytes()).unwrap();
    sock.write_all(&inbound).unwrap();
    let outbound = read_message(&mut sock);
    let seen = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    assert_eq!(seen, expected_upstream);
    assert_eq!(outbound, expected_outbound);
    let logged = skillstar_gateway::outbound_log();
    let forwarded = format!("http://{fake_addr}/v1/chat/completions");
    assert!(
        logged.iter().any(|url| url == &forwarded),
        "chat forward should be on the outbound log: {logged:?}"
    );
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn chat_fixture_is_a_recent_call() {
    let _guard = env_lock();
    // The ring is process-wide and the ledger tests also forward turns as
    // codex with this fixture; drop their entries so the find below can only
    // meet this test's own call.
    clear_recent_calls();
    let _data = IsolatedDataDir::new();
    let inbound = fixture("inbound.json");
    let upstream_response = fixture("upstream_response.json");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let fake_addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let _ = read_message(&mut sock);
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            upstream_response.len()
        );
        sock.write_all(header.as_bytes()).unwrap();
        sock.write_all(&upstream_response).unwrap();
    });

    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );
    let mut sock = TcpStream::connect(addr).unwrap();
    let header = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer skillstar-codex\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        inbound.len()
    );
    sock.write_all(header.as_bytes()).unwrap();
    sock.write_all(&inbound).unwrap();
    let _outbound = read_message(&mut sock);

    let call = recent_calls()
        .into_iter()
        .find(|call| call.agent == "codex" && call.model == "m1")
        .expect("the chat fixture should be on the ring");
    assert_eq!(call.status, 200);
    assert_eq!(call.completion_tokens, Some(5));
    let text = format!("{call:?}");
    assert!(!text.contains(&format!("http://{fake_addr}")), "{text}");
    assert!(!text.contains("https://"), "{text}");
    assert!(!text.contains("api.openai.com"), "{text}");
    assert!(!text.contains("sk-"), "{text}");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_200_turn_lands_one_ledger_line() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("ledger-200");
    let inbound = fixture("inbound.json");
    let upstream_response = fixture("upstream_response.json");
    let fake_addr = fake_upstream(
        1,
        Reply {
            status: "200 OK",
            content_type: "application/json",
            body: upstream_response.clone(),
        },
    );

    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );
    let (head, outbound) = post_turn(
        addr,
        "/v1/chat/completions",
        &[
            ("Authorization", "Bearer skillstar-codex"),
            ("X-Skillstar-Session", "s-ledger-200"),
        ],
        &inbound,
    );
    assert_eq!(status_of(&head), 200);
    assert_eq!(outbound, fixture("outbound.json"));

    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1, "one turn, one line");
    let record = &lines[0];
    assert_eq!(record["agent"], "codex");
    assert_eq!(
        record["session"], "s-ledger-200",
        "the session header is piped through"
    );
    assert_eq!(record["model_asked"], "m1");
    assert_eq!(record["model_answered"], "m1");
    assert_eq!(record["catalog"], "", "routing is not wired yet");
    assert_eq!(
        record["account"], "",
        "the skillstar-codex channel names no account"
    );
    assert_eq!(record["tokens"]["input"], 10);
    assert_eq!(record["tokens"]["output"], 5);
    assert_eq!(record["status"], 200);
    assert!(record["latency_ms"].as_u64().is_some());
    assert!(
        record["at"].as_i64().unwrap_or(0) > 1_700_000_000_000,
        "unix millis"
    );
    assert_eq!(record["error_kind"], serde_json::Value::Null);
    assert_eq!(record["endpoint"], "/v1/chat/completions");
    let text = format!("{record}");
    assert!(
        !text.contains("skillstar-codex"),
        "no bearer in the line: {text}"
    );
    assert!(!text.contains(&format!("http://{fake_addr}")), "{text}");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_body_without_session_headers_gets_a_derived_session() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("ledger-derived-session");
    let inbound = fixture("inbound.json");
    let fake_addr = fake_upstream(
        1,
        Reply {
            status: "200 OK",
            content_type: "application/json",
            body: fixture("upstream_response.json"),
        },
    );
    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );
    let _ = post_turn(
        addr,
        "/v1/chat/completions",
        &[("Authorization", "Bearer skillstar-codex")],
        &inbound,
    );

    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    let session = lines[0]["session"].as_str().unwrap();
    assert!(
        session.starts_with("skillstar-"),
        "body-derived id: {session}"
    );
    assert!(!session.is_empty());
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_presented_key_is_recorded_as_a_fingerprint_account() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("ledger-account");
    let inbound = fixture("inbound.json");
    let fake_addr = fake_upstream(
        1,
        Reply {
            status: "200 OK",
            content_type: "application/json",
            body: fixture("upstream_response.json"),
        },
    );
    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );
    let _ = post_turn(
        addr,
        "/v1/chat/completions",
        &[("Authorization", "Bearer sk-agent-key-value")],
        &inbound,
    );

    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    let account = lines[0]["account"].as_str().unwrap();
    assert!(account.starts_with("key:"), "fingerprint form: {account}");
    assert_eq!(account.len(), 4 + 8, "eight hex chars: {account}");
    let text = format!("{}", lines[0]);
    assert!(
        !text.contains("sk-agent-key-value"),
        "the key itself never lands: {text}"
    );
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_429_upstream_turn_lands_one_rate_limited_line() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("ledger-429");
    let inbound = fixture("inbound.json");
    let refusal =
        br#"{"error":{"message":"Too many requests","type":"rate_limit_error"}}"#.to_vec();
    let fake_addr = fake_upstream(
        1,
        Reply {
            status: "429 Too Many Requests",
            content_type: "application/json",
            body: refusal,
        },
    );
    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );
    let (head, _) = post_turn(
        addr,
        "/v1/chat/completions",
        &[("Authorization", "Bearer skillstar-codex")],
        &inbound,
    );
    assert_eq!(status_of(&head), 429);

    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["status"], 429);
    assert_eq!(lines[0]["error_kind"], "rate_limit");
    assert_eq!(lines[0]["tokens"]["input"], 0);
    assert_eq!(lines[0]["tokens"]["output"], 0);
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_502_without_an_upstream_lands_one_upstream_line() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("ledger-502");
    let (addr, stop, handle) = start(ServeOptions::bind("127.0.0.1:0".parse().unwrap()));
    let (head, _) = post_turn(
        addr,
        "/v1/chat/completions",
        &[],
        br#"{"model":"m1","messages":[{"role":"user","content":"hi"}]}"#,
    );
    assert_eq!(status_of(&head), 502);

    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["status"], 502);
    assert_eq!(lines[0]["error_kind"], "upstream");
    assert_eq!(lines[0]["agent"], "other");
    assert_eq!(lines[0]["model_asked"], "m1");
    assert_eq!(lines[0]["model_answered"], "");
    assert!(!lines[0]["session"].as_str().unwrap().is_empty());
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn an_sse_tail_frame_usage_is_recorded() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("ledger-sse");
    let inbound = br#"{"model":"m9","stream":true,"messages":[{"role":"user","content":"hi"}]}"#;
    let stream = concat!(
        "data: {\"id\":\"c1\",\"model\":\"m9\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"He\"}}]}\n\n",
        "data: {\"id\":\"c1\",\"model\":\"m9\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"llo\"}}],\"usage\":null}\n\n",
        "data: {\"id\":\"c1\",\"model\":\"m9\",\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":4},\"completion_tokens_details\":{\"reasoning_tokens\":2}}}\n\n",
        "data: [DONE]\n\n",
    )
    .as_bytes().to_vec();
    let fake_addr = fake_upstream(
        1,
        Reply {
            status: "200 OK",
            content_type: "text/event-stream",
            body: stream,
        },
    );
    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );
    let (head, outbound) = post_turn(
        addr,
        "/v1/chat/completions",
        &[("Authorization", "Bearer skillstar-codex")],
        inbound,
    );
    assert_eq!(status_of(&head), 200);
    assert!(
        outbound.starts_with(b"data: "),
        "the stream passes through: {outbound:?}"
    );

    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    // The golden tail frame: usage and the answered model from the last
    // frame that carries them.
    assert_eq!(lines[0]["model_asked"], "m9");
    assert_eq!(lines[0]["model_answered"], "m9");
    assert_eq!(lines[0]["tokens"]["input"], 7);
    assert_eq!(lines[0]["tokens"]["output"], 3);
    assert_eq!(lines[0]["tokens"]["cache_read"], 4);
    assert_eq!(lines[0]["tokens"]["reasoning"], 2);
    assert_eq!(lines[0]["status"], 200);
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn an_anthropic_turn_keeps_usage_when_the_reply_fails_to_rebuild() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("ledger-anthropic-sse");
    let inbound = br#"{"model":"m2","max_tokens":16,"messages":[{"role":"user","content":"hi"}]}"#;
    // The Anthropic inbound request is rebuilt as a Chat stream, so the
    // upstream replies SSE; the rebuild back to a Messages reply cannot read
    // a stream, and the agent sees a 502 — but the tokens were spent, and
    // the ledger keeps them from the raw reply.
    let stream = concat!(
        "data: {\"id\":\"c1\",\"model\":\"m2-up\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"}}]}\n\n",
        "data: {\"id\":\"c1\",\"model\":\"m2-up\",\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":6}}\n\n",
        "data: [DONE]\n\n",
    )
    .as_bytes().to_vec();
    let fake_addr = fake_upstream(
        1,
        Reply {
            status: "200 OK",
            content_type: "text/event-stream",
            body: stream,
        },
    );
    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );
    let (head, outbound) = post_turn(
        addr,
        "/v1/messages",
        &[("Authorization", "Bearer skillstar-claude-code")],
        inbound,
    );
    assert_eq!(
        status_of(&head),
        502,
        "the reply cannot be rebuilt: {outbound:?}"
    );

    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["agent"], "claude-code");
    assert_eq!(lines[0]["endpoint"], "/v1/messages");
    assert_eq!(lines[0]["model_asked"], "m2");
    assert_eq!(lines[0]["model_answered"], "m2-up");
    assert_eq!(lines[0]["tokens"]["input"], 11);
    assert_eq!(lines[0]["tokens"]["output"], 6);
    assert_eq!(lines[0]["status"], 502);
    assert_eq!(lines[0]["error_kind"], "upstream");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_turn_over_a_readonly_data_directory_still_succeeds() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("ledger-readonly");
    if !cfg!(unix) {
        return;
    }
    // The gateway directory exists but cannot be written to: every append
    // fails, and no turn may notice.
    let gateway_dir = data.dir.join("gateway");
    std::fs::create_dir_all(&gateway_dir).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gateway_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    }
    let inbound = fixture("inbound.json");
    let fake_addr = fake_upstream(
        1,
        Reply {
            status: "200 OK",
            content_type: "application/json",
            body: fixture("upstream_response.json"),
        },
    );
    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );
    let (head, outbound) = post_turn(
        addr,
        "/v1/chat/completions",
        &[("Authorization", "Bearer skillstar-codex")],
        &inbound,
    );
    assert_eq!(
        status_of(&head),
        200,
        "a failed append never breaks the turn"
    );
    assert_eq!(outbound, fixture("outbound.json"));
    assert!(!data.dir.join("gateway").join("usage.jsonl").exists());
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gateway_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn concurrent_turns_leave_whole_lines() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("ledger-concurrent");
    let inbound = fixture("inbound.json");
    let response = fixture("upstream_response.json");
    let fake_addr = fake_upstream(
        8,
        Reply {
            status: "200 OK",
            content_type: "application/json",
            body: response,
        },
    );
    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{fake_addr}")),
    );

    thread::scope(|scope| {
        for _ in 0..2 {
            let (addr, inbound) = (addr, inbound.clone());
            scope.spawn(move || {
                for _ in 0..4 {
                    let (head, _) = post_turn(
                        addr,
                        "/v1/chat/completions",
                        &[("Authorization", "Bearer skillstar-codex")],
                        &inbound,
                    );
                    assert_eq!(status_of(&head), 200);
                }
            });
        }
    });

    let lines = data.usage_lines();
    assert_eq!(
        lines.len(),
        8,
        "two threads, four turns each, whole lines only"
    );
    for record in &lines {
        assert_eq!(
            record["tokens"]["output"], 5,
            "every line parses whole: {record}"
        );
    }
    stop.stop();
    handle.join().unwrap().unwrap();
}

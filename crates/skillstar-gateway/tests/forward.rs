//! End-to-end turns through the injected upstream env (evolution slice 10):
//! a scriptable fake provider, a fake account book, and the real listener.
//! Each test asserts what left the gateway (the signed headers, the order
//! the candidates were tried), what the agent got back, and the ledger line
//! the turn landed — catalog and account attribution included.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use skillstar_gateway::{
    AccountBook, AccountSnapshot, AllowanceSnapshot, ProviderSnapshot, ServeError, ServeOptions,
    Upstream, UpstreamEnv, serve,
};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A `SKILLSTAR_DATA_DIR` unique to this instance, so one test's ledger
/// reads see only that test's turns. Hold `env_lock` while it lives.
struct LedgerSandbox {
    previous: Option<std::ffi::OsString>,
    dir: std::path::PathBuf,
}

impl LedgerSandbox {
    fn new(label: &str) -> Self {
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "skillstar-gateway-forward-{label}-{}-{nanos}",
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

/// One request the fake provider saw.
#[derive(Clone, Debug)]
struct Captured {
    path: String,
    authorization: String,
    accept: String,
    body: Vec<u8>,
}

/// One canned reply.
struct Reply {
    status: &'static str,
    body: Vec<u8>,
}

fn json_ok(body: Vec<u8>) -> Reply {
    Reply {
        status: "200 OK",
        body,
    }
}

/// A fake provider that answers `count` requests. Each answer is chosen by
/// the request's Authorization header, so candidate order and per-candidate
/// failures script themselves. Replies close the connection, so every
/// request is a fresh capture.
fn fake_provider(
    count: usize,
    answer: Arc<dyn Fn(&Captured) -> Reply + Send + Sync>,
) -> (SocketAddr, Arc<Mutex<Vec<Captured>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let hits: Arc<Mutex<Vec<Captured>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&hits);
    thread::spawn(move || {
        for _ in 0..count {
            let Ok((mut sock, _)) = listener.accept() else {
                break;
            };
            let Some(captured) = read_request(&mut sock) else {
                continue;
            };
            seen.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(captured.clone());
            let reply = answer(&captured);
            let head = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.status,
                reply.body.len()
            );
            let _ = sock.write_all(head.as_bytes());
            let _ = sock.write_all(&reply.body);
        }
    });
    (addr, hits)
}

/// Read one request: its path, the headers this file asserts on, the body.
/// The budget is generous because a rotating turn makes several sequential
/// upstream roundtrips, and under a full test-suite load those can outrun a
/// tight one.
fn read_request(sock: &mut TcpStream) -> Option<Captured> {
    sock.set_read_timeout(Some(Duration::from_secs(20))).ok()?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        if let Some(parts) = split_request(&buf) {
            return Some(parts);
        }
        match sock.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }
    split_request(&buf)
}

fn split_request(buf: &[u8]) -> Option<Captured> {
    let split = buf.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..split]).ok()?;
    let mut lines = head.split("\r\n");
    let request = lines.next()?;
    let path = request.split_whitespace().nth(1)?.to_string();
    let mut authorization = String::new();
    let mut accept = String::new();
    let mut length = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "authorization" => authorization = value.trim().to_string(),
            "accept" => accept = value.trim().to_string(),
            "content-length" => length = value.trim().parse().ok(),
            _ => {}
        }
    }
    let body = &buf[split + 4..];
    let length = length?;
    if body.len() < length {
        return None;
    }
    Some(Captured {
        path,
        authorization,
        accept,
        body: body[..length].to_vec(),
    })
}

/// The fake book: accounts and allowances by catalog id.
struct FakeBook {
    accounts: Vec<(&'static str, &'static str)>,
    allowances: Vec<(&'static str, f64)>,
}

impl AccountBook for FakeBook {
    fn account(&self, catalog_id: &str) -> Option<AccountSnapshot> {
        self.accounts
            .iter()
            .find(|(catalog, _)| *catalog == catalog_id)
            .map(|(_, token)| AccountSnapshot {
                access_token: Some(token.to_string()),
                ..AccountSnapshot::default()
            })
    }

    fn allowance(&self, catalog_id: &str) -> Option<AllowanceSnapshot> {
        self.allowances
            .iter()
            .find(|(catalog, _)| *catalog == catalog_id)
            .map(|(_, percent)| AllowanceSnapshot {
                percent: *percent,
                renews_at: None,
            })
    }
}

fn upstream(
    id: &str,
    catalog_id: &str,
    endpoint: String,
    provider: Option<ProviderSnapshot>,
) -> Upstream {
    Upstream {
        id: id.to_string(),
        catalog_id: catalog_id.to_string(),
        endpoint,
        provider,
    }
}

fn env(candidates: Vec<Upstream>, book: FakeBook) -> UpstreamEnv {
    UpstreamEnv {
        resolve: Box::new(move |_model_ref: &str| candidates.clone()),
        book: Box::new(book),
        attribute: Box::new(|id: &str| {
            (
                format!("cat-{id}"),
                format!("sub-{id}"),
            )
        }),
    }
}

fn start(
    env: UpstreamEnv,
) -> (
    SocketAddr,
    skillstar_gateway::Stop,
    thread::JoinHandle<Result<(), ServeError>>,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let options = ServeOptions::bind("127.0.0.1:0".parse().unwrap())
        .env(env)
        .on_bound(tx);
    let stop = options.stop_handle();
    let handle = thread::spawn(move || serve(options));
    let addr = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    (addr, stop, handle)
}

/// POST one chat turn with the shared passthrough fixture, return status and
/// body.
fn post_turn(addr: SocketAddr) -> (u16, Vec<u8>) {
    let inbound = fixture("inbound.json");
    let mut sock = TcpStream::connect(addr).unwrap();
    let head = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer skillstar-codex\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        inbound.len()
    );
    sock.write_all(head.as_bytes()).unwrap();
    sock.write_all(&inbound).unwrap();
    let (head, body) = read_http(&mut sock);
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, body)
}

fn read_http(sock: &mut TcpStream) -> (String, Vec<u8>) {
    sock.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
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

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/magpie/translate/chat-passthrough")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn always_ok(_captured: &Captured) -> Reply {
    json_ok(fixture("upstream_response.json"))
}

#[test]
fn a_routed_turn_signs_the_account_catalog_and_attributes_the_ledger() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-account");
    let (addr, hits) = fake_provider(1, Arc::new(always_ok));
    // A roomy account candidate and an unknown provider-row candidate: smart
    // order sends the roomy one first, and a 200 ends the turn there.
    let candidates = vec![
        upstream("acct-row", "codex", format!("http://{addr}"), None),
        upstream(
            "relay-row",
            "gemini-cli",
            format!("http://{addr}"),
            Some(ProviderSnapshot {
                api_key: Some("sk-relay-secret".to_string()),
            }),
        ),
    ];
    let book = FakeBook {
        accounts: vec![("codex", "tok-acct")],
        allowances: vec![("codex", 10.0)],
    };
    let (listen, stop, handle) = start(env(candidates, book));

    let (status, body) = post_turn(listen);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));

    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert_eq!(hits.len(), 1, "a success ends the turn on one candidate");
    assert_eq!(hits[0].path, "/v1/chat/completions");
    assert_eq!(hits[0].authorization, "Bearer tok-acct");
    assert_eq!(hits[0].accept, "application/json");
    assert_eq!(
        hits[0].body,
        fixture("upstream_request.json"),
        "the translated body, not the inbound bytes, leaves the gateway"
    );

    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["catalog"], "cat-acct-row", "the winner's catalog");
    assert_eq!(lines[0]["account"], "sub-acct-row");
    assert_eq!(lines[0]["status"], 200);
    assert_eq!(lines[0]["model_asked"], "m1");
    let text = format!("{}", lines[0]);
    assert!(!text.contains("tok-acct"), "no signing secret: {text}");
    assert!(!text.contains(&format!("http://{addr}")), "{text}");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_spent_account_falls_through_to_the_provider_row_key() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-relay");
    let (addr, hits) = fake_provider(1, Arc::new(always_ok));
    // The account candidate is used up (98 is the threshold): smart order
    // puts the unknown provider row ahead of it.
    let candidates = vec![
        upstream("acct-row", "codex", format!("http://{addr}"), None),
        upstream(
            "relay-row",
            "gemini-cli",
            format!("http://{addr}"),
            Some(ProviderSnapshot {
                api_key: Some("sk-relay-secret".to_string()),
            }),
        ),
    ];
    let book = FakeBook {
        accounts: vec![("codex", "tok-acct")],
        allowances: vec![("codex", 99.0)],
    };
    let (listen, stop, handle) = start(env(candidates, book));

    let (status, _) = post_turn(listen);
    assert_eq!(status, 200);

    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].authorization, "Bearer sk-relay-secret",
        "the provider row signs with its own key"
    );
    let lines = data.usage_lines();
    assert_eq!(lines[0]["catalog"], "cat-relay-row");
    let text = format!("{}", lines[0]);
    assert!(!text.contains("sk-relay-secret"), "{text}");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_401_is_passed_through_once_and_lands_an_auth_line() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-401");
    let refusal = br#"{"error":{"message":"invalid key","type":"auth_error"}}"#.to_vec();
    let scripted = refusal.clone();
    let answer = Arc::new(move |_captured: &Captured| Reply {
        status: "401 Unauthorized",
        body: scripted.clone(),
    });
    let (addr, hits) = fake_provider(2, answer);
    let candidates = vec![upstream("auth-row", "codex", format!("http://{addr}"), None)];
    let book = FakeBook {
        accounts: vec![("codex", "tok-acct")],
        allowances: vec![("codex", 10.0)],
    };
    let (listen, stop, handle) = start(env(candidates, book));

    let (status, body) = post_turn(listen);
    assert_eq!(
        status, 401,
        "the default reauthorize hook gives up: no resend, plain pass-through"
    );
    assert_eq!(body, refusal);

    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len();
    assert_eq!(hits, 1, "a 401 does not rotate: one upstream call");
    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["status"], 401);
    assert_eq!(lines[0]["error_kind"], "auth");
    assert_eq!(lines[0]["catalog"], "cat-auth-row");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_429_rotates_to_the_next_candidate_and_the_rest_seat_sticks() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-429");
    let rate = br#"{"error":{"message":"Too many requests","type":"rate_limit_error"}}"#.to_vec();
    let answer = Arc::new(move |captured: &Captured| {
        if captured.authorization.contains("tok-a") {
            Reply {
                status: "429 Too Many Requests",
                body: rate.clone(),
            }
        } else {
            always_ok(captured)
        }
    });
    let (addr, hits) = fake_provider(3, answer);
    let candidates = vec![
        upstream("seat-a", "codex", format!("http://{addr}"), None),
        upstream("seat-b", "kiro", format!("http://{addr}"), None),
    ];
    let book = FakeBook {
        accounts: vec![("codex", "tok-a"), ("kiro", "tok-b")],
        allowances: vec![("codex", 10.0), ("kiro", 20.0)],
    };
    let (listen, stop, handle) = start(env(candidates, book));

    // Turn 1: seat-a is refused for rate, seat-b answers.
    let (status, _) = post_turn(listen);
    assert_eq!(status, 200);
    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["catalog"], "cat-seat-b", "the winner, not the first try");
    assert_eq!(lines[0]["account"], "sub-seat-b");
    assert_eq!(lines[0]["error_kind"], serde_json::Value::Null);

    // Turn 2: seat-a is resting, so only seat-b is asked.
    let (status, _) = post_turn(listen);
    assert_eq!(status, 200);
    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert_eq!(hits.len(), 3, "turn 1 asked both, turn 2 asked only seat-b");
    assert_eq!(hits[2].authorization, "Bearer tok-b");
    assert_eq!(data.usage_lines().len(), 2);
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_quota_refusal_names_itself_in_the_ledger() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-quota");
    // The quota word list, not the bare 429, decides the ledger's error
    // kind when the refusal is the turn's final answer.
    let refusal =
        br#"{"error":{"message":"You have exceeded your plan limit","type":"plan_limit"}}"#.to_vec();
    let scripted = refusal.clone();
    let answer = Arc::new(move |_captured: &Captured| Reply {
        status: "429 Too Many Requests",
        body: scripted.clone(),
    });
    let (addr, hits) = fake_provider(1, answer);
    let candidates = vec![upstream("plan-row", "codex", format!("http://{addr}"), None)];
    let book = FakeBook {
        accounts: vec![("codex", "tok-acct")],
        allowances: vec![("codex", 10.0)],
    };
    let (listen, stop, handle) = start(env(candidates, book));

    let (status, body) = post_turn(listen);
    assert_eq!(status, 429);
    assert_eq!(body, refusal);
    let seen = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len();
    assert_eq!(seen, 1, "a single-candidate turn asks once");
    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0]["error_kind"], "quota",
        "the rest word lists refine what the status alone settles"
    );
    assert_eq!(lines[0]["catalog"], "cat-plan-row");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn every_candidate_rested_keeps_the_502_semantics() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-rested");
    let rate = br#"{"error":{"message":"Too many requests","type":"rate_limit_error"}}"#.to_vec();
    let answer = Arc::new(move |_captured: &Captured| Reply {
        status: "429 Too Many Requests",
        body: rate.clone(),
    });
    let (addr, hits) = fake_provider(2, answer);
    let candidates = vec![
        upstream("gone-a", "codex", format!("http://{addr}"), None),
        upstream("gone-b", "kiro", format!("http://{addr}"), None),
    ];
    let book = FakeBook {
        accounts: vec![("codex", "tok-a"), ("kiro", "tok-b")],
        allowances: vec![("codex", 10.0), ("kiro", 20.0)],
    };
    let (listen, stop, handle) = start(env(candidates, book));

    // Turn 1: both refuse; the last refusal is what the agent sees.
    let (status, _) = post_turn(listen);
    assert_eq!(status, 429);
    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["status"], 429);
    assert_eq!(lines[0]["error_kind"], "rate_limit");
    assert_eq!(lines[0]["catalog"], "cat-gone-b", "the last candidate to answer");

    // Turn 2: both seats rest, so the plain no-upstream 502 answers.
    let (status, body) = post_turn(listen);
    assert_eq!(status, 502);
    assert_eq!(body, b"no upstream");
    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len();
    assert_eq!(hits, 2, "a rested seat is not asked again");
    let lines = data.usage_lines();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1]["status"], 502);
    assert_eq!(lines[1]["error_kind"], "upstream");
    assert_eq!(lines[1]["catalog"], "", "nobody answered");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn no_candidates_keeps_the_502_semantics() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-empty");
    let (listen, stop, handle) = start(env(Vec::new(), FakeBook {
        accounts: vec![],
        allowances: vec![],
    }));
    let (status, body) = post_turn(listen);
    assert_eq!(status, 502);
    assert_eq!(body, b"no upstream");
    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["status"], 502);
    assert_eq!(lines[0]["error_kind"], "upstream");
    stop.stop();
    handle.join().unwrap().unwrap();
}

#[test]
fn a_bridge_candidate_sends_no_http() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-bridge");
    let (addr, hits) = fake_provider(1, Arc::new(always_ok));
    // `anthropic` signs onto the process bridge: no HTTP leaves the gateway,
    // the seat is not punished, and the turn answers the plain 502.
    let candidates = vec![upstream("bridge-row", "anthropic", format!("http://{addr}"), None)];
    let book = FakeBook {
        accounts: vec![("anthropic", "sk-ant-not-sent")],
        allowances: vec![],
    };
    let (listen, stop, handle) = start(env(candidates, book));

    let (status, body) = post_turn(listen);
    assert_eq!(status, 502);
    assert_eq!(body, b"no upstream");
    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len();
    assert_eq!(hits, 0, "the bridge candidate never opens HTTP");
    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["catalog"], "");
    let text = format!("{}", lines[0]);
    assert!(!text.contains("sk-ant-not-sent"), "{text}");
    stop.stop();
    handle.join().unwrap().unwrap();
}

// ── the 401 self-heal (evolution slice 11, spec D9) ────────────────────────

/// A book whose `reauthorize` hook answers. `Rotate` swaps the signing token
/// and hands the fresh material back (the production shape: the app writes
/// the renewed credentials where `account()` reads them); `GiveUp` models a
/// heal that could not produce material — a busy serialization domain, a
/// dead refresh grant — and answers `None`. Clones share the counters, so a
/// clone held by the test watches what the turn did to the book it kept.
struct HealBook {
    token: Arc<Mutex<String>>,
    healed: Arc<std::sync::atomic::AtomicU32>,
    gives_up: bool,
}

impl HealBook {
    fn rotating(stale: &str) -> Self {
        Self {
            token: Arc::new(Mutex::new(stale.to_string())),
            healed: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            gives_up: false,
        }
    }

    fn refusing(stale: &str) -> Self {
        Self {
            gives_up: true,
            ..Self::rotating(stale)
        }
    }

    fn heal_calls(&self) -> u32 {
        self.healed.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Clone for HealBook {
    fn clone(&self) -> Self {
        Self {
            token: Arc::clone(&self.token),
            healed: Arc::clone(&self.healed),
            gives_up: self.gives_up,
        }
    }
}

impl AccountBook for HealBook {
    fn account(&self, _catalog_id: &str) -> Option<AccountSnapshot> {
        let token = self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        Some(AccountSnapshot {
            access_token: Some(token),
            ..AccountSnapshot::default()
        })
    }

    fn allowance(&self, _catalog_id: &str) -> Option<AllowanceSnapshot> {
        Some(AllowanceSnapshot {
            percent: 10.0,
            renews_at: None,
        })
    }

    fn reauthorize(&self, _catalog_id: &str) -> Option<AccountSnapshot> {
        self.healed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.gives_up {
            return None;
        }
        let mut token = self.token.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *token = "tok-fresh".to_string();
        Some(AccountSnapshot {
            access_token: Some(token.clone()),
            ..AccountSnapshot::default()
        })
    }
}

fn refused(body: Vec<u8>) -> Reply {
    Reply {
        status: "401 Unauthorized",
        body,
    }
}

/// The happy heal: the upstream refuses the stale token, the hook rotates
/// it, and the same candidate is re-signed and re-sent exactly once. The
/// resent request carries the new token, and the turn lands one 200 ledger
/// line — the retry belongs to the same turn, not a new one.
#[test]
fn a_401_self_heal_resigns_and_resends_the_new_token_once() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-heal");
    let answer = Arc::new(|captured: &Captured| {
        if captured.authorization.contains("tok-stale") {
            refused(br#"{"error":{"message":"token expired"}}"#.to_vec())
        } else {
            always_ok(captured)
        }
    });
    let (addr, hits) = fake_provider(2, answer);
    let book = HealBook::rotating("tok-stale");
    let seen = book.clone();
    let candidates = vec![upstream("heal-row", "codex", format!("http://{addr}"), None)];
    let env = UpstreamEnv {
        resolve: Box::new(move |_model_ref: &str| candidates.clone()),
        book: Box::new(book),
        attribute: Box::new(|id: &str| (format!("cat-{id}"), format!("sub-{id}"))),
    };
    let (listen, stop, handle) = start(env);

    let (status, body) = post_turn(listen);
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));

    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert_eq!(hits.len(), 2, "one original send, one healed resend");
    assert_eq!(hits[0].authorization, "Bearer tok-stale");
    assert_eq!(
        hits[1].authorization, "Bearer tok-fresh",
        "the resend carries the material the hook wrote back"
    );
    assert_eq!(
        seen.heal_calls(),
        1,
        "the hook is asked exactly once per turn"
    );

    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1, "the healed retry is part of the same turn");
    assert_eq!(lines[0]["status"], 200);
    assert_eq!(lines[0]["catalog"], "cat-heal-row");
    assert_eq!(lines[0]["account"], "sub-heal-row");
    let text = format!("{}", lines[0]);
    assert!(!text.contains("tok-stale"), "no signing secret: {text}");
    assert!(!text.contains("tok-fresh"), "{text}");
    stop.stop();
    handle.join().unwrap().unwrap();
}

/// A 401 that survives the heal: exactly one resend, then the refusal is
/// passed through, the ledger names it auth, and the candidate sits in the
/// auth rest — the next turn does not ask it again.
#[test]
fn a_401_that_survives_the_heal_passes_through_and_parks_the_seat() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-heal-dead");
    let refusal = br#"{"error":{"message":"still invalid"}}"#.to_vec();
    let scripted = refusal.clone();
    let answer = Arc::new(move |_captured: &Captured| Reply {
        status: "401 Unauthorized",
        body: scripted.clone(),
    });
    let (addr, hits) = fake_provider(2, answer);
    let book = HealBook::rotating("tok-stale");
    let seen = book.clone();
    let candidates = vec![upstream("dead-row", "codex", format!("http://{addr}"), None)];
    let env = UpstreamEnv {
        resolve: Box::new(move |_model_ref: &str| candidates.clone()),
        book: Box::new(book),
        attribute: Box::new(|id: &str| (format!("cat-{id}"), format!("sub-{id}"))),
    };
    let (listen, stop, handle) = start(env);

    let (status, body) = post_turn(listen);
    assert_eq!(status, 401);
    assert_eq!(body, refusal);
    let hits_after_turn = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len();
    assert_eq!(hits_after_turn, 2, "one send, exactly one healed resend");
    assert_eq!(seen.heal_calls(), 1);
    let lines = data.usage_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["status"], 401);
    assert_eq!(lines[0]["error_kind"], "auth");
    assert_eq!(lines[0]["catalog"], "cat-dead-row");

    // The auth rest holds the seat: the next turn is the plain no-upstream
    // 502, and no third upstream call leaves the gateway.
    let (status, body) = post_turn(listen);
    assert_eq!(status, 502);
    assert_eq!(body, b"no upstream");
    assert_eq!(
        hits.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len(),
        2,
        "the rested seat is not asked again"
    );
    assert_eq!(seen.heal_calls(), 1, "no second heal without a send");
    let lines = data.usage_lines();
    assert_eq!(lines[1]["status"], 502);
    assert_eq!(lines[1]["error_kind"], "upstream");
    stop.stop();
    handle.join().unwrap().unwrap();
}

/// A hook with no material to give (the degraded path: a busy serialization
/// domain, or a catalog nothing can renew): no resend at all, the 401 is
/// passed through, and the seat still backs off into the auth rest.
#[test]
fn a_401_the_hook_cannot_heal_passes_through_with_backoff() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-heal-none");
    let refusal = br#"{"error":{"message":"invalid key"}}"#.to_vec();
    let scripted = refusal.clone();
    let answer = Arc::new(move |_captured: &Captured| Reply {
        status: "401 Unauthorized",
        body: scripted.clone(),
    });
    let (addr, hits) = fake_provider(2, answer);
    let book = HealBook::refusing("tok-stale");
    let seen = book.clone();
    let candidates = vec![upstream("stuck-row", "codex", format!("http://{addr}"), None)];
    let env = UpstreamEnv {
        resolve: Box::new(move |_model_ref: &str| candidates.clone()),
        book: Box::new(book),
        attribute: Box::new(|id: &str| (format!("cat-{id}"), format!("sub-{id}"))),
    };
    let (listen, stop, handle) = start(env);

    let (status, body) = post_turn(listen);
    assert_eq!(status, 401);
    assert_eq!(body, refusal);
    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len();
    assert_eq!(hits, 1, "no material, no resend");
    assert_eq!(seen.heal_calls(), 1, "the hook was still asked once");
    let lines = data.usage_lines();
    assert_eq!(lines[0]["status"], 401);
    assert_eq!(lines[0]["error_kind"], "auth");

    // The give-up still backs off: a later turn does not hammer the dead
    // login while the heal machinery is stuck.
    let (status, body) = post_turn(listen);
    assert_eq!(status, 502);
    assert_eq!(body, b"no upstream");
    stop.stop();
    handle.join().unwrap().unwrap();
}

/// 403 is not an auth failure (`is_auth_error` is 401-only): the hook is
/// never asked, the refusal passes through, and no rest is noted for it.
#[test]
fn a_403_never_reaches_the_heal_hook() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-403");
    let refusal = br#"{"error":{"message":"forbidden","type":"forbidden"}}"#.to_vec();
    let scripted = refusal.clone();
    let answer = Arc::new(move |_captured: &Captured| Reply {
        status: "403 Forbidden",
        body: scripted.clone(),
    });
    let (addr, hits) = fake_provider(2, answer);
    let book = HealBook::rotating("tok-stale");
    let seen = book.clone();
    let candidates = vec![upstream("denied-row", "codex", format!("http://{addr}"), None)];
    let env = UpstreamEnv {
        resolve: Box::new(move |_model_ref: &str| candidates.clone()),
        book: Box::new(book),
        attribute: Box::new(|id: &str| (format!("cat-{id}"), format!("sub-{id}"))),
    };
    let (listen, stop, handle) = start(env);

    let (status, body) = post_turn(listen);
    assert_eq!(status, 403);
    assert_eq!(body, refusal);
    let seen_hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len();
    assert_eq!(seen_hits, 1);
    assert_eq!(seen.heal_calls(), 0, "403 never triggers the heal");

    // 403 is the request's fault, not a rest: the next turn asks again.
    let (status, _) = post_turn(listen);
    assert_eq!(status, 403);
    assert_eq!(
        hits.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len(),
        2,
        "no rest seat was noted for a 403"
    );
    assert_eq!(seen.heal_calls(), 0);
    let lines = data.usage_lines();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error_kind"], "auth", "the ledger's classify reads 401|403 as auth");
    stop.stop();
    handle.join().unwrap().unwrap();
}

/// A rotating failure never triggers the heal either: the 429 moves to the
/// next candidate (or, alone, ends the turn) without asking the hook.
#[test]
fn a_429_rotates_without_asking_the_heal_hook() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-429-no-heal");
    let rate = br#"{"error":{"message":"Too many requests","type":"rate_limit_error"}}"#.to_vec();
    let scripted = rate.clone();
    let answer = Arc::new(move |_captured: &Captured| Reply {
        status: "429 Too Many Requests",
        body: scripted.clone(),
    });
    let (addr, hits) = fake_provider(1, answer);
    let book = HealBook::rotating("tok-stale");
    let seen = book.clone();
    let candidates = vec![upstream("rate-row", "codex", format!("http://{addr}"), None)];
    let env = UpstreamEnv {
        resolve: Box::new(move |_model_ref: &str| candidates.clone()),
        book: Box::new(book),
        attribute: Box::new(|id: &str| (format!("cat-{id}"), format!("sub-{id}"))),
    };
    let (listen, stop, handle) = start(env);

    let (status, _) = post_turn(listen);
    assert_eq!(status, 429);
    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .len();
    assert_eq!(hits, 1, "a single-candidate 429 asks once");
    assert_eq!(seen.heal_calls(), 0, "429 never triggers the heal");
    let lines = data.usage_lines();
    assert_eq!(lines[0]["error_kind"], "rate_limit");
    stop.stop();
    handle.join().unwrap().unwrap();
}

/// The cross-runtime probe (slice 11): the sync heal hook is called from a
/// turn riding the listener's own multi-thread runtime. The hook here does
/// what the production bridge does — hands the work to a thread that owns
/// its own runtime and blocks on it through a bounded channel — so this
/// test fails by hanging if that shape ever deadlocks or panics on runtime
/// nesting. `post_turn` returning 200 is the proof the resend completed.
#[test]
fn the_heal_hook_blocks_across_runtimes_from_inside_the_serve_thread() {
    let _guard = env_lock();
    let data = LedgerSandbox::new("fwd-heal-probe");
    let answer = Arc::new(|captured: &Captured| {
        if captured.authorization.contains("stale") {
            refused(br#"{"error":{"message":"token expired"}}"#.to_vec())
        } else {
            always_ok(captured)
        }
    });
    let (addr, hits) = fake_provider(2, answer);
    let book = BridgeProbeBook::default();
    let calls = Arc::clone(&book.calls);
    let candidates = vec![upstream("probe-row", "codex", format!("http://{addr}"), None)];
    let env = UpstreamEnv {
        resolve: Box::new(move |_model_ref: &str| candidates.clone()),
        book: Box::new(book),
        attribute: Box::new(|id: &str| (format!("cat-{id}"), format!("sub-{id}"))),
    };
    let (listen, stop, handle) = start(env);

    let (status, body) = post_turn(listen);
    assert_eq!(
        status, 200,
        "{} (hook calls: {})",
        String::from_utf8_lossy(&body),
        calls.load(std::sync::atomic::Ordering::SeqCst)
    );
    let hits = hits
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[1].authorization, "Bearer tok-probe-fresh");
    let lines = data.usage_lines();
    assert_eq!(lines[0]["status"], 200);
    stop.stop();
    handle.join().unwrap().unwrap();
}

/// The probe's book: `reauthorize` runs an async lock acquisition on a
/// thread with its own current-thread runtime and blocks the turn on the
/// result through a channel — the exact bridge shape the app injects.
struct BridgeProbeBook {
    token: Mutex<String>,
    calls: Arc<std::sync::atomic::AtomicU32>,
}

impl Default for BridgeProbeBook {
    fn default() -> Self {
        Self {
            token: Mutex::new("tok-probe-stale".to_string()),
            calls: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }
}

impl AccountBook for BridgeProbeBook {
    fn account(&self, _catalog_id: &str) -> Option<AccountSnapshot> {
        let token = self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        Some(AccountSnapshot {
            access_token: Some(token),
            ..AccountSnapshot::default()
        })
    }

    fn allowance(&self, _catalog_id: &str) -> Option<AllowanceSnapshot> {
        None
    }

    fn reauthorize(&self, _catalog_id: &str) -> Option<AccountSnapshot> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let fresh = "tok-probe-fresh".to_string();
        // The production bridge: hand the async work (here a tokio Mutex,
        // standing in for Usage's per-catalog serialization domain) to a
        // thread that owns its own runtime, then block the turn on the
        // answer through a bounded channel wait. The renewed material is
        // also written back where account() reads it — that write-back is
        // the contract the turn's re-sign relies on.
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("probe runtime");
            let gate = std::sync::Arc::new(tokio::sync::Mutex::new(()));
            let _guard = runtime.block_on(async { gate.lock().await });
            let _ = tx.send(fresh);
        });
        let fresh = rx.recv_timeout(Duration::from_secs(10)).ok()?;
        *self.token.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = fresh.clone();
        Some(AccountSnapshot {
            access_token: Some(fresh),
            ..AccountSnapshot::default()
        })
    }
}

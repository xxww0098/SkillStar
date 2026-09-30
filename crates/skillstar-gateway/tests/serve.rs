use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::Duration;

use skillstar_gateway::{
    ADDR_ENV, DEFAULT_ADDR, HEADER_READ_TIMEOUT, IDLE_TIMEOUT, PLACEHOLDER_BEARER, Protocol,
    REFUSED_PORT, ServeError, ServeOptions, outbound_body, recent_calls, resolve_addr, serve,
    upstream_body,
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

fn http_body(buf: &[u8]) -> Option<Vec<u8>> {
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
    let length = length?;
    let body = &buf[split + 4..];
    if body.len() < length {
        return None;
    }
    Some(body[..length].to_vec())
}

fn read_message(sock: &mut TcpStream) -> Vec<u8> {
    sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        if let Some(body) = http_body(&buf) {
            return body;
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
    http_body(&buf).unwrap_or_else(|| panic!("no http body in {buf:?}"))
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

//! A text-only target is given a description. Off does not ask for one.

use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};
use skillstar_gateway::{ServeError, ServeOptions, VisionCall, VisionReply, apply_vision, serve};

const EXPECTED_PROMPT: &str = "You describe images for an AI model that cannot see them. It will answer the user from your description alone, so leave nothing out that it may need.\n- Transcribe all text exactly as written, keeping its layout: code, terminal output, error messages, logs, UI labels, menus, file names, numbers.\n- For a screenshot of an app or page: which app or page it is, its layout, and the state of what is on it (selected, disabled, checked, highlighted, error states).\n- For a chart or table: its kind, axes and labels, and every value you can read.\n- For a diagram: its elements and how they are connected.\n- For a photo or drawing: what it shows, with the details that matter.\nDescribe only what is there. Don't guess at what can't be read, say it can't be read. Don't answer questions or give advice. No preamble.";

const PICTURE: &str = "data:image/png;base64,aGVsbG8=";
const HELLO: &str = "A red square with the word HELLO in it.";

#[test]
fn vision_system_bytes_match() {
    assert_eq!(skillstar_gateway::VISION_SYSTEM, EXPECTED_PROMPT);
    assert!(!EXPECTED_PROMPT.ends_with('\n'));
}

#[test]
fn vision_replaces_image_for_text_only_target() {
    let _lock = lock_gateway_env();
    let root = scratch("vision-replace");
    let _env = EnvRestore::sandbox(&root);
    write_gateway(&root, r#"{"vision":"probe/eye"}"#);

    let twice = image_chat(
        "text/m",
        &["data:image/png;base64,twice", "data:image/png;base64,twice"],
    );
    let asks = AtomicUsize::new(0);
    let out = apply_vision(&twice, false, false, &|_call| {
        asks.fetch_add(1, Ordering::SeqCst);
        VisionReply::Bytes(answered("one view"))
    })
    .unwrap();
    assert_eq!(asks.load(Ordering::SeqCst), 1);
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("one view"), "{text}");
    assert!(text.contains("probe/eye describes it"), "{text}");
    assert!(!text.contains("twice"), "{text}");

    let inbound = image_chat("text/m", &[PICTURE]);
    let describer = Upstream::spawn(vec![answered_string(HELLO)]);
    let answerer = Upstream::spawn(vec![
        r#"{"choices":[{"message":{"content":"ok"}}]}"#.to_string(),
    ]);
    let describer_addr = describer.addr;
    let rewritten = apply_vision(&inbound, false, false, &|call| {
        assert_eq!(call.user_agent, "skillstar-vision/1");
        assert_eq!(call.timeout, Duration::from_secs(120));
        let raw = post_bytes(describer_addr, call.user_agent, &call.body);
        let (_, response) = http_parts(&raw);
        VisionReply::Bytes(response)
    })
    .unwrap();

    let described = describer.recv();
    let (headers, request) = http_parts(&described);
    assert!(
        headers.contains("skillstar-vision/1"),
        "description request user agent: {headers}"
    );
    let doc: Value = serde_json::from_slice(&request).unwrap();
    assert_eq!(doc["model"], "probe/eye");
    assert_eq!(doc["stream"], false);
    assert_eq!(doc["max_tokens"], 4096);
    assert_eq!(
        doc["messages"][0]["content"].as_str(),
        Some(EXPECTED_PROMPT)
    );
    assert_eq!(
        doc["messages"][1]["content"][0]["text"],
        "Describe this image."
    );
    assert_eq!(
        doc["messages"][1]["content"][1]["image_url"]["url"],
        PICTURE
    );

    let _ = post_bytes(answerer.addr, "skillstar-test", &rewritten);
    let answered_hit = String::from_utf8(answerer.recv_body()).unwrap();
    assert!(answered_hit.contains("HELLO"), "{answered_hit}");
    assert!(
        answered_hit.contains("probe/eye describes it"),
        "{answered_hit}"
    );
    assert!(
        answered_hit.contains("[End of the image's description]"),
        "{answered_hit}"
    );
    assert!(
        !answered_hit.contains("aGVsbG8="),
        "the answerer still received the image: {answered_hit}"
    );

    let asked = AtomicBool::new(false);
    let looping = serde_json::to_vec(&json!({
        "model": "text/m",
        "messages": [
            {"role": "system", "content": EXPECTED_PROMPT},
            {"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": PICTURE}}
            ]}
        ]
    }))
    .unwrap();
    let out = apply_vision(&looping, false, false, &|_call| {
        asked.store(true, Ordering::SeqCst);
        VisionReply::Failed
    })
    .unwrap();
    assert!(
        !asked.load(Ordering::SeqCst),
        "a description was described again"
    );
    assert_eq!(out, looping);

    let out = apply_vision(&inbound, false, true, &|_call| {
        asked.store(true, Ordering::SeqCst);
        VisionReply::Failed
    })
    .unwrap();
    assert!(!asked.load(Ordering::SeqCst));
    assert_eq!(out, inbound);
}

#[test]
fn vision_off_rejects_image() {
    let _lock = lock_gateway_env();
    let root = scratch("vision-off");
    let _env = EnvRestore::sandbox(&root);
    write_gateway(&root, r#"{"vision":"off"}"#);
    // No slash: a `provider/model` id is rejected by the listener before forward.
    let inbound = image_chat("textm", &[PICTURE]);
    let asked = AtomicBool::new(false);
    let ask = |_: &VisionCall| {
        asked.store(true, Ordering::SeqCst);
        VisionReply::Failed
    };

    let reject = apply_vision(&inbound, false, false, &ask).unwrap_err();
    assert_eq!(reject.status, 400);
    assert!(
        reject.message.contains("does not support image input"),
        "{}",
        reject.message
    );
    assert!(reject.message.contains("textm"), "{}", reject.message);
    assert!(!asked.load(Ordering::SeqCst));

    let kept = apply_vision(&inbound, true, false, &ask).unwrap();
    assert_eq!(kept, inbound);
    assert!(!asked.load(Ordering::SeqCst));

    let older = serde_json::to_vec(&json!({
        "model": "textm",
        "messages": [
            {"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,old"}}
            ]},
            {"role": "user", "content": "now just text"}
        ]
    }))
    .unwrap();
    let omitted = String::from_utf8(apply_vision(&older, false, false, &ask).unwrap()).unwrap();
    assert!(
        omitted.contains("[Image omitted: this model accepts text only.]"),
        "{omitted}"
    );
    assert!(!omitted.contains("base64,old"), "{omitted}");
    assert!(!asked.load(Ordering::SeqCst));

    let upstream = Upstream::spawn(vec![
        r#"{"choices":[{"message":{"content":"ok"}}]}"#.to_string(),
    ]);
    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap())
            .upstream(format!("http://{}", upstream.addr)),
    );
    let _stop = stop_later(stop, handle);
    let response = post_bytes(addr, "skillstar-test", &inbound);
    let (response_headers, _) = http_parts(&response);
    assert!(
        response_headers.starts_with("HTTP/1.1 200"),
        "{response_headers}"
    );
    let (request_headers, body) = http_parts(&upstream.recv());
    assert_eq!(body, inbound);
    assert!(
        !request_headers.contains("skillstar-vision/1"),
        "off still described: {request_headers}"
    );
    drop(_stop);
}

#[test]
fn vision_user_agent_and_caps() {
    let _lock = lock_gateway_env();
    let root = scratch("vision-caps");
    let _env = EnvRestore::sandbox(&root);
    write_gateway(&root, r#"{"vision":"caps/wave"}"#);

    let wave = std::sync::Arc::new((
        Mutex::new(Wave {
            inflight: 0,
            max: 0,
            arrived: 0,
            go: false,
        }),
        Condvar::new(),
    ));
    // Distinct URLs so the gate, not the cache, limits the asks.
    let held: Vec<String> = (0..8).map(|index| format!("hold-{index}")).collect();
    let held_ref: Vec<&str> = held.iter().map(String::as_str).collect();
    let body = image_chat("text/m", &held_ref);
    let wave_ask = std::sync::Arc::clone(&wave);
    apply_vision(&body, false, false, &|call| {
        assert_describe_call(call, "caps/wave");
        let (lock, cv) = &*wave_ask;
        let mut guard = lock.lock().unwrap_or_else(|poison| poison.into_inner());
        guard.inflight += 1;
        if guard.inflight > guard.max {
            guard.max = guard.inflight;
        }
        if !guard.go {
            guard.arrived += 1;
            if guard.arrived >= 4 {
                guard.go = true;
                cv.notify_all();
            } else {
                loop {
                    let (next, waited) = cv
                        .wait_timeout(guard, Duration::from_secs(20))
                        .unwrap_or_else(|poison| poison.into_inner());
                    guard = next;
                    if guard.go || waited.timed_out() {
                        break;
                    }
                }
            }
        }
        guard.inflight -= 1;
        VisionReply::Bytes(answered("held"))
    })
    .unwrap();
    let max = wave
        .0
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .max;
    assert_eq!(max, 4, "descriptions in flight at once");

    write_gateway(&root, r#"{"vision":"caps/keep"}"#);
    let counts = Mutex::new(HashMap::<String, usize>::new());
    for index in 0..257 {
        let src = format!("img-{index:04}");
        let one = image_chat("text/m", &[&src]);
        apply_vision(&one, false, false, &|call| {
            assert_describe_call(call, "caps/keep");
            let src = call_image(call);
            *counts
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .entry(src)
                .or_insert(0) += 1;
            VisionReply::Bytes(answered("kept"))
        })
        .unwrap();
    }
    let again = image_chat("text/m", &["img-0000"]);
    apply_vision(&again, false, false, &|call| {
        assert_describe_call(call, "caps/keep");
        let src = call_image(call);
        *counts
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .entry(src)
            .or_insert(0) += 1;
        VisionReply::Bytes(answered("kept"))
    })
    .unwrap();
    let counts = counts.lock().unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(counts.get("img-0000").copied(), Some(2));
    assert_eq!(counts.get("img-0256").copied(), Some(1));
    assert_eq!(counts.get("img-0001").copied(), Some(1));
}

#[test]
fn vision_listener_describes_then_forwards() {
    let _lock = lock_gateway_env();
    let root = scratch("vision-listen");
    let _env = EnvRestore::sandbox(&root);
    write_gateway(&root, r#"{"vision":"listen/eye"}"#);
    let inbound = image_chat("textm", &[PICTURE]);
    let upstream = Upstream::spawn(vec![
        answered_string(HELLO),
        r#"{"choices":[{"message":{"content":"done"}}]}"#.to_string(),
    ]);
    let (addr, stop, handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap())
            .upstream(format!("http://{}", upstream.addr)),
    );
    let _stop = stop_later(stop, handle);
    let response = post_bytes(addr, "skillstar-test", &inbound);
    let (response_headers, response_body) = http_parts(&response);
    assert!(
        response_headers.starts_with("HTTP/1.1 200"),
        "{response_headers} {}",
        String::from_utf8_lossy(&response_body)
    );
    let (vision_headers, vision_body) = http_parts(&upstream.recv());
    assert!(
        vision_headers.contains("skillstar-vision/1"),
        "{vision_headers}"
    );
    let doc: Value = serde_json::from_slice(&vision_body).unwrap();
    assert_eq!(
        doc["messages"][0]["content"].as_str(),
        Some(EXPECTED_PROMPT)
    );
    assert_eq!(
        doc["messages"][1]["content"][1]["image_url"]["url"],
        PICTURE
    );
    let forwarded = String::from_utf8(upstream.recv_body()).unwrap();
    assert!(forwarded.contains("HELLO"), "{forwarded}");
    assert!(forwarded.contains("listen/eye describes it"), "{forwarded}");
    assert!(!forwarded.contains("aGVsbG8="), "{forwarded}");
    assert!(response_body.windows(4).any(|mark| mark == b"done"));
    drop(_stop);

    write_gateway(&root, r#"{"vision":"textm"}"#);
    let same = image_chat("textm", &[PICTURE]);
    let eye = Upstream::spawn(vec![r#"{"ok":true}"#.to_string()]);
    let (eye_addr, eye_stop, eye_handle) = start(
        ServeOptions::bind("127.0.0.1:0".parse().unwrap()).upstream(format!("http://{}", eye.addr)),
    );
    let _eye = stop_later(eye_stop, eye_handle);
    let _ = post_bytes(eye_addr, "skillstar-test", &same);
    let (headers, body) = http_parts(&eye.recv());
    assert_eq!(body, same);
    assert!(
        !headers.contains("skillstar-vision/1"),
        "the vision model was asked to describe its own image: {headers}"
    );
}

fn assert_describe_call(call: &VisionCall, model: &str) {
    assert_eq!(call.user_agent, "skillstar-vision/1");
    assert_eq!(call.timeout, Duration::from_secs(120));
    let doc: Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(doc["model"], model);
    assert_eq!(
        doc["messages"][0]["content"].as_str(),
        Some(EXPECTED_PROMPT)
    );
}

fn call_image(call: &VisionCall) -> String {
    let doc: Value = serde_json::from_slice(&call.body).unwrap();
    doc["messages"][1]["content"][1]["image_url"]["url"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

struct Wave {
    inflight: usize,
    max: usize,
    arrived: usize,
    go: bool,
}

fn image_chat(model: &str, srcs: &[&str]) -> Vec<u8> {
    let content: Vec<Value> = srcs
        .iter()
        .map(|src| json!({"type": "image_url", "image_url": {"url": src}}))
        .collect();
    serde_json::to_vec(&json!({
        "model": model,
        "messages": [{"role": "user", "content": content}]
    }))
    .unwrap()
}

fn answered(text: &str) -> Vec<u8> {
    answered_string(text).into_bytes()
}

fn answered_string(text: &str) -> String {
    json!({"choices":[{"message":{"content": text}}]}).to_string()
}

fn start(
    options: ServeOptions,
) -> (
    SocketAddr,
    skillstar_gateway::Stop,
    JoinHandle<Result<(), ServeError>>,
) {
    let (tx, rx) = mpsc::channel();
    let options = options.on_bound(tx);
    let stop = options.stop_handle();
    let handle = thread::spawn(move || serve(options));
    let addr = rx
        .recv_timeout(Duration::from_secs(20))
        .expect("listener bound");
    (addr, stop, handle)
}

struct StopLater {
    stop: Option<skillstar_gateway::Stop>,
    handle: Option<JoinHandle<Result<(), ServeError>>>,
}

fn stop_later(
    stop: skillstar_gateway::Stop,
    handle: JoinHandle<Result<(), ServeError>>,
) -> StopLater {
    StopLater {
        stop: Some(stop),
        handle: Some(handle),
    }
}

impl Drop for StopLater {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.stop();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct Upstream {
    addr: SocketAddr,
    rx: Mutex<mpsc::Receiver<Vec<u8>>>,
    handle: Option<JoinHandle<()>>,
}

impl Upstream {
    fn spawn(responses: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            for response_body in responses {
                let Ok((mut sock, _)) = listener.accept() else {
                    break;
                };
                sock.set_read_timeout(Some(Duration::from_secs(20))).ok();
                let raw = read_http(&mut sock);
                if raw.is_empty() {
                    break;
                }
                let _ = tx.send(raw);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response_body.len()
                );
                let _ = sock.write_all(header.as_bytes());
                let _ = sock.write_all(response_body.as_bytes());
            }
        });
        Self {
            addr,
            rx: Mutex::new(rx),
            handle: Some(handle),
        }
    }

    fn recv(&self) -> Vec<u8> {
        self.rx
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .recv_timeout(Duration::from_secs(20))
            .expect("upstream request")
    }

    fn recv_body(&self) -> Vec<u8> {
        http_parts(&self.recv()).1
    }
}

impl Drop for Upstream {
    fn drop(&mut self) {
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(200));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn post_bytes(addr: SocketAddr, user_agent: &str, body: &[u8]) -> Vec<u8> {
    let mut sock = TcpStream::connect(addr).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let header = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nUser-Agent: {user_agent}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    sock.write_all(header.as_bytes()).unwrap();
    sock.write_all(body).unwrap();
    read_http(&mut sock)
}

fn read_http(sock: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        if let Some(total) = framed_len(&buf)
            && buf.len() >= total
        {
            buf.truncate(total);
            break;
        }
        match sock.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(error)
                if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut =>
            {
                break;
            }
            Err(error) => panic!("read http: {error}"),
        }
    }
    buf
}

fn framed_len(buf: &[u8]) -> Option<usize> {
    let split = buf.windows(4).position(|mark| mark == b"\r\n\r\n")?;
    let headers = std::str::from_utf8(&buf[..split]).ok()?;
    let length = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse::<usize>().ok()
        } else {
            None
        }
    })?;
    Some(split + 4 + length)
}

fn http_parts(raw: &[u8]) -> (String, Vec<u8>) {
    let split = raw
        .windows(4)
        .position(|mark| mark == b"\r\n\r\n")
        .expect("http headers");
    let headers = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut body = raw[split + 4..].to_vec();
    if let Some(total) = framed_len(raw) {
        let len = total - (split + 4);
        body.truncate(len);
    }
    (headers, body)
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
    let path =
        std::env::temp_dir().join(format!("skillstar-{label}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn gateway_file(root: &Path) -> PathBuf {
    root.join("data").join("config").join("model_gateway.json")
}

fn write_gateway(root: &Path, body: &str) {
    let path = gateway_file(root);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

struct EnvRestore {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    root: PathBuf,
}

impl EnvRestore {
    fn sandbox(root: &Path) -> Self {
        let home = root.join("home");
        let data = root.join("data");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data).unwrap();
        let pairs = [
            ("HOME", home.as_path()),
            ("USERPROFILE", home.as_path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", home.as_path()),
            ("SKILLSTAR_DATA_DIR", data.as_path()),
        ];
        let saved = pairs
            .into_iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe { std::env::set_var(key, value) };
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

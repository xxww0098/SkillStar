//! Claude subscription bridge. The fake `claude` is this package's capture
//! binary, copied or linked onto a private PATH. Nothing here calls a real
//! Claude install or the developer's home directory.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use skillstar_gateway::{
    AccountSnapshot, CallbackOutcome, ClaudeBridge, ClaudeError, ClaudeLaunch, ClaudeRun,
    ServeError, ServeOptions, ToolResult, begin_callback, clear_outbound_log, find_claude_binary,
    listener_bridge, outbound_log, run_mcp_helper, serve,
};

fn gate() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct EnvRestore {
    saved: Vec<(String, Option<OsString>)>,
}

impl EnvRestore {
    fn apply(pairs: &[(String, Option<String>)]) -> Self {
        let saved = pairs
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
                (key.clone(), previous)
            })
            .collect();
        Self { saved }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("skillstar-claude-test-{}-{n}", std::process::id()));
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("home")).unwrap();
        fs::create_dir_all(root.join("capture")).unwrap();
        install_fake(&root.join("bin"));
        Self { root }
    }

    fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn capture(&self) -> PathBuf {
        self.root.join("capture")
    }

    fn exe(&self) -> PathBuf {
        self.root.join("skillstar")
    }

    fn prepare_env(&self, flood: bool, extra: &[(&str, &str)]) -> EnvRestore {
        let home = self.home().to_string_lossy().into_owned();
        let mut pairs = vec![
            (
                "PATH".to_string(),
                Some(self.bin_dir().to_string_lossy().into_owned()),
            ),
            ("HOME".to_string(), Some(home.clone())),
            ("USERPROFILE".to_string(), Some(home)),
            (
                "SKILLSTAR_CLAUDE_CAPTURE".to_string(),
                Some(self.capture().to_string_lossy().into_owned()),
            ),
            (
                "SKILLSTAR_CLAUDE_FLOOD".to_string(),
                flood.then(|| "1".to_string()),
            ),
            ("KEEP".to_string(), Some("yes".to_string())),
        ];
        for (key, value) in extra {
            pairs.push(((*key).to_string(), Some((*value).to_string())));
        }
        let _ = fs::remove_file(self.capture().join("args"));
        let _ = fs::remove_file(self.capture().join("env"));
        let _ = fs::remove_file(self.capture().join("stderr_done"));
        EnvRestore::apply(&pairs)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn install_fake(dir: &Path) {
    let src = PathBuf::from(env!("CARGO_BIN_EXE_claude_capture"));
    #[cfg(unix)]
    {
        let dest = dir.join("claude");
        let _ = fs::remove_file(&dest);
        std::os::unix::fs::symlink(&src, &dest).unwrap();
    }
    #[cfg(windows)]
    {
        fs::copy(&src, dir.join("claude.exe")).unwrap();
    }
}

fn launch(
    bridge: &ClaudeBridge,
    scratch: &Scratch,
    model: &str,
    effort: &str,
    web_search: bool,
    token: Option<&str>,
) -> ClaudeRun {
    let mut spec = ClaudeLaunch::new(model, scratch.exe(), "http://127.0.0.1:9");
    spec.effort = effort.to_string();
    spec.web_search = web_search;
    spec.account = AccountSnapshot {
        access_token: token.map(str::to_string),
        account_id: token.map(|_| "snapshot-account-id-9f3c".to_string()),
        api_key: token.map(|_| "snapshot-api-key-9f3c".to_string()),
    };
    bridge
        .launch(spec)
        .unwrap_or_else(|error| panic!("launch {model}: {error}"))
}

fn wait_text(path: &Path) -> String {
    let start = Instant::now();
    loop {
        if let Ok(text) = fs::read_to_string(path)
            && !text.is_empty()
        {
            return text;
        }
        if start.elapsed() > Duration::from_secs(20) {
            panic!("timed out waiting for {}", path.display());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn env_map(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if map.insert(key.to_string(), value.to_string()).is_some() {
            panic!("duplicate env key {key}");
        }
    }
    map
}

fn expected_args(model: &str, mcp_json: &str, web_search: bool, effort: &str) -> Vec<String> {
    let tools = if web_search { "WebSearch" } else { "" };
    let mut args = [
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--include-partial-messages",
        "--verbose",
        "--model",
        model,
        "--tools",
        tools,
        "--strict-mcp-config",
        "--mcp-config",
        mcp_json,
        "--setting-sources",
        "",
        "--dangerously-skip-permissions",
        "--no-session-persistence",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    if !effort.is_empty() {
        args.push("--effort".to_string());
        args.push(effort.to_string());
        args.push("--thinking-display".to_string());
        args.push("summarized".to_string());
    }
    args
}

fn assert_mcp(mcp_json: &str, exe: &Path) {
    let mcp: Value = serde_json::from_str(mcp_json).unwrap();
    let server = &mcp["mcpServers"]["skillstar"];
    assert!(mcp["mcpServers"].get("magpie").is_none());
    assert_eq!(server["command"], exe.to_string_lossy().as_ref());
    assert_eq!(server["args"][0], "claude-mcp-helper");
    let callback = server["args"][1].as_str().unwrap();
    assert!(
        callback.starts_with("http://127.0.0.1:9/_skillstar/claude-mcp/"),
        "{callback}"
    );
    let tools = PathBuf::from(server["args"][2].as_str().unwrap());
    let parent = tools
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy();
    assert!(parent.starts_with("skillstar-claude-"), "{parent}");
    assert!(tools.is_file(), "{}", tools.display());
}

struct Server {
    addr: SocketAddr,
    stop: skillstar_gateway::Stop,
    handle: Option<thread::JoinHandle<Result<(), ServeError>>>,
}

impl Server {
    fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let options = ServeOptions::bind("127.0.0.1:0".parse().unwrap()).on_bound(tx);
        let stop = options.stop_handle();
        let handle = thread::spawn(move || serve(options));
        let addr = match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(addr) => addr,
            Err(error) => panic!("listener did not bind: {error}"),
        };
        assert_ne!(addr.port(), 21847);
        assert_ne!(addr.port(), 3425);
        Self {
            addr,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.stop();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn post(addr: SocketAddr, path: &str, body: &[u8]) -> (u16, Vec<u8>) {
    let mut sock = TcpStream::connect(addr).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    sock.set_write_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let header = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    sock.write_all(header.as_bytes()).unwrap();
    sock.write_all(body).unwrap();
    read_response(&mut sock)
}

fn read_response(sock: &mut TcpStream) -> (u16, Vec<u8>) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    let start = Instant::now();
    loop {
        if let Some(parsed) = split_response(&buf) {
            return parsed;
        }
        if start.elapsed() > Duration::from_secs(20) {
            panic!("response timeout: {}", String::from_utf8_lossy(&buf));
        }
        match sock.read(&mut tmp) {
            Ok(0) => {
                return split_response(&buf)
                    .unwrap_or_else(|| panic!("short response {}", String::from_utf8_lossy(&buf)));
            }
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(error)
                if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut => {
            }
            Err(error) => panic!("{error}"),
        }
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

fn split_response(buf: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = buf.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..split]).ok()?;
    let status = head.split_whitespace().nth(1)?.parse().ok()?;
    let mut length = None;
    for line in head.split("\r\n").skip(1) {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(value.trim().parse::<usize>().ok()?);
        }
    }
    let body = &buf[split + 4..];
    let length = length?;
    if body.len() < length {
        return None;
    }
    Some((status, body[..length].to_vec()))
}

fn write_exe(path: &Path, executable: bool) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, b"not-executed").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
    #[cfg(not(unix))]
    {
        let _ = executable;
    }
}

#[test]
fn claude_bridge_strips_oauth_even_when_snapshot_has_token() {
    let _gate = gate();
    let scratch = Scratch::new();
    let _env = scratch.prepare_env(
        true,
        &[
            ("ANTHROPIC_BASE_URL", "https://api.anthropic.com"),
            ("ANTHROPIC_API_KEY", "parent-api-key"),
            ("ANTHROPIC_AUTH_TOKEN", "parent-auth-token"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "parent-oauth-token"),
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_ENTRYPOINT", "parent-entry"),
            ("CLAUDE_CODE_SSE_PORT", "9"),
            ("ENABLE_CLAUDEAI_MCP_SERVERS", "9"),
        ],
    );
    clear_outbound_log();
    let bridge = ClaudeBridge::manual();
    let run = launch(
        &bridge,
        &scratch,
        "sonnet-under-test",
        "",
        false,
        Some("snapshot-access-token-9f3c"),
    );
    let env_text = wait_text(&scratch.capture().join("env"));
    let args_text = wait_text(&scratch.capture().join("args"));
    let start = Instant::now();
    let stderr = loop {
        let stderr = run.stderr();
        let done = scratch.capture().join("stderr_done").exists();
        if done && stderr.len() == (1 << 20) && stderr.starts_with(b"HEAD") {
            break stderr;
        }
        if start.elapsed() > Duration::from_secs(10) {
            panic!("stderr cap: done={done} len={}", stderr.len());
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(stderr.len(), 1 << 20);
    assert!(stderr.starts_with(b"HEAD"));

    let env = env_map(&env_text);
    assert_eq!(env.get("KEEP").map(String::as_str), Some("yes"));
    assert_eq!(
        env.get("ENABLE_CLAUDEAI_MCP_SERVERS").map(String::as_str),
        Some("0")
    );
    assert_eq!(
        env.get("DISABLE_AUTO_COMPACT").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        env.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
            .map(String::as_str),
        Some("1")
    );
    for key in [
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "CLAUDE_CODE_SSE_PORT",
    ] {
        assert!(!env.contains_key(key), "{key} leaked into the child");
    }
    assert!(!env_text.contains("snapshot-access-token-9f3c"));
    assert!(!env_text.contains("snapshot-account-id-9f3c"));
    assert!(!env_text.contains("snapshot-api-key-9f3c"));
    assert!(!args_text.contains("snapshot-access-token-9f3c"));
    assert!(!args_text.contains("snapshot-account-id-9f3c"));
    assert!(!args_text.contains("snapshot-api-key-9f3c"));
    assert!(!scratch.home().join(".claude").exists());
    for url in outbound_log() {
        assert!(!url.contains("api.anthropic.com"), "{url}");
        assert!(!url.contains("platform.claude.com"), "{url}");
        assert!(!url.contains("/v1/oauth/token"), "{url}");
        assert!(!url.contains("/v1/messages"), "{url}");
        assert!(!url.contains("snapshot-access-token-9f3c"), "{url}");
    }
}

#[test]
fn claude_bridge_args_match_magpie_list() {
    let _gate = gate();
    let scratch = Scratch::new();
    let _env = scratch.prepare_env(false, &[]);
    let bridge = ClaudeBridge::manual();

    let high = launch(&bridge, &scratch, "sonnet-under-test", "xhigh", true, None);
    let high_args: Vec<String> = wait_text(&scratch.capture().join("args"))
        .lines()
        .map(str::to_string)
        .collect();
    let high_mcp = high_args
        .iter()
        .position(|arg| arg == "--mcp-config")
        .map(|index| high_args[index + 1].clone())
        .expect("mcp-config");
    assert_eq!(
        high_args,
        expected_args("sonnet-under-test", &high_mcp, true, "max")
    );
    assert_mcp(&high_mcp, &scratch.exe());
    drop(high);
    fs::remove_file(scratch.capture().join("args")).unwrap();

    let plain = launch(&bridge, &scratch, "opus-under-test", "", false, None);
    let plain_args: Vec<String> = wait_text(&scratch.capture().join("args"))
        .lines()
        .map(str::to_string)
        .collect();
    let plain_mcp = plain_args
        .iter()
        .position(|arg| arg == "--mcp-config")
        .map(|index| plain_args[index + 1].clone())
        .expect("mcp-config");
    assert_eq!(
        plain_args,
        expected_args("opus-under-test", &plain_mcp, false, "")
    );
    assert!(!plain_args.iter().any(|arg| arg == "--effort"));
    assert!(!plain_args.iter().any(|arg| arg == "--thinking-display"));
    assert_mcp(&plain_mcp, &scratch.exe());
    drop(plain);
}

#[test]
fn claude_bridge_callback_rejects_non_loopback() {
    let _gate = gate();
    let scratch = Scratch::new();
    let _env = scratch.prepare_env(false, &[]);
    let bridge = ClaudeBridge::manual();
    let run = launch(&bridge, &scratch, "sonnet-under-test", "", false, None);
    let token = run.token().to_string();

    match begin_callback(
        &bridge,
        "192.0.2.10:9".parse().unwrap(),
        &token,
        b"not-json",
    ) {
        CallbackOutcome::Ready { status, body } => {
            assert_eq!(status, 403);
            assert_eq!(body, "forbidden");
        }
        CallbackOutcome::Wait(_) => panic!("non-loopback parked a call"),
    }
    assert!(run.is_running());
    assert!(!run.resolve_tool(
        "toolu_side",
        ToolResult {
            content: Value::Null,
            is_error: false,
        }
    ));

    match begin_callback(
        &bridge,
        "0.0.0.0:9".parse().unwrap(),
        &token,
        br#"{"tool_call_id":"toolu_unspec"}"#,
    ) {
        CallbackOutcome::Ready { status, body } => {
            assert_eq!(status, 403);
            assert_eq!(body, "forbidden");
        }
        CallbackOutcome::Wait(_) => panic!("unspecified peer parked a call"),
    }

    match begin_callback(&bridge, "127.0.0.1:9".parse().unwrap(), &token, b"{}") {
        CallbackOutcome::Ready { status, body } => {
            assert_eq!(status, 400);
            assert_eq!(body, "invalid tool call");
        }
        CallbackOutcome::Wait(_) => panic!("empty tool id waited"),
    }

    let v6 = SocketAddr::from((Ipv6Addr::LOCALHOST, 9));
    match begin_callback(&bridge, v6, "no-such-token", b"{}") {
        CallbackOutcome::Ready { status, body } => {
            assert_eq!(status, 404);
            assert_eq!(body, "unknown or expired Claude run");
        }
        CallbackOutcome::Wait(_) => panic!("unknown token waited"),
    }

    match begin_callback(&bridge, v6, &token, br#"{"tool_call_id":"toolu_v6"}"#) {
        CallbackOutcome::Wait(rx) => {
            assert!(run.resolve_tool(
                "toolu_v6",
                ToolResult {
                    content: serde_json::json!(["from-v6"]),
                    is_error: false,
                }
            ));
            let result = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(result.content, serde_json::json!(["from-v6"]));
            assert!(!result.is_error);
        }
        CallbackOutcome::Ready { status, body } => panic!("::1 rejected: {status} {body}"),
    }

    match begin_callback(
        &bridge,
        "127.0.0.1:9".parse().unwrap(),
        &token,
        br#"{"tool_call_id":"toolu_drop"}"#,
    ) {
        CallbackOutcome::Wait(rx) => {
            drop(run);
            assert!(rx.recv_timeout(Duration::from_secs(2)).is_err());
        }
        CallbackOutcome::Ready { status, body } => panic!("loopback rejected: {status} {body}"),
    }

    let server = Server::start();
    let (status, body) = post(server.addr, "/_skillstar/claude-mcp/not-a-token", b"{}");
    assert_eq!(status, 404);
    assert_eq!(body, b"unknown or expired Claude run");

    let run = launch(
        listener_bridge(),
        &scratch,
        "sonnet-under-test",
        "",
        false,
        None,
    );
    let path = format!("/_skillstar/claude-mcp/{}", run.token());
    let addr = server.addr;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        tx.send(post(addr, &path, br#"{"tool_call_id":"toolu_http"}"#))
            .unwrap()
    });
    let started = Instant::now();
    let mut sent = false;
    while started.elapsed() < Duration::from_secs(20) {
        if run.resolve_tool(
            "toolu_http",
            ToolResult {
                content: serde_json::json!([{"type": "text", "text": "pong"}]),
                is_error: false,
            },
        ) {
            sent = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(sent, "callback was not parked");
    let (status, body) = rx.recv_timeout(Duration::from_secs(20)).unwrap();
    assert_eq!(status, 200);
    let parsed: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["content"][0]["text"], "pong");
    assert!(parsed.get("is_error").is_none());
    drop(run);
}

#[test]
fn claude_bridge_idle_caps() {
    let _gate = gate();
    let scratch = Scratch::new();
    let _env = scratch.prepare_env(false, &[]);

    {
        let bridge = ClaudeBridge::manual();
        let mut runs = Vec::new();
        for _ in 0..7 {
            let run = launch(&bridge, &scratch, "sonnet-under-test", "", false, None);
            run.keep_idle();
            runs.push(run);
        }
        assert_eq!(bridge.idle_count(), 6);
        assert!(!runs[0].is_running());
        for run in &runs[1..] {
            assert!(run.is_running());
        }
    }

    {
        let bridge = ClaudeBridge::manual();
        let oldest = launch(&bridge, &scratch, "sonnet-under-test", "", false, None);
        oldest.keep_idle();
        assert!(bridge.advance(Duration::from_millis(1)));
        let mut rest = Vec::new();
        for _ in 0..6 {
            let run = launch(&bridge, &scratch, "sonnet-under-test", "", false, None);
            run.keep_idle();
            rest.push(run);
        }
        assert!(!oldest.is_running());
        assert_eq!(bridge.idle_count(), 6);
        for run in &rest {
            assert!(run.is_running());
        }
        assert!(bridge.advance(Duration::from_secs(20 * 60) - Duration::from_millis(1)));
        for run in &rest {
            assert!(run.is_running());
        }
        assert!(bridge.advance(Duration::from_millis(1)));
        for run in &rest {
            assert!(!run.is_running());
        }
        assert_eq!(bridge.idle_count(), 0);
    }

    {
        let bridge = ClaudeBridge::manual();
        let run = launch(&bridge, &scratch, "sonnet-under-test", "", false, None);
        assert!(bridge.advance(Duration::from_secs(30 * 60) - Duration::from_millis(1)));
        assert!(run.is_running());
        assert!(bridge.advance(Duration::from_millis(1)));
        assert!(!run.is_running());
    }

    {
        let bridge = ClaudeBridge::manual();
        let run = launch(&bridge, &scratch, "sonnet-under-test", "", false, None);
        run.wait_for_tool();
        assert!(bridge.advance(Duration::from_secs(5 * 60) - Duration::from_millis(1)));
        assert!(run.is_running());
        assert!(bridge.advance(Duration::from_millis(1)));
        assert!(!run.is_running());
    }
}

#[test]
fn claude_bridge_binary_order() {
    let root = std::env::temp_dir().join(format!("skillstar-claude-lookup-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let path_dir = root.join("path");
    let home = root.join("home");
    let path_claude = path_dir.join(if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    });
    let home_claude = home.join(".local").join("bin").join(if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    });
    let first = root.join("first");
    let second = root.join("second");
    write_exe(&path_claude, true);
    write_exe(&home_claude, true);
    write_exe(&first, true);
    write_exe(&second, true);
    let fallbacks = [first.clone(), second.clone()];

    let found = find_claude_binary(Some(path_dir.as_os_str()), Some(&home), &fallbacks).unwrap();
    assert_eq!(found, path_claude);

    fs::remove_file(&path_claude).unwrap();
    let found = find_claude_binary(Some(path_dir.as_os_str()), Some(&home), &fallbacks).unwrap();
    assert_eq!(found, home_claude);

    fs::remove_file(&home_claude).unwrap();
    fs::create_dir_all(path_dir.join("claude")).unwrap();
    let found = find_claude_binary(Some(path_dir.as_os_str()), Some(&home), &fallbacks).unwrap();
    assert_eq!(found, first);

    let missing = find_claude_binary(
        Some(root.join("empty").as_os_str()),
        Some(&root.join("no-home")),
        &[root.join("absent")],
    );
    assert!(matches!(missing, Err(ClaudeError::NotInstalled)));

    #[cfg(unix)]
    {
        let dull = path_dir.join("claude");
        let _ = fs::remove_dir_all(&dull);
        write_exe(&dull, false);
        write_exe(&home_claude, true);
        let found =
            find_claude_binary(Some(path_dir.as_os_str()), Some(&home), &fallbacks).unwrap();
        assert_eq!(found, home_claude);
    }

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn claude_bridge_helper_frames_only() {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let dir = std::env::temp_dir().join(format!(
        "skillstar-claude-helper-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("tools.json"), b"[]").unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (body_tx, body_rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        let mut buf = Vec::new();
        let mut tmp = [0u8; 2048];
        let body = loop {
            let n = sock.read(&mut tmp).unwrap();
            if n == 0 {
                break http_body(&buf).unwrap_or(buf);
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(body) = http_body(&buf) {
                break body;
            }
        };
        body_tx.send(body).unwrap();
        let payload = br#"{"content":[{"type":"text","text":"pong"}]}"#;
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        );
        sock.write_all(header.as_bytes()).unwrap();
        sock.write_all(payload).unwrap();
    });

    let (stdout_r, stdout_w) = io::pipe().unwrap();
    let (mut stderr_r, stderr_w) = io::pipe().unwrap();
    let (stdin_r, mut stdin_w) = io::pipe().unwrap();
    let callback = format!("http://{addr}/_skillstar/claude-mcp/tok");
    let tools = dir.join("tools.json");
    let helper = thread::spawn(move || {
        run_mcp_helper(
            &[callback, tools.display().to_string()],
            stdin_r,
            stdout_w,
            stderr_w,
        )
    });
    let (line_tx, line_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = io::BufReader::new(stdout_r);
        loop {
            let mut line = String::new();
            match io::BufRead::read_line(&mut reader, &mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let _ = line_tx.send(line);
                }
                Err(_) => break,
            }
        }
    });

    for frame in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ping","arguments":{},"_meta":{"claudecode/toolUseId":"toolu_1"}}}"#,
    ] {
        stdin_w.write_all(frame.as_bytes()).unwrap();
        stdin_w.write_all(b"\n").unwrap();
        stdin_w.flush().unwrap();
    }

    let mut lines = Vec::new();
    while lines.len() < 3 {
        let line = line_rx
            .recv_timeout(Duration::from_secs(20))
            .expect("mcp frame");
        lines.push(serde_json::from_str::<Value>(line.trim()).unwrap());
    }
    drop(stdin_w);
    assert_eq!(helper.join().unwrap(), 0);
    assert!(line_rx.recv_timeout(Duration::from_millis(200)).is_err());

    let mut stderr = String::new();
    stderr_r.read_to_string(&mut stderr).unwrap();
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(lines.len(), 3);

    let init = lines.iter().find(|line| line["id"] == 1).unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "skillstar");
    let list = lines.iter().find(|line| line["id"] == 2).unwrap();
    assert!(list["result"]["tools"].is_array());
    let call = lines.iter().find(|line| line["id"] == 3).unwrap();
    assert_eq!(call["result"]["content"][0]["text"], "pong");
    assert_eq!(call["result"]["isError"], false);

    let posted: Value =
        serde_json::from_slice(&body_rx.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
    assert_eq!(posted["tool_call_id"], "toolu_1");
    assert_eq!(posted["name"], "ping");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn claude_bridge_helper_usage_on_stderr() {
    let (mut out_r, out_w) = io::pipe().unwrap();
    let (mut err_r, err_w) = io::pipe().unwrap();
    let code = run_mcp_helper(&[], io::empty(), out_w, err_w);
    assert_ne!(code, 0);
    let mut stdout = String::new();
    out_r.read_to_string(&mut stdout).unwrap();
    assert!(stdout.is_empty());
    let mut stderr = String::new();
    err_r.read_to_string(&mut stderr).unwrap();
    assert!(stderr.contains("claude MCP helper expects callback URL and tools file"));
}

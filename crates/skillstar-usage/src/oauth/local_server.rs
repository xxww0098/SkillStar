//! Local HTTP listener for OAuth `redirect_uri` callbacks.
//!
//! Spawns `tiny_http` on `127.0.0.1:{port}`, waits for a single GET to
//! `/auth/callback?code=...&state=...`, validates `state`, returns the code.
//!
//! Codex uses fixed ports with `/cancel` shutdown (mirrors the official CLI).
//! Antigravity and other providers may bind arbitrary ports.

use std::io::{Cursor, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tiny_http::{Header, Response, Server};
use tokio::sync::oneshot;
use tokio::task;

use crate::{UsageError, UsageResult};

const SUCCESS_HTML: &str = r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>登录成功</title>
<style>body{font-family:-apple-system,system-ui,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#0b1020;color:#e7e9ee}
.card{background:rgba(255,255,255,0.04);padding:2rem 3rem;border-radius:12px;border:1px solid rgba(255,255,255,0.08);text-align:center}
h1{margin:0 0 .5rem;font-size:1.4rem}p{margin:0;color:#9ba3b4}</style></head>
<body><div class="card"><h1>✓ 登录成功</h1><p>可以关闭此窗口返回应用。</p></div></body></html>"#;

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const BIND_RETRY_DELAY: Duration = Duration::from_millis(200);
const BIND_MAX_ATTEMPTS: u32 = 10;
const CANCEL_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Short-lived OAuth callback listener.
pub struct CallbackSession {
    pub port: u16,
    cancelled: Arc<AtomicBool>,
    server: Arc<Server>,
}

impl CallbackSession {
    /// Signal the listener loop to exit and release the bound port.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let _ = request_cancel(self.port);
    }
}

/// Bind a callback listener, retrying stale servers via `/cancel` and optional
/// fallback port (Codex: 1455 → 1457).
pub fn start_session(preferred_port: u16, fallback_port: Option<u16>) -> UsageResult<CallbackSession> {
    let mut bind_port = preferred_port;
    let mut using_fallback = false;
    let mut cancel_attempted = false;
    let mut attempts = 0u32;

    loop {
        let addr = format!("127.0.0.1:{}", bind_port);
        match Server::http(&addr) {
            Ok(server) => {
                return Ok(CallbackSession {
                    port: bind_port,
                    cancelled: Arc::new(AtomicBool::new(false)),
                    server: Arc::new(server),
                });
            }
            Err(err) => {
                let addr_in_use = err
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io_err| io_err.kind() == std::io::ErrorKind::AddrInUse);

                if addr_in_use {
                    if !cancel_attempted && !using_fallback {
                        cancel_attempted = true;
                        let _ = request_cancel(preferred_port);
                    }
                    thread::sleep(BIND_RETRY_DELAY);
                    attempts += 1;
                    if attempts >= BIND_MAX_ATTEMPTS {
                        if let Some(fallback) = fallback_port.filter(|_| !using_fallback) {
                            bind_port = fallback;
                            using_fallback = true;
                            attempts = 0;
                            cancel_attempted = false;
                            continue;
                        }
                        return Err(UsageError::Other(format!(
                            "无法监听 {}: 端口已被占用",
                            addr
                        )));
                    }
                    continue;
                }
                return Err(UsageError::Other(format!("无法监听 {}: {}", addr, err)));
            }
        }
    }
}

/// Ask a running listener on `port` to shut down (official Codex CLI pattern).
pub fn request_cancel(port: u16) -> std::io::Result<()> {
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))?;
    let mut stream = TcpStream::connect_timeout(&addr, CANCEL_CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(CANCEL_CONNECT_TIMEOUT))?;
    stream.set_write_timeout(Some(CANCEL_CONNECT_TIMEOUT))?;
    stream.write_all(b"GET /cancel HTTP/1.1\r\n")?;
    stream.write_all(format!("Host: 127.0.0.1:{port}\r\n").as_bytes())?;
    stream.write_all(b"Connection: close\r\n\r\n")?;
    let mut buf = [0u8; 64];
    let _ = stream.read(&mut buf);
    Ok(())
}

/// Run an HTTP listener on `127.0.0.1:{port}` and wait for a callback that
/// matches `expected_state`. Returns the `code` parameter.
///
/// `timeout` defaults to 5 minutes if `None`.
pub async fn wait_for_callback(
    port: u16,
    expected_state: String,
    timeout: Option<Duration>,
) -> UsageResult<String> {
    let session = start_session(port, None)?;
    wait(session, expected_state, timeout).await
}

/// Wait for OAuth callback on an already-bound [`CallbackSession`].
pub async fn wait(
    session: CallbackSession,
    expected_state: String,
    timeout: Option<Duration>,
) -> UsageResult<String> {
    let timeout = timeout.unwrap_or(Duration::from_secs(300));
    let (tx, rx) = oneshot::channel::<UsageResult<String>>();
    let server = session.server.clone();
    let cancelled = session.cancelled.clone();
    let state = expected_state.clone();

    let join = task::spawn_blocking(move || {
        let deadline = Instant::now() + timeout;
        let mut tx = Some(tx);
        loop {
            if tx.is_none() {
                break;
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(Err(UsageError::Other("OAuth 回调超时".into())));
                }
                break;
            }

            let poll_for = remaining.min(POLL_INTERVAL);
            match server.recv_timeout(poll_for) {
                Ok(Some(request)) => {
                    let url = request.url().to_string();
                    let outcome = handle_request(&url, &state);
                    respond(request, outcome.is_ok());
                    match outcome {
                        RequestOutcome::Code(code) => {
                            if let Some(tx) = tx.take() {
                                let _ = tx.send(Ok(code));
                            }
                            break;
                        }
                        RequestOutcome::Cancelled => {
                            if let Some(tx) = tx.take() {
                                let _ = tx.send(Err(UsageError::Other("用户取消登录".into())));
                            }
                            break;
                        }
                        RequestOutcome::Ignored => {}
                    }
                }
                Ok(None) => {
                    if cancelled.load(Ordering::SeqCst) {
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(Err(UsageError::Other("用户取消登录".into())));
                        }
                        break;
                    }
                }
                Err(e) => {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(Err(UsageError::Other(format!(
                            "OAuth listener error: {}",
                            e
                        ))));
                    }
                    break;
                }
            }
        }
        server.unblock();
    });

    let result = rx
        .await
        .map_err(|_| UsageError::Other("OAuth listener dropped".into()))?;
    drop(session);
    let _ = join.await;
    result
}

enum RequestOutcome {
    Code(String),
    Cancelled,
    Ignored,
}

impl RequestOutcome {
    fn is_ok(&self) -> bool {
        matches!(self, RequestOutcome::Code(_) | RequestOutcome::Cancelled)
    }
}

fn handle_request(url: &str, expected_state: &str) -> RequestOutcome {
    let path = url.split('?').next().unwrap_or(url);
    if path == "/cancel" {
        return RequestOutcome::Cancelled;
    }
    if path != "/auth/callback" {
        return RequestOutcome::Ignored;
    }
    match parse_callback(url, expected_state) {
        Ok(code) => RequestOutcome::Code(code),
        Err(_) => RequestOutcome::Ignored,
    }
}

fn respond(request: tiny_http::Request, ok: bool) {
    let body = if ok { SUCCESS_HTML } else { SUCCESS_HTML };
    let resp = Response::new(
        200.into(),
        vec![
            Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap(),
        ],
        Cursor::new(body.as_bytes().to_vec()),
        Some(body.len()),
        None,
    );
    let _ = request.respond(resp);
}

fn parse_callback(url: &str, expected_state: &str) -> UsageResult<String> {
    // url looks like "/auth/callback?code=...&state=..."
    let q_idx = url
        .find('?')
        .ok_or_else(|| UsageError::Other("回调缺少查询参数".into()))?;
    let query = &url[q_idx + 1..];

    let mut code = None;
    let mut state = None;
    let mut error = None;
    for part in query.split('&') {
        let (k, v) = match part.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        let decoded = percent_decode(v).unwrap_or_else(|| v.to_string());
        match k {
            "code" => code = Some(decoded),
            "state" => state = Some(decoded),
            "error" => error = Some(decoded),
            "error_description" if error.is_none() => error = Some(decoded),
            _ => {}
        }
    }

    if let Some(err) = error {
        return Err(UsageError::Other(format!("OAuth 错误：{}", err)));
    }
    let code = code.ok_or_else(|| UsageError::Other("回调缺少 code".into()))?;
    let state = state.ok_or_else(|| UsageError::Other("回调缺少 state".into()))?;
    if state != expected_state {
        return Err(UsageError::Other(
            "OAuth state 不匹配，可能被 CSRF 攻击".into(),
        ));
    }
    Ok(code)
}

fn percent_decode(s: &str) -> Option<String> {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                let val = u8::from_str_radix(hex, 16).ok()?;
                out.push(val);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ok() {
        let r = parse_callback("/auth/callback?code=abc&state=xyz", "xyz");
        assert_eq!(r.unwrap(), "abc");
    }

    #[test]
    fn parse_state_mismatch() {
        let r = parse_callback("/auth/callback?code=abc&state=zzz", "xyz");
        assert!(r.is_err());
    }

    #[test]
    fn parse_error_branch() {
        let r = parse_callback("/auth/callback?error=access_denied&state=xyz", "xyz");
        assert!(r.is_err());
    }

    #[test]
    fn cancel_path_is_cancelled() {
        assert!(matches!(
            handle_request("/cancel", "xyz"),
            RequestOutcome::Cancelled
        ));
    }

    #[tokio::test]
    async fn cancel_releases_port_for_rebind() {
        let session = start_session(0, None).expect("bind ephemeral port");
        let port = session.port;
        let state = "cancel-test".to_string();
        let session_for_wait = CallbackSession {
            port,
            cancelled: session.cancelled.clone(),
            server: session.server.clone(),
        };
        let wait_task = tokio::spawn(async move {
            wait(session_for_wait, state, Some(Duration::from_secs(5))).await
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        session.cancel();
        let result = wait_task.await.expect("wait task");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("取消"));

        let rebound = start_session(port, None);
        assert!(rebound.is_ok(), "port should be reusable after cancel");
    }
}
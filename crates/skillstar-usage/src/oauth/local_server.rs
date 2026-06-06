//! Local HTTP listener for OAuth `redirect_uri` callbacks.
//!
//! Spawns `tiny_http` on `127.0.0.1:{port}`, waits for a single GET to
//! `/auth/callback?code=...&state=...`, validates `state`, returns the code.
//!
//! Used by Codex (port 1455), Antigravity (random port). Cursor and Qoder
//! use `poll_flow` instead.

use std::io::Cursor;
use std::sync::Arc;
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

/// Run an HTTP listener on `127.0.0.1:{port}` and wait for a callback that
/// matches `expected_state`. Returns the `code` parameter.
///
/// `timeout` defaults to 5 minutes if `None`.
pub async fn wait_for_callback(
    port: u16,
    expected_state: String,
    timeout: Option<Duration>,
) -> UsageResult<String> {
    let timeout = timeout.unwrap_or(Duration::from_secs(300));
    let addr = format!("127.0.0.1:{}", port);

    let server =
        Server::http(&addr).map_err(|e| UsageError::Other(format!("无法监听 {}: {}", addr, e)))?;
    let server = Arc::new(server);

    let (tx, rx) = oneshot::channel::<UsageResult<String>>();
    let server_clone = server.clone();
    let state = expected_state.clone();

    let join = task::spawn_blocking(move || {
        let deadline = Instant::now() + timeout;
        let mut tx = Some(tx);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(Err(UsageError::Other("OAuth 回调超时".into())));
                }
                break;
            }
            match server_clone.recv_timeout(remaining) {
                Ok(Some(request)) => {
                    let url = request.url().to_string();
                    let outcome = parse_callback(&url, &state);

                    let body = match &outcome {
                        Ok(_) => SUCCESS_HTML,
                        Err(_) => SUCCESS_HTML,
                    };
                    let resp = Response::new(
                        200.into(),
                        vec![
                            Header::from_bytes(
                                &b"Content-Type"[..],
                                &b"text/html; charset=utf-8"[..],
                            )
                            .unwrap(),
                        ],
                        Cursor::new(body.as_bytes().to_vec()),
                        Some(body.len()),
                        None,
                    );
                    let _ = request.respond(resp);

                    if let Some(tx) = tx.take() {
                        let _ = tx.send(outcome);
                    }
                    break;
                }
                Ok(None) => continue,
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
    });

    let result = rx
        .await
        .map_err(|_| UsageError::Other("OAuth listener dropped".into()))?;
    // Unblock the spawn_blocking task by ensuring server is dropped.
    drop(server);
    let _ = join.await;
    result
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
}

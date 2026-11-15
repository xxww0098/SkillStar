//! Loopback callback listener for the `LocalCallback` login flow (RFC 8252).
//!
//! The server binds `127.0.0.1` only, refuses anything it did not have to read
//! (it only looks at a request's query string, never a body or a path), and
//! hands the query map to whoever is waiting. The same map shape is what a
//! pasted callback URL or bare authorization code parses into, so the manual
//! path and the listener path converge before any token exchange happens.
//!
//! This is the one place in the crate that talks plain TCP: the listener is
//! local, so it deliberately bypasses `probe_http_client` and its proxy
//! configuration.

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tiny_http::{Header, Response, Server};
use url::Url;

use crate::oauth::error::{OAuthError, OAuthResult};

/// Path providers redirect back to. Only the query string is used.
pub const CALLBACK_PATH: &str = "/auth/callback";

/// One callback's query parameters, exactly as the provider sent them.
pub type QueryMap = BTreeMap<String, String>;

/// How long a login may wait for its callback before the flow gives up.
pub const CALLBACK_TIMEOUT: Duration = Duration::from_secs(600);

/// How long one listener wakeup waits before re-checking the other door.
const WAIT_SLICE: Duration = Duration::from_millis(200);

const SUCCESS_PAGE: &str = r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>登录完成</title>
<style>body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;
background:#0f1115;color:#e8eaf0;font:15px/1.6 -apple-system,"PingFang SC",sans-serif}
main{text-align:center}h1{margin:0 0 8px;font-size:20px}p{margin:0;color:#98a2b3}</style></head>
<body><main><h1>登录成功</h1><p>可以关闭此页面，回到 SkillStar。</p></main></body></html>"#;

const INVALID_PAGE: &str = r#"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8"><title>回调无效</title>
<style>body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;
background:#0f1115;color:#e8eaf0;font:15px/1.6 -apple-system,"PingFang SC",sans-serif}
main{text-align:center}h1{margin:0 0 8px;font-size:20px}p{margin:0;color:#98a2b3}</style></head>
<body><main><h1>回调无效</h1><p>这个地址没有携带授权参数，请重新从 SkillStar 发起登录。</p></main></body></html>"#;

/// A listener on `127.0.0.1` that hands over the first callback's query map.
pub struct LoopbackServer {
    server: Server,
    port: u16,
    /// The path the provider redirects to. A registered desktop client pins it
    /// (Google's wants `/oauth-callback`), so it travels with the listener
    /// instead of being assumed at the response site.
    path: &'static str,
}

impl std::fmt::Debug for LoopbackServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoopbackServer")
            .field("port", &self.port)
            .field("path", &self.path)
            .finish()
    }
}

impl LoopbackServer {
    /// Bind the first free port from `preferred`, else an OS-assigned one, with
    /// the default callback path.
    ///
    /// A provider that pinned a redirect URI needs its port to win, so the
    /// caller passes its candidates in priority order and a busy port simply
    /// means "try the next one".
    pub fn bind(preferred: &[u16]) -> OAuthResult<Self> {
        Self::bind_with(preferred, CALLBACK_PATH)
    }

    /// Bind with an explicit callback path, for a client whose registered
    /// redirect URI is not ours to choose.
    pub fn bind_with(preferred: &[u16], path: &'static str) -> OAuthResult<Self> {
        for port in preferred {
            if let Some(server) = bind_port(*port, path) {
                return Ok(server);
            }
            tracing::debug!(port, "loopback callback port is busy, trying the next one");
        }
        bind_port(0, path).ok_or_else(|| {
            OAuthError::Other("无法在 127.0.0.1 上启动本机回调服务器".to_string())
        })
    }

    /// The port actually bound.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The redirect URI handed to the provider.
    pub fn redirect_uri(&self) -> String {
        format!("http://localhost:{}{}", self.port(), self.path)
    }

    /// Wait for the callback through either door: the browser reaching the
    /// listener, or a callback URL / code the user pasted (which
    /// submit_callback parks on `pasted`). Gives up after `timeout`, and
    /// immediately when `stop` turns true (a cancelled login).
    pub fn wait_for_query(
        &self,
        pasted: &mpsc::Receiver<QueryMap>,
        timeout: Duration,
        stop: impl Fn() -> bool,
    ) -> OAuthResult<QueryMap> {
        let deadline = Instant::now() + timeout;
        loop {
            if stop() {
                return Err(OAuthError::Rejected("登录已取消".to_string()));
            }
            if let Ok(query) = pasted.try_recv() {
                return Ok(query);
            }
            if Instant::now() >= deadline {
                return Err(OAuthError::Rejected(
                    "等待本机回调超时，请重新发起登录".to_string(),
                ));
            }
            if let Some(query) = self.recv_query(WAIT_SLICE)? {
                return Ok(query);
            }
        }
    }

    /// Wait up to `wait` for one callback that carries a query string.
    ///
    /// Returns `Ok(None)` when the wait elapsed, or when the request had no
    /// query (a favicon probe, say); the caller loops until its own deadline.
    pub fn recv_query(&self, wait: Duration) -> OAuthResult<Option<QueryMap>> {
        match self.server.recv_timeout(wait) {
            Ok(None) => Ok(None),
            Ok(Some(request)) => match query_of(request.url()) {
                Some(query) => {
                    respond(request, 200, SUCCESS_PAGE);
                    Ok(Some(query))
                }
                None => {
                    respond(request, 400, INVALID_PAGE);
                    Ok(None)
                }
            },
            Err(error) => Err(OAuthError::Other(format!(
                "本机回调服务器读取失败: {error}"
            ))),
        }
    }
}

fn bind_port(port: u16, path: &'static str) -> Option<LoopbackServer> {
    let server = Server::http(("127.0.0.1", port)).ok()?;
    let port = server.server_addr().to_ip()?.port();
    Some(LoopbackServer { server, port, path })
}

fn query_of(url: &str) -> Option<QueryMap> {
    let (_, query) = url.split_once('?')?;
    let map: QueryMap = url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    (!map.is_empty()).then_some(map)
}

fn respond(request: tiny_http::Request, status: u16, body: &str) {
    let mut response = Response::from_string(body).with_status_code(status);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]) {
        response = response.with_header(header);
    }
    if let Ok(header) = Header::from_bytes(&b"Connection"[..], &b"close"[..]) {
        response = response.with_header(header);
    }
    let _ = request.respond(response);
}

/// Parse what the user pasted into the same query map the listener produces.
///
/// Accepts a callback URL, a `code=...` query string, or a bare authorization
/// code. A URL must point at this machine: a callback link for someone else's
/// host is a phishing attempt, not a login.
pub fn parse_callback_text(text: &str) -> OAuthResult<QueryMap> {
    let trimmed = trim_pasted(text);
    if trimmed.is_empty() {
        return Err(OAuthError::Rejected("粘贴的回调内容为空".to_string()));
    }
    if trimmed.contains("://") {
        let url = Url::parse(trimmed)
            .map_err(|error| OAuthError::Rejected(format!("回调链接无效: {error}")))?;
        if !is_loopback_host(url.host_str()) {
            return Err(OAuthError::Rejected(format!(
                "只接受 127.0.0.1/localhost 的回调链接，收到: {}",
                url.host_str().unwrap_or("(缺少主机)")
            )));
        }
        let map: QueryMap = url
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        if map.is_empty() {
            return Err(OAuthError::Rejected(
                "回调链接里没有查询参数".to_string(),
            ));
        }
        return Ok(map);
    }
    if trimmed.starts_with("code=") {
        let map: QueryMap = url::form_urlencoded::parse(trimmed.as_bytes())
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        if map.get("code").is_some_and(|code| !code.is_empty()) {
            return Ok(map);
        }
        return Err(OAuthError::Rejected("粘贴的查询串里没有 code".to_string()));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(OAuthError::Rejected(
            "粘贴的授权码不能包含空白字符".to_string(),
        ));
    }
    Ok(QueryMap::from([("code".to_string(), trimmed.to_string())]))
}

fn trim_pasted(text: &str) -> &str {
    text.trim()
        .trim_matches(|character: char| {
            matches!(character, '"' | '\'' | '`' | '<' | '>')
        })
        .trim()
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(
        host,
        Some("127.0.0.1") | Some("localhost") | Some("::1") | Some("[::1]")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    #[test]
    fn pasted_url_becomes_the_callback_query_map() {
        let map = parse_callback_text("http://localhost:1455/auth/callback?code=abc&state=s1")
            .expect("parse");
        assert_eq!(map.get("code").map(String::as_str), Some("abc"));
        assert_eq!(map.get("state").map(String::as_str), Some("s1"));
    }

    #[test]
    fn pasted_ipv4_and_ipv6_loopback_urls_are_accepted() {
        assert!(parse_callback_text("http://127.0.0.1:1455/auth/callback?code=a").is_ok());
        assert!(parse_callback_text("http://[::1]:1455/auth/callback?code=a").is_ok());
    }

    #[test]
    fn pasted_foreign_host_is_refused() {
        let error = parse_callback_text("https://evil.example/auth/callback?code=abc")
            .expect_err("foreign host must be refused");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
        assert!(error.to_string().contains("127.0.0.1"));
    }

    #[test]
    fn bare_code_becomes_the_same_map() {
        let map = parse_callback_text("  abc123-_.~  ").expect("parse");
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("code").map(String::as_str), Some("abc123-_.~"));
    }

    #[test]
    fn wrapped_code_and_query_string_are_accepted() {
        let quoted = parse_callback_text("\"abc123\"").expect("quoted");
        assert_eq!(quoted.get("code").map(String::as_str), Some("abc123"));

        let query = parse_callback_text("code=abc&state=s1").expect("query string");
        assert_eq!(query.get("code").map(String::as_str), Some("abc"));
        assert_eq!(query.get("state").map(String::as_str), Some("s1"));
    }

    #[test]
    fn blank_paste_is_rejected() {
        assert!(matches!(
            parse_callback_text("   "),
            Err(OAuthError::Rejected(_))
        ));
        assert!(matches!(
            parse_callback_text("abc def"),
            Err(OAuthError::Rejected(_))
        ));
        assert!(matches!(
            parse_callback_text("http://localhost:1455/auth/callback"),
            Err(OAuthError::Rejected(_))
        ));
    }

    #[test]
    fn pasted_callback_finishes_the_wait_without_a_browser() {
        let server = LoopbackServer::bind(&[]).expect("bind");
        let (pasted, pasted_rx) = mpsc::channel();
        pasted
            .send(QueryMap::from([("code".to_string(), "abc".to_string())]))
            .expect("park the paste");

        let query = server
            .wait_for_query(&pasted_rx, CALLBACK_TIMEOUT, || false)
            .expect("pasted callback");
        assert_eq!(query.get("code").map(String::as_str), Some("abc"));
    }

    #[test]
    fn a_silent_callback_door_times_out() {
        let server = LoopbackServer::bind(&[]).expect("bind");
        let (_pasted, pasted_rx) = mpsc::channel();
        let error = server
            .wait_for_query(&pasted_rx, Duration::from_millis(50), || false)
            .expect_err("nothing arrives");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
        assert!(error.to_string().contains("超时"));
    }

    #[test]
    fn a_cancelled_wait_stops_immediately() {
        let server = LoopbackServer::bind(&[]).expect("bind");
        let (_pasted, pasted_rx) = mpsc::channel();
        let error = server
            .wait_for_query(&pasted_rx, CALLBACK_TIMEOUT, || true)
            .expect_err("cancelled");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
        assert!(error.to_string().contains("已取消"));
    }

    #[test]
    fn redirect_uri_matches_the_bound_port() {
        let server = LoopbackServer::bind(&[]).expect("bind");
        assert_eq!(
            server.redirect_uri(),
            format!("http://localhost:{}/auth/callback", server.port())
        );
    }

    #[test]
    fn a_registered_callback_path_travels_with_the_listener() {
        let server = LoopbackServer::bind_with(&[], "/oauth-callback").expect("bind");
        assert_eq!(
            server.redirect_uri(),
            format!("http://localhost:{}/oauth-callback", server.port())
        );

        // A pasted callback for that path parses into the same query map.
        let map = parse_callback_text(&format!(
            "http://localhost:{}/oauth-callback?code=abc&state=s1",
            server.port()
        ))
        .expect("parse");
        assert_eq!(map.get("code").map(String::as_str), Some("abc"));
    }

    #[test]
    fn busy_preferred_port_falls_through() {
        // Hold one port with a plain listener, then ask the server for it.
        let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind taken port");
        let taken_port = taken.local_addr().expect("addr").port();
        let server = LoopbackServer::bind(&[taken_port]).expect("bind other port");
        assert_ne!(server.port(), taken_port);
    }

    #[test]
    fn listener_answers_the_callback_with_html() {
        let server = LoopbackServer::bind(&[]).expect("bind");
        let port = server.port();
        let handle = std::thread::spawn(move || server.recv_query(Duration::from_secs(10)));

        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        write!(
            stream,
            "GET /auth/callback?code=abc&state=s1 HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n"
        )
        .expect("request");
        let response = read_until_marker(&mut stream, "</html>");
        assert!(response.starts_with("HTTP/1.1 200"), "response: {response}");
        assert!(response.contains("登录成功"), "response: {response}");
        assert!(
            response.to_ascii_lowercase().contains("text/html"),
            "response: {response}"
        );

        let map = handle
            .join()
            .expect("thread")
            .expect("recv")
            .expect("one callback");
        assert_eq!(map.get("code").map(String::as_str), Some("abc"));
        assert_eq!(map.get("state").map(String::as_str), Some("s1"));
    }

    #[test]
    fn request_without_query_keeps_waiting() {
        let server = LoopbackServer::bind(&[]).expect("bind");
        let port = server.port();
        let handle = std::thread::spawn(move || {
            // Keep serving until the deadline: a request without a query gets a
            // 400 page but must not resolve the login.
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let mut queries = Vec::new();
            while std::time::Instant::now() < deadline {
                if let Some(query) = server.recv_query(Duration::from_millis(100)).expect("recv") {
                    queries.push(query);
                }
            }
            queries
        });

        let mut probe = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        probe
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        write!(
            probe,
            "GET /favicon.ico HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n"
        )
        .expect("request");
        let response = read_until_marker(&mut probe, "</html>");
        assert!(response.starts_with("HTTP/1.1 400"), "response: {response}");

        assert!(handle.join().expect("thread").is_empty());
    }

    fn read_until_marker(stream: &mut TcpStream, marker: &str) -> String {
        let mut collected = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    collected.extend_from_slice(&buffer[..read]);
                    if String::from_utf8_lossy(&collected).contains(marker) {
                        break;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(error) => panic!("read failed: {error}"),
            }
        }
        String::from_utf8_lossy(&collected).into_owned()
    }
}

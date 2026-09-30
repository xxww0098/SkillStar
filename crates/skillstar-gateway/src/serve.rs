//! Loopback listener for the gateway route table and the Claude MCP callback.
//!
//! The first request on a connection has `HEADER_READ_TIMEOUT` to finish its
//! headers, counted from when the connection is accepted. After a response,
//! the next request may wait `IDLE_TIMEOUT` for its first byte, then has
//! another header budget. That split is what Go's server does: the idle
//! deadline applies while peeking for the next request, and the header
//! deadline starts only once those bytes arrive. A body already in progress
//! is not cut off by either one.

use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::translate::{Protocol, outbound_body, upstream_body};

/// Listen address when `SKILLSTAR_GATEWAY_ADDR` is unset or empty.
pub const DEFAULT_ADDR: &str = "127.0.0.1:21847";

/// Environment variable that replaces [`DEFAULT_ADDR`].
pub const ADDR_ENV: &str = "SKILLSTAR_GATEWAY_ADDR";

/// Port that must not be bound. It is the other product's default.
pub const REFUSED_PORT: u16 = 3425;

/// Bearer agents are told to send. Checking a wrong bearer is a later slice.
pub const PLACEHOLDER_BEARER: &str = "skillstar";

/// How long the headers of one request may take to arrive.
pub const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a keep-alive connection may sit before the next request starts.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Why `serve` did not keep listening.
#[derive(Debug)]
pub enum ServeError {
    BadAddr(String),
    RefusedPort,
    Busy,
    Bind(io::Error),
    Runtime(io::Error),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadAddr(raw) => write!(f, "无法解析监听地址 {raw}"),
            Self::RefusedPort => write!(f, "拒绝监听端口 {REFUSED_PORT}"),
            Self::Busy => f.write_str("地址已被占用"),
            Self::Bind(error) => write!(f, "监听失败: {error}"),
            Self::Runtime(error) => write!(f, "网关运行时失败: {error}"),
        }
    }
}

impl std::error::Error for ServeError {}

/// Stops a running `serve`. Dropping it does not stop the listener.
#[derive(Clone)]
pub struct Stop {
    tx: watch::Sender<bool>,
}

impl Stop {
    /// Ask `serve` to return. Connections already accepted finish their turn.
    pub fn stop(&self) {
        let _ = self.tx.send(true);
    }
}

#[derive(Clone, Copy)]
struct InboundLimits {
    header: Duration,
    idle: Duration,
}

/// What to listen on, and where Chat turns are forwarded.
pub struct ServeOptions {
    addr: SocketAddr,
    upstream: Option<String>,
    stop_tx: watch::Sender<bool>,
    stop_rx: watch::Receiver<bool>,
    bound: Option<std::sync::mpsc::Sender<SocketAddr>>,
    limits: InboundLimits,
}

impl ServeOptions {
    /// Address from the environment, refusing port [`REFUSED_PORT`].
    pub fn from_env() -> Result<Self, ServeError> {
        Ok(Self::bind(resolve_addr()?))
    }

    /// Listen on `addr`. `serve` still refuses port [`REFUSED_PORT`].
    pub fn bind(addr: SocketAddr) -> Self {
        let (stop_tx, stop_rx) = watch::channel(false);
        Self {
            addr,
            upstream: None,
            stop_tx,
            stop_rx,
            bound: None,
            limits: InboundLimits {
                header: HEADER_READ_TIMEOUT,
                idle: IDLE_TIMEOUT,
            },
        }
    }

    /// Chat Completions origin, without a path. `http://127.0.0.1:9`.
    pub fn upstream(mut self, base: impl Into<String>) -> Self {
        self.upstream = Some(base.into());
        self
    }

    /// Receives the address after the socket is bound. Port 0 is filled in.
    pub fn on_bound(mut self, tx: std::sync::mpsc::Sender<SocketAddr>) -> Self {
        self.bound = Some(tx);
        self
    }

    /// Handle that makes [`serve`] return.
    pub fn stop_handle(&self) -> Stop {
        Stop {
            tx: self.stop_tx.clone(),
        }
    }

    #[cfg(test)]
    fn with_timeouts(mut self, header: Duration, idle: Duration) -> Self {
        self.limits = InboundLimits { header, idle };
        self
    }
}

/// `SKILLSTAR_GATEWAY_ADDR`, or [`DEFAULT_ADDR`] when that variable is empty.
pub fn resolve_addr() -> Result<SocketAddr, ServeError> {
    let raw = match std::env::var(ADDR_ENV) {
        Ok(value) if !value.is_empty() => value,
        _ => DEFAULT_ADDR.to_string(),
    };
    let mut addr: SocketAddr = raw.parse().map_err(|_| ServeError::BadAddr(raw))?;
    if addr.port() == REFUSED_PORT {
        return Err(ServeError::RefusedPort);
    }
    if crate::listen::listen_is_lan() {
        addr.set_ip(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }
    Ok(addr)
}

/// Base URL written into agent files. A wildcard listen is still loopback here.
pub fn published_origin() -> String {
    let port = resolve_addr().map(|addr| addr.port()).unwrap_or(21847);
    format!("http://127.0.0.1:{port}")
}

/// Bind and answer the gateway route table until [`Stop::stop`].
///
/// A second bind of the same address returns [`ServeError::Busy`] and leaves
/// the first listener running. stdout is not used.
pub fn serve(options: ServeOptions) -> Result<(), ServeError> {
    if options.addr.port() == REFUSED_PORT {
        return Err(ServeError::RefusedPort);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(ServeError::Runtime)?;
    runtime.block_on(run(options))
}

async fn run(options: ServeOptions) -> Result<(), ServeError> {
    let listener = TcpListener::bind(options.addr)
        .await
        .map_err(classify_bind)?;
    let bound = listener.local_addr().map_err(ServeError::Bind)?;
    if let Some(tx) = &options.bound {
        let _ = tx.send(bound);
    }
    let upstream = options.upstream.clone();
    let mut stop = options.stop_rx;
    loop {
        tokio::select! {
            result = stop.changed() => {
                if result.is_err() || *stop.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, peer) = match accepted {
                    Ok(pair) => pair,
                    Err(_) => continue,
                };
                let upstream = upstream.clone();
                let limits = options.limits;
                tokio::spawn(async move {
                    let finished = Arc::new(AtomicBool::new(false));
                    let io = TokioIo::new(BudgetIo::new(stream, Arc::clone(&finished), limits));
                    let service = service_fn(move |request| {
                        let upstream = upstream.clone();
                        let finished = Arc::clone(&finished);
                        async move {
                            let response = dispatch(request, upstream.as_deref(), peer).await;
                            finished.store(true, Ordering::Relaxed);
                            Ok::<_, std::convert::Infallible>(response)
                        }
                    });
                    let connection = http1::Builder::new()
                        .header_read_timeout(None)
                        .serve_connection(io, service);
                    let _ = connection.await;
                });
            }
        }
    }
    Ok(())
}

fn classify_bind(error: io::Error) -> ServeError {
    if error.kind() == ErrorKind::AddrInUse {
        ServeError::Busy
    } else {
        ServeError::Bind(error)
    }
}

async fn dispatch(
    request: Request<Incoming>,
    upstream: Option<&str>,
    peer: SocketAddr,
) -> Response<Full<Bytes>> {
    let authorization = header_text(request.headers(), hyper::header::AUTHORIZATION);
    let user_agent = header_text(request.headers(), hyper::header::USER_AGENT);
    if request.method() == Method::POST
        && let Some(token) = crate::claude::callback_token(request.uri().path())
    {
        let token = token.to_string();
        let body = match request.into_body().collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return plain(StatusCode::BAD_REQUEST, "bad request"),
        };
        return claude_callback(peer, &token, &body).await;
    }
    let head = request.method() == Method::HEAD;
    let upgrade = request
        .headers()
        .get(hyper::header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let agent = crate::rules::request_agent(&crate::rules::Caller {
        authorization: &authorization,
        user_agent: &user_agent,
        ..crate::rules::Caller::default()
    });
    match crate::surface::plan(method.as_str(), &path, upgrade.as_deref(), &agent) {
        crate::surface::Plan::Local(local) => respond_local(local, head),
        crate::surface::Plan::WithBody(kind) => {
            let inbound = match request.into_body().collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => return plain(StatusCode::BAD_REQUEST, "bad request"),
            };
            match crate::surface::finish(kind, &path, &inbound) {
                crate::surface::Outcome::Local(local) => respond_local(local, head),
                crate::surface::Outcome::Forward { protocol, url_path } => {
                    forward_turn(
                        upstream,
                        protocol,
                        &url_path,
                        inbound,
                        &authorization,
                        &user_agent,
                    )
                    .await
                }
            }
        }
    }
}

struct Turn {
    status: StatusCode,
    body: Vec<u8>,
    json: bool,
}

impl Turn {
    fn text(status: StatusCode, message: &str) -> Self {
        Self {
            status,
            body: message.as_bytes().to_vec(),
            json: false,
        }
    }

    fn json(status: StatusCode, body: Vec<u8>) -> Self {
        Self {
            status,
            body,
            json: true,
        }
    }
}

async fn forward_turn(
    upstream: Option<&str>,
    protocol: Option<Protocol>,
    url_path: &str,
    inbound: Bytes,
    authorization: &str,
    user_agent: &str,
) -> Response<Full<Bytes>> {
    let turned = forward_body(upstream, protocol, url_path, &inbound).await;
    crate::trace::note_forward(
        authorization,
        user_agent,
        &inbound,
        turned.status.as_u16(),
        &turned.body,
    );
    let content_type = if turned.json {
        "application/json"
    } else {
        "text/plain; charset=utf-8"
    };
    Response::builder()
        .status(turned.status)
        .header(hyper::header::CONTENT_TYPE, content_type)
        .body(Full::new(Bytes::from(turned.body)))
        .unwrap_or_else(|_| plain(StatusCode::INTERNAL_SERVER_ERROR, "response"))
}

async fn forward_body(
    upstream: Option<&str>,
    protocol: Option<Protocol>,
    url_path: &str,
    inbound: &Bytes,
) -> Turn {
    let upstream_bytes = match protocol {
        Some(protocol) => match upstream_body(protocol, inbound) {
            Ok(body) => crate::effort::apply_upstream_effort(&body, ""),
            Err(_) => return Turn::text(StatusCode::BAD_REQUEST, "bad request"),
        },
        None => inbound.to_vec(),
    };
    let Some(base) = upstream else {
        return Turn::text(StatusCode::BAD_GATEWAY, "no upstream");
    };
    // After translation, before redaction. Raw image routes have no protocol
    // and stay byte-for-byte; a missing vision id leaves the body alone.
    let upstream_bytes = if protocol.is_some() {
        match crate::vision::rewrite_forward(&upstream_bytes, base).await {
            Ok(bytes) => bytes,
            Err(reject) => {
                let status = StatusCode::from_u16(reject.status).unwrap_or(StatusCode::BAD_GATEWAY);
                return Turn::text(status, &reject.message);
            }
        }
    } else {
        upstream_bytes
    };
    let upstream_bytes = crate::redact::mask_outbound(&upstream_bytes);
    let url = format!("{}{url_path}", base.trim_end_matches('/'));
    crate::outbound::note_outbound(&url);
    let client = match skillstar_core::infra::http_client::stream_http_client() {
        Ok(client) => client,
        Err(_) => return Turn::text(StatusCode::BAD_GATEWAY, "upstream client"),
    };
    let pending = client
        .post(url)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(upstream_bytes);
    let response = match skillstar_core::infra::http_client::send_stream(pending).await {
        Ok(response) => response,
        Err(_) => return Turn::text(StatusCode::BAD_GATEWAY, "upstream request"),
    };
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(_) => return Turn::text(StatusCode::BAD_GATEWAY, "upstream body"),
    };
    let bytes = crate::redact::unmask_response(&bytes);
    let outbound = match protocol {
        Some(protocol) => match outbound_body(protocol, &bytes) {
            Ok(body) => body,
            Err(_) => return Turn::text(StatusCode::BAD_GATEWAY, "upstream body"),
        },
        None => bytes.to_vec(),
    };
    Turn::json(status, outbound)
}

fn header_text(headers: &hyper::HeaderMap, name: hyper::header::HeaderName) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn respond_local(local: crate::surface::Local, head: bool) -> Response<Full<Bytes>> {
    let status = StatusCode::from_u16(local.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = if head { Vec::new() } else { local.body };
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, local.content_type)
        .body(Full::new(Bytes::from(body)))
        .unwrap_or_else(|_| plain(StatusCode::INTERNAL_SERVER_ERROR, "response"))
}

async fn claude_callback(peer: SocketAddr, token: &str, body: &[u8]) -> Response<Full<Bytes>> {
    use crate::claude::{CallbackOutcome, begin_callback, listener_bridge};

    match begin_callback(listener_bridge(), peer, token, body) {
        CallbackOutcome::Ready { status, body } => {
            let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            plain(status, &body)
        }
        CallbackOutcome::Wait(rx) => {
            let received = tokio::task::spawn_blocking(move || rx.recv()).await;
            match received {
                Ok(Ok(result)) => json_body(StatusCode::OK, result.json_bytes()),
                _ => plain(StatusCode::GONE, "the agent's run ended"),
            }
        }
    }
}

fn json_body(status: StatusCode, bytes: Vec<u8>) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Full::new(Bytes::from(bytes)))
        .unwrap_or_else(|_| plain(StatusCode::INTERNAL_SERVER_ERROR, "response"))
}

fn plain(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(message.to_string())))
        .expect("plain response uses a valid status")
}

enum ReadMode {
    First { since: Instant },
    Idle { since: Instant },
    Headers { since: Instant },
    Body,
}

struct BudgetIo {
    inner: TcpStream,
    response_finished: Arc<AtomicBool>,
    limits: InboundLimits,
    mode: ReadMode,
    tail: [u8; 3],
    tail_len: usize,
    sleep: Option<Pin<Box<tokio::time::Sleep>>>,
    sleep_deadline: Option<Instant>,
}

impl BudgetIo {
    fn new(inner: TcpStream, response_finished: Arc<AtomicBool>, limits: InboundLimits) -> Self {
        Self {
            inner,
            response_finished,
            limits,
            mode: ReadMode::First {
                since: Instant::now(),
            },
            tail: [0; 3],
            tail_len: 0,
            sleep: None,
            sleep_deadline: None,
        }
    }

    fn observe_response(&mut self) {
        if self.response_finished.swap(false, Ordering::Relaxed)
            && matches!(self.mode, ReadMode::Body)
        {
            self.mode = ReadMode::Idle {
                since: Instant::now(),
            };
            self.tail_len = 0;
            self.sleep = None;
            self.sleep_deadline = None;
        }
    }

    fn deadline(&self) -> Option<Instant> {
        let (since, budget) = match self.mode {
            ReadMode::First { since } | ReadMode::Headers { since } => (since, self.limits.header),
            ReadMode::Idle { since } => (since, self.limits.idle),
            ReadMode::Body => return None,
        };
        Some(since.checked_add(budget).unwrap_or(since))
    }

    fn deadline_fired(&mut self, cx: &mut Context<'_>) -> bool {
        let Some(deadline) = self.deadline() else {
            self.sleep = None;
            self.sleep_deadline = None;
            return false;
        };
        if Instant::now() >= deadline {
            return true;
        }
        if self.sleep_deadline != Some(deadline) {
            self.sleep = Some(Box::pin(tokio::time::sleep_until(deadline.into())));
            self.sleep_deadline = Some(deadline);
        }
        matches!(
            self.sleep.as_mut().unwrap().as_mut().poll(cx),
            Poll::Ready(())
        )
    }

    fn note(&mut self, chunk: &[u8]) {
        if chunk.is_empty() || matches!(self.mode, ReadMode::Body) {
            return;
        }
        if matches!(self.mode, ReadMode::Idle { .. }) {
            self.mode = ReadMode::Headers {
                since: Instant::now(),
            };
            self.tail_len = 0;
            self.sleep = None;
            self.sleep_deadline = None;
        }
        if header_finished(&mut self.tail, &mut self.tail_len, chunk) {
            self.mode = ReadMode::Body;
            self.sleep = None;
            self.sleep_deadline = None;
        }
    }
}

fn header_finished(tail: &mut [u8; 3], tail_len: &mut usize, chunk: &[u8]) -> bool {
    let mut window = Vec::with_capacity(*tail_len + chunk.len());
    window.extend_from_slice(&tail[..*tail_len]);
    window.extend_from_slice(chunk);
    let found = window.windows(4).any(|part| part == b"\r\n\r\n");
    let keep = window.len().min(3);
    if keep > 0 {
        let start = window.len() - keep;
        tail[..keep].copy_from_slice(&window[start..]);
    }
    *tail_len = keep;
    found
}

fn timeout_err() -> io::Error {
    io::Error::new(ErrorKind::TimedOut, "inbound timeout")
}

impl AsyncRead for BudgetIo {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.observe_response();
        if this.deadline_fired(cx) {
            return Poll::Ready(Err(timeout_err()));
        }
        let filled = buf.filled().len();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let read = buf.filled()[filled..].to_vec();
                this.note(&read);
                if this.deadline_fired(cx) {
                    Poll::Ready(Err(timeout_err()))
                } else {
                    Poll::Ready(Ok(()))
                }
            }
            other => other,
        }
    }
}

impl AsyncWrite for BudgetIo {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::thread;

    fn serve_for(
        header: Duration,
        idle: Duration,
    ) -> (SocketAddr, Stop, thread::JoinHandle<Result<(), ServeError>>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let options = ServeOptions::bind("127.0.0.1:0".parse().unwrap())
            .with_timeouts(header, idle)
            .on_bound(tx);
        let stop = options.stop_handle();
        let handle = thread::spawn(move || serve(options));
        let addr = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        (addr, stop, handle)
    }

    fn expect_close(sock: &mut TcpStream) {
        sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut buf = [0u8; 256];
        loop {
            match sock.read(&mut buf) {
                Ok(0) => return,
                Ok(_) => continue,
                Err(error) if error.kind() == ErrorKind::ConnectionReset => return,
                Err(error) => panic!("connection stayed open: {error}"),
            }
        }
    }

    #[test]
    fn first_request_headers_time_out() {
        let (addr, stop, handle) = serve_for(Duration::from_millis(200), Duration::from_secs(5));
        let mut sock = TcpStream::connect(addr).unwrap();
        expect_close(&mut sock);
        stop.stop();
        handle.join().unwrap().unwrap();
    }

    #[test]
    fn slow_header_uses_header_budget_not_idle() {
        let (addr, stop, handle) = serve_for(Duration::from_millis(200), Duration::from_secs(5));
        let mut sock = TcpStream::connect(addr).unwrap();
        sock.write_all(b"POST /v1/chat/completions HTTP/1.1\r\nHost: x\r\n")
            .unwrap();
        expect_close(&mut sock);
        stop.stop();
        handle.join().unwrap().unwrap();
    }

    #[test]
    fn idle_gap_times_out_after_a_response() {
        let (addr, stop, handle) = serve_for(Duration::from_secs(5), Duration::from_millis(200));
        let mut sock = TcpStream::connect(addr).unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let body = b"{}";
        let header = format!(
            "POST /nope HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        sock.write_all(header.as_bytes()).unwrap();
        sock.write_all(body).unwrap();
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            match sock.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
                Err(error) if error.kind() == ErrorKind::ConnectionReset => break,
                Err(error) => panic!("idle did not close the keep-alive: {error}"),
            }
        }
        assert!(
            buf.windows(4).any(|window| window == b"\r\n\r\n"),
            "the response should arrive before the idle close"
        );
        stop.stop();
        handle.join().unwrap().unwrap();
    }
}

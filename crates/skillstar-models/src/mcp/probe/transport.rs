//! The probe's transport seam: one narrow trait, one stdio implementation, one
//! HTTP implementation.
//!
//! The epoch-detection logic in `mod.rs` is written against [`ProbeTransport`]
//! and nothing else. That is what lets the whole state machine — which is
//! where the actual complexity lives — be tested with a scripted fake, with no
//! process spawned and no socket opened.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use super::rpc::{self, JsonRpcError};

/// How long a single request may take before the peer is considered silent.
pub(crate) const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// One request the probe wants to make.
pub(crate) struct ProbeCall<'a> {
    pub id: u64,
    pub method: &'a str,
    pub params: Value,
    /// Protocol version to advertise. On HTTP this must also go out as the
    /// `MCP-Protocol-Version` header, or a conforming server answers `400`.
    pub protocol_version: &'a str,
}

/// What came back. Deliberately coarser than a JSON-RPC response: the epoch
/// decision only cares about these four outcomes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProbeReply {
    /// A JSON-RPC `result` object.
    Result(Value),
    /// A JSON-RPC `error` object.
    Error(JsonRpcError),
    /// The endpoint demands authorization (`401` + `WWW-Authenticate`). Not a
    /// failure — it is the signal to start an OAuth flow.
    Unauthorized { challenge: Option<String> },
    /// The peer rejected the request in a way that carries no proof of which
    /// epoch it speaks (an empty `400` body, a `404`, a `405`). The caller
    /// falls back rather than concluding anything.
    Inconclusive(String),
}

/// A single JSON-RPC round trip against one server.
pub(crate) trait ProbeTransport {
    /// Send a request and wait for its response.
    async fn call(&mut self, call: ProbeCall<'_>) -> Result<ProbeReply>;

    /// Send a notification. Notifications have no response; a transport that
    /// cannot express them (or a peer that does not need them) may no-op.
    async fn notify(&mut self, method: &str, params: Value) -> Result<()>;
}

/// Split a JSON-RPC response envelope into the coarse [`ProbeReply`] shape.
pub(crate) fn reply_from_envelope(envelope: &Value) -> Result<ProbeReply> {
    if let Some(error) = envelope.get("error") {
        let error: JsonRpcError = serde_json::from_value(error.clone())
            .context("Malformed JSON-RPC error object in the server's response")?;
        return Ok(ProbeReply::Error(error));
    }
    match envelope.get("result") {
        Some(result) => Ok(ProbeReply::Result(result.clone())),
        None => bail!("The server's response carried neither `result` nor `error`"),
    }
}

// ---------------------------------------------------------------------------
// stdio
// ---------------------------------------------------------------------------

/// Resolve an stdio server's launcher on the enriched PATH.
///
/// GUI-launched desktop apps inherit a minimal login PATH, so the raw process
/// PATH regularly misses Homebrew, `~/.local/bin`, and the toolchains that own
/// `npx` / `uvx` / `dnx`. Resolving here (and passing the absolute path to
/// the spawn) means the probe answers "is this runtime installed?" the same
/// way whether SkillStar was started from a terminal or from the Dock.
pub fn resolve_runtime(command: &str) -> Result<PathBuf> {
    skillstar_core::infra::path_env::which_in_enriched(command).with_context(|| {
        format!(
            "'{command}' was not found on this machine. Install the runtime it belongs to (for example Node.js for `npx`, uv for `uvx`, Docker for `docker`, or the .NET SDK for `dnx`) and try again."
        )
    })
}

/// A child process speaking newline-delimited JSON-RPC over stdin/stdout.
pub(crate) struct StdioTransport {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    timeout: Duration,
}

impl StdioTransport {
    /// Launch the server described by `command` / `args` / `env` / `cwd`.
    ///
    /// The command is executed **directly**, never through a shell: the
    /// arguments here come from registry metadata, and handing a registry
    /// author's string to `sh -c` would make every `server.json` a remote code
    /// execution primitive. `stderr` is discarded because the spec is explicit
    /// that stderr output is not an error signal.
    pub(crate) async fn spawn(
        command: &str,
        args: &[String],
        env: &std::collections::BTreeMap<String, String>,
        cwd: Option<&str>,
        timeout: Duration,
    ) -> Result<Self> {
        let program = resolve_runtime(command)?;
        let mut builder = Command::new(&program);
        builder
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for (key, value) in env {
            builder.env(key, value);
        }
        if let Some(cwd) = cwd.filter(|c| !c.trim().is_empty()) {
            builder.current_dir(cwd);
        }
        let mut child = builder
            .spawn()
            .with_context(|| format!("Failed to start '{}'", program.display()))?;
        let stdin = child
            .stdin
            .take()
            .context("The MCP server process exposed no stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("The MCP server process exposed no stdout")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            timeout,
        })
    }

    /// Close stdin — the only portable graceful-shutdown signal for an stdio
    /// server — and reap the child.
    pub(crate) async fn shutdown(mut self) {
        let _ = self.stdin.shutdown().await;
        let _ = self.child.kill().await;
    }

    async fn write_line(&mut self, value: &Value) -> Result<()> {
        let mut line =
            serde_json::to_string(value).context("Failed to serialize a JSON-RPC message")?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .context("Failed to write to the MCP server's stdin")?;
        self.stdin
            .flush()
            .await
            .context("Failed to flush the MCP server's stdin")
    }

    /// Read lines until one parses as a response carrying `id`.
    ///
    /// Anything else on the channel — notifications, progress, a server's own
    /// chatter — is skipped rather than treated as an answer.
    async fn read_response(&mut self, id: u64) -> Result<Value> {
        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .await
                .context("Failed to read from the MCP server's stdout")?;
            if read == 0 {
                bail!("The MCP server closed its stdout before answering");
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(value);
            }
        }
    }
}

impl ProbeTransport for StdioTransport {
    async fn call(&mut self, call: ProbeCall<'_>) -> Result<ProbeReply> {
        let envelope = rpc::request_envelope(call.id, call.method, &call.params);
        self.write_line(&envelope).await?;
        let response = tokio::time::timeout(self.timeout, self.read_response(call.id))
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "The MCP server did not answer `{}` within {}s",
                    call.method,
                    self.timeout.as_secs()
                )
            })??;
        reply_from_envelope(&response)
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let envelope = rpc::notification_envelope(method, &params);
        self.write_line(&envelope).await
    }
}

// ---------------------------------------------------------------------------
// Streamable HTTP
// ---------------------------------------------------------------------------

/// A remote MCP endpoint reached over Streamable HTTP.
pub(crate) struct HttpTransport {
    client: reqwest::Client,
    url: String,
    headers: std::collections::BTreeMap<String, String>,
}

impl HttpTransport {
    /// Build a transport on SkillStar's shared, proxy-aware client.
    ///
    /// Every remote call in the app goes through `probe_http_client` so the
    /// user's proxy settings are honored in one place; a hand-rolled
    /// `reqwest::Client` here would quietly ignore them.
    pub(crate) fn new(
        url: &str,
        headers: &std::collections::BTreeMap<String, String>,
        timeout: Duration,
    ) -> Result<Self> {
        let client = skillstar_core::infra::http_client::probe_http_client(timeout)
            .context("Failed to build the HTTP client used to reach the MCP server")?;
        Ok(Self {
            client,
            url: url.to_string(),
            headers: headers.clone(),
        })
    }
}

impl ProbeTransport for HttpTransport {
    async fn call(&mut self, call: ProbeCall<'_>) -> Result<ProbeReply> {
        let envelope = rpc::request_envelope(call.id, call.method, &call.params);
        let mut request = self
            .client
            .post(&self.url)
            // Both media types are mandatory: the server chooses per request
            // whether to answer with a single JSON object or an SSE stream.
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            // The two routing headers the stateless revision requires, so
            // gateways can route and rate-limit without parsing the body.
            .header("MCP-Protocol-Version", call.protocol_version)
            .header("Mcp-Method", call.method);
        for (name, value) in &self.headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request
            .json(&envelope)
            .send()
            .await
            .with_context(|| format!("Failed to reach the MCP server at {}", self.url))?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            let challenge = response
                .headers()
                .get(reqwest::header::WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            return Ok(ProbeReply::Unauthorized { challenge });
        }
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            let envelope = parse_http_body(&body).with_context(|| {
                format!("The MCP server at {} returned an unreadable body", self.url)
            })?;
            return reply_from_envelope(&envelope);
        }

        // A `400` is where the epoch question is actually decided, so the body
        // is inspected before any verdict: a recognizable MCP error means the
        // peer *is* modern and merely disagreed about the version, whereas an
        // empty or unparseable body proves nothing and must fall back.
        if let Some(envelope) = parse_http_body(&body).ok()
            && let Ok(reply) = reply_from_envelope(&envelope)
        {
            return Ok(reply);
        }
        Ok(ProbeReply::Inconclusive(format!(
            "HTTP {status} from {}{}",
            self.url,
            summarize_body(&body)
        )))
    }

    async fn notify(&mut self, _method: &str, _params: Value) -> Result<()> {
        // The stateless revision has no client→server notification the probe
        // needs, and the legacy `notifications/initialized` is a no-op for a
        // stateless HTTP endpoint.
        Ok(())
    }
}

/// Parse a response body that may be either a JSON object or an SSE stream.
fn parse_http_body(body: &str) -> Result<Value> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        bail!("empty response body");
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }
    first_sse_json(trimmed).context("response body was neither JSON nor a readable SSE stream")
}

/// Pull the first JSON payload out of an SSE stream.
///
/// A request-scoped stream may carry progress notifications ahead of the
/// actual response, so every `data:` frame is tried until one parses as a
/// response envelope.
fn first_sse_json(body: &str) -> Option<Value> {
    for line in body.lines() {
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(payload)
            && (value.get("result").is_some() || value.get("error").is_some())
        {
            return Some(value);
        }
    }
    None
}

/// A short, log-safe excerpt of an unexpected body.
fn summarize_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let excerpt: String = trimmed.chars().take(200).collect();
    format!(": {excerpt}")
}

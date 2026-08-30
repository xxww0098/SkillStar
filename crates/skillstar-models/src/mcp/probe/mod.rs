//! Post-install health check: does this server actually run, and which MCP
//! revision does it speak?
//!
//! ## Two epochs, one probe
//!
//! The `2026-07-28` revision deleted the `initialize` handshake. There is no
//! version negotiation left to hang a check on, so "is this server healthy?"
//! now has to start by working out *which protocol the server implements* —
//! and it has to do that without a handshake to ask:
//!
//! ```text
//! modern   stdio → server/discover ─ result ──────────────→ modern
//!                                  └ MCP-reserved error ──→ modern (retry version)
//!                                  └ anything else ───────→ try legacy
//!          HTTP  → POST server/discover (required headers + _meta)
//!                                  └ 400 w/ recognizable  → modern (retry version)
//!                                  └ 400 empty / 404 / 405 → try legacy
//!                                  └ 401 + WWW-Authenticate → needs authorization
//! legacy         → initialize → notifications/initialized
//! both           → tools/list  (also yields ttlMs / cacheScope)
//! ```
//!
//! The distinction that makes this work is [`JsonRpcError::proves_modern`]: a
//! code inside the MCP-reserved band is vocabulary only a stateless-revision
//! server has. A plain `-32601 Method not found` is what a legacy server says
//! to `server/discover`, and proves nothing.
//!
//! ## What is *not* a failure
//!
//! A remote server answering `401` with a `WWW-Authenticate` challenge is
//! working correctly — it is telling us to authorize. It gets its own status
//! so the UI can start an OAuth flow instead of showing a red cross. (The flow
//! itself lives elsewhere.)
//!
//! ## Testability
//!
//! Everything above runs against the [`ProbeTransport`] trait, so the state
//! machine is exercised with a scripted fake. No test here starts a process or
//! opens a socket.

mod rpc;
mod transport;

#[cfg(test)]
mod tests;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use ts_rs::TS;

use super::*;
use rpc::{CacheHint, JsonRpcError};
use transport::{
    DEFAULT_CALL_TIMEOUT, HttpTransport, ProbeCall, ProbeReply, ProbeTransport, StdioTransport,
};

pub use rpc::{LEGACY_PROTOCOL_VERSION, MODERN_PROTOCOL_VERSION};
pub use transport::resolve_runtime;

/// Which MCP revision a server turned out to speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "McpSpecEpoch.ts")]
pub enum McpSpecEpoch {
    /// `2026-07-28` and later: stateless, `server/discover`, no handshake.
    Modern,
    /// `2025-11-25` and earlier: `initialize` + `notifications/initialized`.
    Legacy,
}

/// Outcome of one health check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "McpProbeStatus.ts")]
pub enum McpProbeStatus {
    /// The server answered `tools/list`.
    Healthy,
    /// The server requires authorization before it will answer. Expected on a
    /// first contact with a remote server, and the trigger for OAuth — not an
    /// error.
    AuthorizationRequired,
    /// An stdio server's launcher is not installed on this machine.
    RuntimeMissing,
    /// The server could not be reached, started, or understood.
    Unreachable,
}

/// What a health check found.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpProbeReport.ts")]
pub struct McpProbeReport {
    pub server_id: String,
    pub server_name: String,
    pub status: McpProbeStatus,
    /// Which revision the server speaks. `None` when contact never got far
    /// enough to tell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<McpSpecEpoch>,
    /// The protocol version the successful exchange used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    /// Tool names reported by `tools/list`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// `server/discover`'s natural-language guidance for the model. Worth
    /// showing verbatim on a server's detail page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// `ttlMs` from the `tools/list` result, when the server sent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub cache_ttl_ms: Option<u64>,
    /// True when `cacheScope` was `private` — the listing must not be reused
    /// across authorization contexts.
    #[serde(default)]
    pub cache_private: bool,
    /// The `WWW-Authenticate` challenge, when the server asked for
    /// authorization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_challenge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[ts(type = "number")]
    pub checked_at: u64,
}

impl McpProbeReport {
    fn new(entry: &McpServerEntry, status: McpProbeStatus) -> Self {
        Self {
            server_id: entry.id.clone(),
            server_name: entry.name.clone(),
            status,
            epoch: None,
            protocol_version: None,
            tools: Vec::new(),
            instructions: None,
            cache_ttl_ms: None,
            cache_private: false,
            auth_challenge: None,
            error: None,
            checked_at: now_ms(),
        }
    }

    fn failed(entry: &McpServerEntry, status: McpProbeStatus, error: String) -> Self {
        let mut report = Self::new(entry, status);
        report.error = Some(error);
        report
    }
}

// ---------------------------------------------------------------------------
// Epoch cache
// ---------------------------------------------------------------------------

/// Remembered epoch per server identity.
///
/// The spec calls the epoch a property of the *server*, cacheable per process
/// (stdio) or per origin (HTTP) — not per entry. Two store entries pointing at
/// the same remote host therefore share one answer, and the expensive half of
/// the probe (discovering the epoch by trial) happens once.
///
/// Entries are evicted whenever a probe fails, so a server that is upgraded,
/// restarted, or temporarily broken is re-discovered rather than being pinned
/// to a stale verdict.
static EPOCH_CACHE: LazyLock<Mutex<HashMap<String, McpSpecEpoch>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The identity an epoch verdict is cached against.
///
/// stdio keys on what launches the process (the resolved command plus its
/// arguments) because that is what determines which server binary answers.
/// HTTP keys on the origin, since one origin is one deployment regardless of
/// how many paths or entries point at it.
fn epoch_cache_key(entry: &McpServerEntry) -> Option<String> {
    match entry.transport.as_str() {
        "http" | "sse" => {
            let url = entry.url.as_deref()?;
            let parsed = url::Url::parse(url).ok()?;
            let host = parsed.host_str()?;
            Some(match parsed.port() {
                Some(port) => format!("origin:{}://{host}:{port}", parsed.scheme()),
                None => format!("origin:{}://{host}", parsed.scheme()),
            })
        }
        _ => {
            let command = entry.command.as_deref()?;
            Some(format!("process:{command} {}", entry.args.join(" ")))
        }
    }
}

fn cached_epoch(key: &str) -> Option<McpSpecEpoch> {
    EPOCH_CACHE.lock().ok()?.get(key).copied()
}

fn remember_epoch(key: &str, epoch: McpSpecEpoch) {
    if let Ok(mut cache) = EPOCH_CACHE.lock() {
        cache.insert(key.to_string(), epoch);
    }
}

fn forget_epoch(key: &str) {
    if let Ok(mut cache) = EPOCH_CACHE.lock() {
        cache.remove(key);
    }
}

/// Drop every remembered epoch. Exposed for callers that know the world
/// changed underneath them (a bulk re-sync, a test).
pub fn clear_mcp_epoch_cache() {
    if let Ok(mut cache) = EPOCH_CACHE.lock() {
        cache.clear();
    }
}

// ---------------------------------------------------------------------------
// Public probe entry point
// ---------------------------------------------------------------------------

/// Health-check one server: start (or reach) it, work out its epoch, and list
/// its tools.
///
/// Never returns `Err` — a probe that fails is a *result*, not an exception,
/// and the report carries which kind of failure it was.
pub async fn probe_server(entry: &McpServerEntry) -> McpProbeReport {
    let timeout = entry
        .timeout_ms
        .filter(|&ms| ms > 0)
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_CALL_TIMEOUT);
    match entry.transport.as_str() {
        "http" | "sse" => probe_remote(entry, timeout).await,
        _ => probe_stdio(entry, timeout).await,
    }
}

async fn probe_remote(entry: &McpServerEntry, timeout: Duration) -> McpProbeReport {
    let Some(url) = entry.url.as_deref().filter(|u| !u.trim().is_empty()) else {
        return McpProbeReport::failed(
            entry,
            McpProbeStatus::Unreachable,
            format!("{} MCP server '{}' has no url", entry.transport, entry.name),
        );
    };
    let mut transport = match HttpTransport::new(url, &entry.headers, timeout) {
        Ok(transport) => transport,
        Err(e) => {
            return McpProbeReport::failed(entry, McpProbeStatus::Unreachable, e.to_string());
        }
    };
    run_probe(entry, &mut transport).await
}

async fn probe_stdio(entry: &McpServerEntry, timeout: Duration) -> McpProbeReport {
    let Some(command) = entry.command.as_deref().filter(|c| !c.trim().is_empty()) else {
        return McpProbeReport::failed(
            entry,
            McpProbeStatus::Unreachable,
            format!("stdio MCP server '{}' requires a command", entry.name),
        );
    };
    // A missing runtime is its own status: "install Node" is a different
    // instruction from "this server is broken", and conflating them sends the
    // user looking in the wrong place.
    if let Err(e) = resolve_runtime(command) {
        return McpProbeReport::failed(entry, McpProbeStatus::RuntimeMissing, e.to_string());
    }
    let mut transport = match StdioTransport::spawn(
        command,
        &entry.args,
        &entry.env,
        entry.cwd.as_deref(),
        timeout,
    )
    .await
    {
        Ok(transport) => transport,
        Err(e) => {
            return McpProbeReport::failed(entry, McpProbeStatus::Unreachable, e.to_string());
        }
    };
    let report = run_probe(entry, &mut transport).await;
    transport.shutdown().await;
    report
}

// ---------------------------------------------------------------------------
// The epoch state machine (transport-agnostic — this is what tests drive)
// ---------------------------------------------------------------------------

/// Everything one epoch attempt learned.
#[derive(Default)]
struct Discovery {
    protocol_version: Option<String>,
    instructions: Option<String>,
}

/// Run the full check against an already-connected transport.
async fn run_probe<T: ProbeTransport>(entry: &McpServerEntry, transport: &mut T) -> McpProbeReport {
    let cache_key = epoch_cache_key(entry);
    let hint = cache_key.as_deref().and_then(cached_epoch);
    let mut next_id = 1u64;

    let (epoch, discovery) = match hint {
        // A remembered verdict is a starting point, not a promise: if the
        // server was upgraded (or downgraded) since, the attempt fails and the
        // other epoch is tried, exactly as if nothing had been cached.
        Some(McpSpecEpoch::Legacy) => match try_legacy(transport, &mut next_id).await {
            Ok(discovery) => (McpSpecEpoch::Legacy, discovery),
            Err(_) => match try_modern(transport, &mut next_id).await {
                Ok(EpochAttempt::Modern(discovery)) => (McpSpecEpoch::Modern, discovery),
                Ok(EpochAttempt::Unauthorized { challenge }) => {
                    return authorization_required(entry, challenge);
                }
                Ok(EpochAttempt::NotModern(reason)) | Err(reason) => {
                    return unreachable(entry, cache_key.as_deref(), reason);
                }
            },
        },
        Some(McpSpecEpoch::Modern) | None => match try_modern(transport, &mut next_id).await {
            Ok(EpochAttempt::Modern(discovery)) => (McpSpecEpoch::Modern, discovery),
            Ok(EpochAttempt::Unauthorized { challenge }) => {
                return authorization_required(entry, challenge);
            }
            Ok(EpochAttempt::NotModern(modern_reason)) | Err(modern_reason) => {
                match try_legacy(transport, &mut next_id).await {
                    Ok(discovery) => (McpSpecEpoch::Legacy, discovery),
                    Err(legacy_reason) => {
                        return unreachable(
                            entry,
                            cache_key.as_deref(),
                            format!(
                                "the server answered neither revision — modern: {modern_reason}; legacy: {legacy_reason}"
                            ),
                        );
                    }
                }
            }
        },
    };

    let protocol_version = discovery
        .protocol_version
        .clone()
        .unwrap_or_else(|| default_version(epoch).to_string());

    // Both paths converge here: the tool listing is the actual proof of life,
    // and it is where the cache directives come from.
    match list_tools(transport, &mut next_id, epoch, &protocol_version).await {
        Ok((tools, cache)) => {
            if let Some(key) = cache_key.as_deref() {
                remember_epoch(key, epoch);
            }
            let mut report = McpProbeReport::new(entry, McpProbeStatus::Healthy);
            report.epoch = Some(epoch);
            report.protocol_version = Some(protocol_version);
            report.tools = tools;
            report.instructions = discovery.instructions;
            if let Some(cache) = cache {
                report.cache_ttl_ms = Some(cache.ttl_ms);
                report.cache_private = cache.private;
            }
            report
        }
        Err(e) => unreachable(entry, cache_key.as_deref(), e.to_string()),
    }
}

fn default_version(epoch: McpSpecEpoch) -> &'static str {
    match epoch {
        McpSpecEpoch::Modern => MODERN_PROTOCOL_VERSION,
        McpSpecEpoch::Legacy => LEGACY_PROTOCOL_VERSION,
    }
}

fn authorization_required(entry: &McpServerEntry, challenge: Option<String>) -> McpProbeReport {
    let mut report = McpProbeReport::new(entry, McpProbeStatus::AuthorizationRequired);
    report.auth_challenge = challenge;
    report
}

fn unreachable(
    entry: &McpServerEntry,
    cache_key: Option<&str>,
    reason: impl Into<String>,
) -> McpProbeReport {
    // A failed probe invalidates whatever we thought we knew: next time starts
    // from scratch rather than compounding a stale verdict.
    if let Some(key) = cache_key {
        forget_epoch(key);
    }
    McpProbeReport::failed(entry, McpProbeStatus::Unreachable, reason.into())
}

/// What a modern attempt concluded.
enum EpochAttempt {
    /// The peer speaks the stateless revision.
    Modern(Discovery),
    /// The peer wants authorization first.
    Unauthorized { challenge: Option<String> },
    /// Nothing here identifies the peer as modern — fall back.
    NotModern(String),
}

/// Try `server/discover`, retrying once at a version the server offered.
async fn try_modern<T: ProbeTransport>(
    transport: &mut T,
    next_id: &mut u64,
) -> Result<EpochAttempt, String> {
    let mut version = MODERN_PROTOCOL_VERSION.to_string();
    // At most one retry: the server names the versions it accepts, so a second
    // rejection means we have nothing left to offer rather than a reason to
    // keep asking.
    for attempt in 0..2 {
        let id = take_id(next_id);
        let reply = transport
            .call(ProbeCall {
                id,
                method: rpc::METHOD_DISCOVER,
                params: rpc::modern_params(&version),
                protocol_version: &version,
            })
            .await;
        match reply {
            Ok(ProbeReply::Result(result)) => {
                return Ok(EpochAttempt::Modern(Discovery {
                    protocol_version: Some(version),
                    instructions: result
                        .get("instructions")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                }));
            }
            Ok(ProbeReply::Unauthorized { challenge }) => {
                return Ok(EpochAttempt::Unauthorized { challenge });
            }
            Ok(ProbeReply::Error(error)) => {
                if !error.proves_modern() {
                    return Ok(EpochAttempt::NotModern(describe_error(&error)));
                }
                // Modern for certain. Only a version disagreement is worth
                // retrying — any other spec error is a real refusal.
                if attempt == 0
                    && error.code == rpc::ERROR_UNSUPPORTED_PROTOCOL_VERSION
                    && let Some(offered) = error.supported_versions().into_iter().next()
                {
                    version = offered;
                    continue;
                }
                return Ok(EpochAttempt::Modern(Discovery {
                    protocol_version: Some(version),
                    instructions: None,
                }));
            }
            Ok(ProbeReply::Inconclusive(reason)) => return Ok(EpochAttempt::NotModern(reason)),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(EpochAttempt::NotModern(
        "the server rejected every protocol version it offered".to_string(),
    ))
}

/// Run the pre-stateless handshake.
async fn try_legacy<T: ProbeTransport>(
    transport: &mut T,
    next_id: &mut u64,
) -> Result<Discovery, String> {
    let id = take_id(next_id);
    let reply = transport
        .call(ProbeCall {
            id,
            method: rpc::METHOD_INITIALIZE,
            params: rpc::legacy_initialize_params(),
            protocol_version: LEGACY_PROTOCOL_VERSION,
        })
        .await
        .map_err(|e| e.to_string())?;
    let result = match reply {
        ProbeReply::Result(result) => result,
        ProbeReply::Error(error) => return Err(describe_error(&error)),
        ProbeReply::Unauthorized { .. } => {
            return Err("the server requires authorization".to_string());
        }
        ProbeReply::Inconclusive(reason) => return Err(reason),
    };
    transport
        .notify(rpc::METHOD_INITIALIZED, serde_json::json!({}))
        .await
        .map_err(|e| e.to_string())?;
    Ok(Discovery {
        protocol_version: result
            .get("protocolVersion")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        instructions: result
            .get("instructions")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    })
}

/// The `tools/list` both epochs finish on.
async fn list_tools<T: ProbeTransport>(
    transport: &mut T,
    next_id: &mut u64,
    epoch: McpSpecEpoch,
    protocol_version: &str,
) -> Result<(Vec<String>, Option<CacheHint>)> {
    let id = take_id(next_id);
    let params = match epoch {
        McpSpecEpoch::Modern => rpc::modern_params(protocol_version),
        McpSpecEpoch::Legacy => serde_json::json!({}),
    };
    let reply = transport
        .call(ProbeCall {
            id,
            method: rpc::METHOD_TOOLS_LIST,
            params,
            protocol_version,
        })
        .await?;
    match reply {
        ProbeReply::Result(result) => {
            Ok((rpc::tool_names(&result), CacheHint::from_result(&result)))
        }
        ProbeReply::Error(error) => anyhow::bail!("tools/list failed: {}", describe_error(&error)),
        ProbeReply::Unauthorized { .. } => {
            anyhow::bail!("the server requires authorization for tools/list")
        }
        ProbeReply::Inconclusive(reason) => anyhow::bail!("tools/list failed: {reason}"),
    }
}

fn take_id(next_id: &mut u64) -> u64 {
    let id = *next_id;
    *next_id += 1;
    id
}

fn describe_error(error: &JsonRpcError) -> String {
    if error.message.is_empty() {
        format!("JSON-RPC error {}", error.code)
    } else {
        format!("JSON-RPC error {} ({})", error.code, error.message)
    }
}

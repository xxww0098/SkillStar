//! JSON-RPC vocabulary shared by both MCP epochs: request framing, the modern
//! `_meta` envelope, and the error codes that identify a modern server.

use serde::Deserialize;
use serde_json::{Value, json};

/// Protocol version of the stateless ("modern") MCP revision.
///
/// This revision removed `initialize` / `notifications/initialized` and
/// `Mcp-Session-Id` entirely, and added `server/discover`.
pub const MODERN_PROTOCOL_VERSION: &str = "2026-07-28";

/// Newest pre-stateless revision — the version offered when falling back to
/// the `initialize` handshake.
pub const LEGACY_PROTOCOL_VERSION: &str = "2025-11-25";

/// Client identity reported in `_meta` / `initialize`. Self-asserted and
/// explicitly not a security signal per the spec; it exists so server logs can
/// tell who connected.
pub const CLIENT_NAME: &str = "SkillStar";

/// Lowest code in the range the MCP specification reserves for itself.
///
/// Codes in `-32020..=-32099` are spec-assigned, so receiving one *is* the
/// proof that the peer speaks the stateless revision: a legacy server has no
/// vocabulary for them and answers `-32601 Method not found` instead.
const SPEC_ERROR_MIN: i64 = -32099;
/// Highest code in the MCP-reserved range.
const SPEC_ERROR_MAX: i64 = -32020;

/// `UnsupportedProtocolVersion` — modern, but not at the version we asked for.
pub const ERROR_UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

/// JSON-RPC method that replaced the `initialize` handshake.
pub const METHOD_DISCOVER: &str = "server/discover";
/// Legacy handshake request.
pub const METHOD_INITIALIZE: &str = "initialize";
/// Legacy handshake notification.
pub const METHOD_INITIALIZED: &str = "notifications/initialized";
/// The tool listing both epochs finish on.
pub const METHOD_TOOLS_LIST: &str = "tools/list";

/// A JSON-RPC error object as returned by either epoch.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

impl JsonRpcError {
    /// Whether this error identifies the peer as a stateless-revision server.
    ///
    /// Only codes inside the MCP-reserved band count. A generic `-32600`
    /// (invalid request) or `-32601` (method not found) proves nothing: those
    /// are exactly what a legacy server returns for `server/discover`.
    pub fn proves_modern(&self) -> bool {
        (SPEC_ERROR_MIN..=SPEC_ERROR_MAX).contains(&self.code)
    }

    /// Protocol versions the server offered back when rejecting ours.
    ///
    /// The spec carries them in the error's `data`; both a bare array and a
    /// `{ "supported": [...] }` wrapper are accepted because the shape is not
    /// pinned by the schema.
    pub fn supported_versions(&self) -> Vec<String> {
        let strings = |value: &Value| -> Vec<String> {
            value
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        match &self.data {
            Some(data) => {
                let direct = strings(data);
                if !direct.is_empty() {
                    return direct;
                }
                data.get("supported")
                    .or_else(|| data.get("supportedVersions"))
                    .map(strings)
                    .unwrap_or_default()
            }
            None => Vec::new(),
        }
    }
}

/// The `_meta` block every stateless-revision request must carry.
///
/// `protocolVersion` and `clientCapabilities` are both required: the server is
/// forbidden from inferring capabilities from an earlier request, because
/// there is no longer any connection state to infer them from. On HTTP the
/// version here must equal the `MCP-Protocol-Version` header or the server
/// must answer `400` + `HeaderMismatch`.
pub fn modern_meta(protocol_version: &str) -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": protocol_version,
        "io.modelcontextprotocol/clientInfo": {
            "name": CLIENT_NAME,
            "version": env!("CARGO_PKG_VERSION"),
        },
        "io.modelcontextprotocol/clientCapabilities": {},
    })
}

/// Params for a stateless-revision request: the caller's own fields plus the
/// mandatory `_meta` envelope.
pub fn modern_params(protocol_version: &str) -> Value {
    json!({ "_meta": modern_meta(protocol_version) })
}

/// Params for the legacy `initialize` handshake.
pub fn legacy_initialize_params() -> Value {
    json!({
        "protocolVersion": LEGACY_PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": { "name": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION") },
    })
}

/// Serialize one JSON-RPC request envelope.
pub fn request_envelope(id: u64, method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// Serialize one JSON-RPC notification envelope (no `id`, no response).
pub fn notification_envelope(method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

/// The cache directives every `CacheableResult` carries in the stateless
/// revision (`tools/list`, `server/discover`, and the other list methods).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheHint {
    /// Freshness window in milliseconds; `0` means "expired on arrival".
    pub ttl_ms: u64,
    /// `true` when the result must not be shared across authorization
    /// contexts (`cacheScope: "private"`).
    pub private: bool,
}

impl CacheHint {
    /// Read the cache directives out of a result object, if present. Legacy
    /// servers carry none, so this is always optional.
    pub fn from_result(result: &Value) -> Option<Self> {
        let ttl_ms = result.get("ttlMs").and_then(Value::as_u64)?;
        let private = result.get("cacheScope").and_then(Value::as_str) == Some("private");
        Some(Self { ttl_ms, private })
    }
}

/// Extract tool names from a `tools/list` result.
pub fn tool_names(result: &Value) -> Vec<String> {
    result
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Compact JSON byte length of the `tools` array from a `tools/list` result.
///
/// This is the schema the model would ingest, minus `ttlMs` / `cacheScope`.
pub fn tools_schema_bytes(result: &Value) -> usize {
    result
        .get("tools")
        .map(|tools| tools.to_string().len())
        .unwrap_or(0)
}

/// Ceiling of `bytes / 4`. A cheap context-cost estimate, not a tokenizer.
pub fn schema_tokens(bytes: usize) -> u64 {
    (bytes as u64).div_ceil(4)
}

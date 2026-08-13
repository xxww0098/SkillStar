//! Epoch-detection tests driven through a scripted fake transport.
//!
//! Nothing here spawns a process or opens a socket: the whole point of the
//! [`ProbeTransport`] seam is that the branch-heavy part of the probe can be
//! checked deterministically and offline.

use super::*;
use serde_json::json;

/// A transport that replays a fixed script.
///
/// Each entry answers one `call`. The recorded methods and protocol versions
/// are what the tests assert on, since "which request did we send, in which
/// order" is the actual contract of the state machine.
struct FakeTransport {
    replies: std::collections::VecDeque<Result<ProbeReply, String>>,
    pub calls: Vec<(String, String)>,
    pub notifications: Vec<String>,
}

impl FakeTransport {
    fn new(replies: Vec<Result<ProbeReply, String>>) -> Self {
        Self {
            replies: replies.into(),
            calls: Vec::new(),
            notifications: Vec::new(),
        }
    }

    fn methods(&self) -> Vec<&str> {
        self.calls.iter().map(|(m, _)| m.as_str()).collect()
    }
}

impl ProbeTransport for FakeTransport {
    async fn call(&mut self, call: ProbeCall<'_>) -> Result<ProbeReply> {
        self.calls
            .push((call.method.to_string(), call.protocol_version.to_string()));
        match self.replies.pop_front() {
            Some(Ok(reply)) => Ok(reply),
            Some(Err(e)) => anyhow::bail!("{e}"),
            None => anyhow::bail!("fake transport ran out of scripted replies"),
        }
    }

    async fn notify(&mut self, method: &str, _params: serde_json::Value) -> Result<()> {
        self.notifications.push(method.to_string());
        Ok(())
    }
}

fn ok(value: serde_json::Value) -> Result<ProbeReply, String> {
    Ok(ProbeReply::Result(value))
}

fn err(code: i64, message: &str) -> Result<ProbeReply, String> {
    Ok(ProbeReply::Error(JsonRpcError {
        code,
        message: message.to_string(),
        data: None,
    }))
}

fn tools_list(names: &[&str]) -> serde_json::Value {
    json!({
        "tools": names.iter().map(|n| json!({ "name": n })).collect::<Vec<_>>(),
        "ttlMs": 60_000,
        "cacheScope": "public",
    })
}

/// An entry with no cacheable identity, so tests never share epoch state.
fn anonymous_entry(transport: &str) -> McpServerEntry {
    let mut entry = blank_entry("probe-fixture", transport);
    entry.id = "fixture-id".to_string();
    entry
}

async fn probe_with(
    entry: &McpServerEntry,
    replies: Vec<Result<ProbeReply, String>>,
) -> (McpProbeReport, FakeTransport) {
    let mut transport = FakeTransport::new(replies);
    let report = run_probe(entry, &mut transport).await;
    (report, transport)
}

// ---------------------------------------------------------------------------
// Modern path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn discover_result_selects_the_modern_epoch() {
    let entry = anonymous_entry("stdio");
    let (report, transport) = probe_with(
        &entry,
        vec![
            ok(json!({ "supportedVersions": [MODERN_PROTOCOL_VERSION], "instructions": "read me" })),
            ok(tools_list(&["search", "fetch"])),
        ],
    )
    .await;

    assert_eq!(report.status, McpProbeStatus::Healthy);
    assert_eq!(report.epoch, Some(McpSpecEpoch::Modern));
    assert_eq!(
        report.protocol_version.as_deref(),
        Some(MODERN_PROTOCOL_VERSION)
    );
    assert_eq!(report.tools, vec!["search", "fetch"]);
    assert_eq!(report.instructions.as_deref(), Some("read me"));
    assert_eq!(report.cache_ttl_ms, Some(60_000));
    assert!(!report.cache_private);
    assert_eq!(transport.methods(), vec!["server/discover", "tools/list"]);
    // No handshake exists in this revision — sending one would be a protocol error.
    assert!(transport.notifications.is_empty());
}

#[tokio::test]
async fn unsupported_protocol_version_stays_modern_and_retries() {
    let entry = anonymous_entry("stdio");
    let version_error = ProbeReply::Error(JsonRpcError {
        code: -32022,
        message: "unsupported".to_string(),
        data: Some(json!({ "supported": ["2027-01-01"] })),
    });
    let (report, transport) = probe_with(
        &entry,
        vec![Ok(version_error), ok(json!({})), ok(tools_list(&["only"]))],
    )
    .await;

    assert_eq!(report.epoch, Some(McpSpecEpoch::Modern));
    assert_eq!(report.protocol_version.as_deref(), Some("2027-01-01"));
    // Two discovers: the rejected one, then the retry at the offered version.
    assert_eq!(
        transport.methods(),
        vec!["server/discover", "server/discover", "tools/list"]
    );
    assert_eq!(transport.calls[1].1, "2027-01-01");
}

#[tokio::test]
async fn other_spec_reserved_errors_prove_modern_without_falling_back() {
    let entry = anonymous_entry("http");
    // -32021 MissingRequiredClientCapability: still MCP-reserved vocabulary,
    // so the peer is modern even though this particular call failed.
    let (report, transport) = probe_with(
        &entry,
        vec![err(-32021, "need caps"), ok(tools_list(&["t"]))],
    )
    .await;

    assert_eq!(report.epoch, Some(McpSpecEpoch::Modern));
    assert_eq!(transport.methods(), vec!["server/discover", "tools/list"]);
}

// ---------------------------------------------------------------------------
// Legacy fallback
// ---------------------------------------------------------------------------

#[tokio::test]
async fn method_not_found_falls_back_to_the_legacy_handshake() {
    let entry = anonymous_entry("stdio");
    let (report, transport) = probe_with(
        &entry,
        vec![
            // What a pre-stateless server says to `server/discover`.
            err(-32601, "Method not found"),
            ok(json!({ "protocolVersion": LEGACY_PROTOCOL_VERSION, "capabilities": {} })),
            ok(tools_list(&["legacy-tool"])),
        ],
    )
    .await;

    assert_eq!(report.status, McpProbeStatus::Healthy);
    assert_eq!(report.epoch, Some(McpSpecEpoch::Legacy));
    assert_eq!(report.tools, vec!["legacy-tool"]);
    assert_eq!(
        transport.methods(),
        vec!["server/discover", "initialize", "tools/list"]
    );
    // The legacy handshake is only complete once `initialized` is sent.
    assert_eq!(transport.notifications, vec!["notifications/initialized"]);
}

#[tokio::test]
async fn an_inconclusive_http_response_falls_back_rather_than_guessing() {
    let entry = anonymous_entry("http");
    let (report, transport) = probe_with(
        &entry,
        vec![
            // A 400 whose body carried nothing recognizable.
            Ok(ProbeReply::Inconclusive(
                "HTTP 400 from example".to_string(),
            )),
            ok(json!({ "protocolVersion": LEGACY_PROTOCOL_VERSION })),
            ok(tools_list(&["t"])),
        ],
    )
    .await;

    assert_eq!(report.epoch, Some(McpSpecEpoch::Legacy));
    assert_eq!(
        transport.methods(),
        vec!["server/discover", "initialize", "tools/list"]
    );
}

#[tokio::test]
async fn a_transport_error_on_discover_still_tries_legacy() {
    let entry = anonymous_entry("stdio");
    let (report, _) = probe_with(
        &entry,
        vec![
            Err("timed out".to_string()),
            ok(json!({})),
            ok(tools_list(&["t"])),
        ],
    )
    .await;

    assert_eq!(report.status, McpProbeStatus::Healthy);
    assert_eq!(report.epoch, Some(McpSpecEpoch::Legacy));
}

// ---------------------------------------------------------------------------
// Authorization + failure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_401_challenge_is_its_own_status_not_a_failure() {
    let entry = anonymous_entry("http");
    let (report, transport) = probe_with(
        &entry,
        vec![Ok(ProbeReply::Unauthorized {
            challenge: Some("Bearer resource_metadata=\"https://example.com/.well-known\"".into()),
        })],
    )
    .await;

    assert_eq!(report.status, McpProbeStatus::AuthorizationRequired);
    assert!(report.error.is_none(), "{:?}", report.error);
    assert!(report.auth_challenge.unwrap().starts_with("Bearer"));
    // No point continuing to `tools/list` before authorizing.
    assert_eq!(transport.methods(), vec!["server/discover"]);
}

#[tokio::test]
async fn neither_epoch_answering_reports_unreachable_with_both_reasons() {
    let entry = anonymous_entry("stdio");
    let (report, _) = probe_with(
        &entry,
        vec![err(-32601, "no discover"), Err("broken pipe".to_string())],
    )
    .await;

    assert_eq!(report.status, McpProbeStatus::Unreachable);
    assert_eq!(report.epoch, None);
    let error = report.error.unwrap();
    assert!(error.contains("modern"), "{error}");
    assert!(error.contains("broken pipe"), "{error}");
}

#[tokio::test]
async fn a_failing_tools_list_is_unreachable_even_after_a_good_handshake() {
    let entry = anonymous_entry("stdio");
    let (report, _) = probe_with(&entry, vec![ok(json!({})), err(-32603, "internal error")]).await;

    assert_eq!(report.status, McpProbeStatus::Unreachable);
    assert!(report.error.unwrap().contains("tools/list"));
}

#[tokio::test]
async fn a_private_cache_scope_is_carried_through() {
    let entry = anonymous_entry("http");
    let (report, _) = probe_with(
        &entry,
        vec![
            ok(json!({})),
            ok(json!({ "tools": [], "ttlMs": 0, "cacheScope": "private" })),
        ],
    )
    .await;

    assert!(report.cache_private);
    assert_eq!(report.cache_ttl_ms, Some(0));
}

// ---------------------------------------------------------------------------
// Epoch cache
// ---------------------------------------------------------------------------

/// A remote entry on its own origin, with any remembered epoch evicted.
///
/// Each cache test owns a distinct origin and evicts only its own key. A
/// blanket `clear_mcp_epoch_cache()` would be a shared mutable global these
/// tests race each other on when run in parallel.
fn remote_entry(url: &str) -> McpServerEntry {
    let mut entry = blank_entry("cached-remote", "http");
    entry.id = "cached-remote-id".to_string();
    entry.url = Some(url.to_string());
    forget_epoch(&epoch_cache_key(&entry).unwrap());
    entry
}

#[tokio::test]
async fn a_remembered_legacy_epoch_skips_the_modern_attempt() {
    let entry = remote_entry("https://cache-test.example.com/mcp");

    let (first, first_transport) = probe_with(
        &entry,
        vec![
            err(-32601, "no discover"),
            ok(json!({})),
            ok(tools_list(&["t"])),
        ],
    )
    .await;
    assert_eq!(first.epoch, Some(McpSpecEpoch::Legacy));
    assert_eq!(first_transport.methods()[0], "server/discover");

    // Second probe of the same origin starts at `initialize`.
    let (second, second_transport) =
        probe_with(&entry, vec![ok(json!({})), ok(tools_list(&["t"]))]).await;
    assert_eq!(second.epoch, Some(McpSpecEpoch::Legacy));
    assert_eq!(second_transport.methods(), vec!["initialize", "tools/list"]);
}

#[tokio::test]
async fn a_failed_probe_forgets_the_cached_epoch() {
    let entry = remote_entry("https://evict-test.example.com/mcp");

    let (healthy, _) = probe_with(&entry, vec![ok(json!({})), ok(tools_list(&["t"]))]).await;
    assert_eq!(healthy.epoch, Some(McpSpecEpoch::Modern));
    assert_eq!(
        cached_epoch(&epoch_cache_key(&entry).unwrap()),
        Some(McpSpecEpoch::Modern)
    );

    let (failed, _) = probe_with(
        &entry,
        vec![Err("gone".to_string()), Err("gone".to_string())],
    )
    .await;
    assert_eq!(failed.status, McpProbeStatus::Unreachable);
    assert_eq!(cached_epoch(&epoch_cache_key(&entry).unwrap()), None);
}

#[tokio::test]
async fn a_stale_cached_epoch_re_probes_the_other_revision() {
    let entry = remote_entry("https://upgrade-test.example.com/mcp");
    remember_epoch(&epoch_cache_key(&entry).unwrap(), McpSpecEpoch::Legacy);

    // The server was upgraded: `initialize` no longer exists.
    let (report, transport) = probe_with(
        &entry,
        vec![
            err(-32601, "initialize is gone"),
            ok(json!({ "supportedVersions": [MODERN_PROTOCOL_VERSION] })),
            ok(tools_list(&["t"])),
        ],
    )
    .await;

    assert_eq!(report.epoch, Some(McpSpecEpoch::Modern));
    assert_eq!(
        transport.methods(),
        vec!["initialize", "server/discover", "tools/list"]
    );
    assert_eq!(
        cached_epoch(&epoch_cache_key(&entry).unwrap()),
        Some(McpSpecEpoch::Modern)
    );
}

// ---------------------------------------------------------------------------
// Cache keys + runtime check
// ---------------------------------------------------------------------------

#[test]
fn remote_entries_share_one_cache_key_per_origin() {
    let mut a = blank_entry("a", "http");
    a.url = Some("https://api.example.com/mcp".into());
    let mut b = blank_entry("b", "sse");
    b.url = Some("https://api.example.com/other/path".into());
    assert_eq!(epoch_cache_key(&a), epoch_cache_key(&b));

    let mut other = blank_entry("c", "http");
    other.url = Some("https://elsewhere.example.com/mcp".into());
    assert_ne!(epoch_cache_key(&a), epoch_cache_key(&other));
}

#[test]
fn stdio_entries_key_on_the_launched_process() {
    let mut a = blank_entry("a", "stdio");
    a.command = Some("npx".into());
    a.args = vec!["-y".into(), "server-x".into()];
    let mut same = blank_entry("renamed", "stdio");
    same.command = Some("npx".into());
    same.args = vec!["-y".into(), "server-x".into()];
    assert_eq!(epoch_cache_key(&a), epoch_cache_key(&same));

    let mut different = blank_entry("b", "stdio");
    different.command = Some("npx".into());
    different.args = vec!["-y".into(), "server-y".into()];
    assert_ne!(epoch_cache_key(&a), epoch_cache_key(&different));
}

#[test]
fn a_missing_stdio_runtime_names_the_command() {
    let mut entry = blank_entry("needs-runtime", "stdio");
    entry.command = Some("skillstar-definitely-not-a-real-binary".into());
    let err = check_stdio_runtime(&entry).unwrap_err().to_string();
    assert!(
        err.contains("skillstar-definitely-not-a-real-binary"),
        "{err}"
    );
    assert!(err.contains("not found"), "{err}");
}

#[test]
fn remote_entries_need_no_local_runtime() {
    let mut entry = blank_entry("remote", "http");
    entry.url = Some("https://example.com/mcp".into());
    assert!(check_stdio_runtime(&entry).is_ok());
}

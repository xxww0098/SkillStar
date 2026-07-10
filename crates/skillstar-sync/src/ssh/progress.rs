//! Progress reporting abstraction for SSH connection steps.
//!
//! The crate stays Tauri-agnostic: connection functions take an
//! `&impl ProgressSink` and call `sink.emit(...)` at each phase. The Tauri
//! command layer injects a sink that forwards to `window.emit("ssh://connect-stream")`.
//!
//! `NoopSink` is the test/default sink (no-ops). The frontend filters events by
//! `session_id` so concurrent host operations don't interleave.

use serde::{Deserialize, Serialize};

/// A single connection-progress event for the UI console.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshProgressEvent {
    /// Unique id for this connection attempt — the frontend filters by it.
    pub session_id: String,
    /// `dial` | `handshake` | `host_key` | `auth` | `sftp` | `scan` | `done` | `error`
    pub phase: Phase,
    /// `start` | `ok` | `warn` | `fail` | `pending`
    pub status: Status,
    /// Human-readable line shown in the console.
    pub message: String,
    /// Epoch milliseconds (best-effort) for timestamping.
    pub ts_ms: u64,
    /// Structured payload for special phases (e.g. host_key fingerprint).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Dial,
    Handshake,
    HostKey,
    Auth,
    Sftp,
    /// Remote skill discovery / listing over an open SFTP session.
    Scan,
    Done,
    Error,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Start,
    Ok,
    Warn,
    Fail,
    Pending,
}

/// Receives progress events. Implemented by the Tauri layer (forwards to
/// `window.emit`); tests use [`NoopSink`].
pub trait ProgressSink: Send + Sync {
    fn emit(&self, event: SshProgressEvent);
}

/// A sink that discards everything. The default for tests / callers that
/// don't care about progress.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSink;

impl ProgressSink for NoopSink {
    fn emit(&self, _event: SshProgressEvent) {}
}

/// Helper to build an event with a fresh timestamp.
#[allow(dead_code)]
pub fn event(session_id: &str, phase: Phase, status: Status, message: impl Into<String>) -> SshProgressEvent {
    SshProgressEvent {
        session_id: session_id.to_string(),
        phase,
        status,
        message: message.into(),
        ts_ms: now_ms(),
        detail: None,
    }
}

/// Helper for an event carrying structured detail (e.g. a host-key fingerprint).
#[allow(dead_code)]
pub fn event_with_detail(
    session_id: &str,
    phase: Phase,
    status: Status,
    message: impl Into<String>,
    detail: serde_json::Value,
) -> SshProgressEvent {
    SshProgressEvent {
        session_id: session_id.to_string(),
        phase,
        status,
        message: message.into(),
        ts_ms: now_ms(),
        detail: Some(detail),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct CollectSink {
        events: Arc<Mutex<Vec<SshProgressEvent>>>,
    }

    impl ProgressSink for CollectSink {
        fn emit(&self, event: SshProgressEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    #[test]
    fn noop_sink_swallows_events() {
        NoopSink.emit(event("s1", Phase::Dial, Status::Start, "x"));
        // no panic, no output — that's the contract
    }

    #[test]
    fn collect_sink_records_order() {
        let sink = CollectSink::default();
        sink.emit(event("s1", Phase::Dial, Status::Start, "dialing"));
        sink.emit(event("s1", Phase::Dial, Status::Ok, "connected"));
        let evts = sink.events.lock().unwrap();
        assert_eq!(evts.len(), 2);
        assert_eq!(evts[0].status, Status::Start);
        assert_eq!(evts[1].status, Status::Ok);
    }

    #[test]
    fn scan_phase_serializes_as_snake_case() {
        let e = event("s1", Phase::Scan, Status::Start, "scanning remote for skills…");
        assert_eq!(e.phase, Phase::Scan);
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"phase\":\"scan\""));
    }

    #[test]
    fn event_with_detail_carries_payload() {
        let e = event_with_detail(
            "s1",
            Phase::HostKey,
            Status::Pending,
            "verify",
            serde_json::json!({ "fingerprint": "SHA256:abc" }),
        );
        assert_eq!(e.detail.unwrap()["fingerprint"], "SHA256:abc");
    }
}

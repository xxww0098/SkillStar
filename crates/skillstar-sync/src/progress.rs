//! Progress reporting abstraction for S3 sync operations.
//!
//! Forked from `skillstar_ssh::progress` with sync-specific phases. The crate
//! stays Tauri-agnostic: sync functions take an `&impl ProgressSink` and call
//! `sink.emit(...)` at each phase. The Tauri command layer injects a sink that
//! forwards to `window.emit("s3://sync-stream")`.

use serde::{Deserialize, Serialize};

/// A single sync-progress event for the UI console.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3ProgressEvent {
    /// Unique id for this sync attempt — the frontend filters by it.
    pub session_id: String,
    /// `resolve` | `list_local` | `pack` | `upload` | `upload_manifest` |
    /// `download` | `unpack` | `scan` | `done` | `error`
    pub phase: Phase,
    /// `start` | `ok` | `warn` | `fail` | `pending`
    pub status: Status,
    /// Human-readable line shown in the console.
    pub message: String,
    /// Epoch milliseconds (best-effort) for timestamping.
    pub ts_ms: u64,
    /// Structured payload (e.g. per-skill upload counts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Resolving target + building S3 client.
    Resolve,
    /// Enumerating local installed skills.
    ListLocal,
    /// Packing a local skill into a tarball.
    Pack,
    /// Uploading a tarball to the bucket.
    Upload,
    /// Uploading / replacing the manifest.
    UploadManifest,
    /// Downloading a tarball.
    Download,
    /// Unpacking a downloaded tarball.
    Unpack,
    /// Reading the remote manifest.
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
    fn emit(&self, event: S3ProgressEvent);
}

/// A sink that discards everything. The default for tests / callers that
/// don't care about progress.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSink;

impl ProgressSink for NoopSink {
    fn emit(&self, _event: S3ProgressEvent) {}
}

/// Helper to build an event with a fresh timestamp.
pub fn event(session_id: &str, phase: Phase, status: Status, message: impl Into<String>) -> S3ProgressEvent {
    S3ProgressEvent {
        session_id: session_id.to_string(),
        phase,
        status,
        message: message.into(),
        ts_ms: now_ms(),
        detail: None,
    }
}

/// Helper for an event carrying structured detail.
pub fn event_with_detail(
    session_id: &str,
    phase: Phase,
    status: Status,
    message: impl Into<String>,
    detail: serde_json::Value,
) -> S3ProgressEvent {
    S3ProgressEvent {
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

    #[test]
    fn event_serialises_snake_case_phases() {
        let e = event("s1", Phase::UploadManifest, Status::Ok, "done");
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"phase\":\"upload_manifest\""));
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"sessionId\":\"s1\""));
    }

    #[test]
    fn noop_swallows() {
        NoopSink.emit(event("s1", Phase::Scan, Status::Start, "x"));
    }
}

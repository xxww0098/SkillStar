//! Tauri commands for S3 cloud sync.
//!
//! Thin forwarder layer over the `skillstar-sync` crate, mirroring
//! `commands/ssh_hosts.rs`: all logic lives in the crate, the commands only
//! translate errors to [`AppError`] and pass types across IPC. Progress events
//! stream to the frontend via the `s3://sync-stream` channel.
//!
//! Credential handling: `secret_access_key` is never returned to the frontend.
//! `add_s3_target` / `update_s3_target` accept an optional `secret_access_key`
//! string that is written to the system keyring and then dropped.

use skillstar_core::infra::error::AppError;
use skillstar_sync::progress::{Phase, ProgressSink, S3ProgressEvent, event};
use skillstar_sync::store::KeyringSecretStore;
use skillstar_sync::{TargetsStore, pull_manifest, push_all, restore_entries};
use tauri::{AppHandle, Emitter};

/// Re-exported DTOs so command signatures stay terse.
pub use skillstar_sync::{
    ConnectionTestResult, InstallSummary, ManifestEntry, ManifestEntryView, PushSummary,
    S3TargetDef,
};

/// The Tauri event channel the frontend listens on for sync progress.
const SYNC_STREAM_CHANNEL: &str = "s3://sync-stream";

/// A [`ProgressSink`] that forwards each event to the Tauri frontend via
/// `window.emit("s3://sync-stream", event)`.
#[derive(Clone)]
struct TauriProgressSink {
    app: AppHandle,
}

impl ProgressSink for TauriProgressSink {
    fn emit(&self, event: S3ProgressEvent) {
        let _ = self.app.emit(SYNC_STREAM_CHANNEL, event);
    }
}

/// A fresh, unique-ish session id per command invocation so the frontend can
/// filter events to the in-flight operation.
fn new_session_id() -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("sync_{now_ms}")
}

/// Map an `anyhow::Error` into an [`AppError::Sync`].
fn to_sync_err(err: anyhow::Error) -> AppError {
    AppError::Sync(err.to_string())
}

// ── Target CRUD ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_s3_targets() -> Result<Vec<S3TargetDef>, AppError> {
    Ok(skillstar_sync::load_targets())
}

#[tauri::command]
pub async fn add_s3_target(
    def: S3TargetDef,
    secret_access_key: Option<String>,
) -> Result<S3TargetDef, AppError> {
    let store = TargetsStore::new(KeyringSecretStore);
    store
        .add(def, secret_access_key.as_deref())
        .map_err(to_sync_err)
}

#[tauri::command]
pub async fn update_s3_target(
    id: String,
    def: S3TargetDef,
    secret_access_key: Option<String>,
) -> Result<(), AppError> {
    let store = TargetsStore::new(KeyringSecretStore);
    store
        .update(&id, def, secret_access_key.as_deref())
        .map_err(to_sync_err)
}

#[tauri::command]
pub async fn delete_s3_target(id: String) -> Result<(), AppError> {
    let store = TargetsStore::new(KeyringSecretStore);
    store.remove(&id).map_err(to_sync_err)
}

#[tauri::command]
pub async fn test_s3_connection(def: S3TargetDef) -> Result<ConnectionTestResult, AppError> {
    let secrets = KeyringSecretStore;
    // test_connection needs an AppHandle-free path in the crate; the Tauri sink
    // is wired only for the streaming sync commands below, so probe quietly.
    skillstar_sync::test_connection_quiet(&def, &secrets)
        .await
        .map_err(to_sync_err)
}

// ── Sync operations ─────────────────────────────────────────────────

#[tauri::command]
pub async fn push_skills_to_cloud(
    target_id: String,
    app: AppHandle,
) -> Result<PushSummary, AppError> {
    let secrets = KeyringSecretStore;
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    push_all(&target_id, &secrets, &session_id, &sink)
        .await
        .map_err(|e| {
            sink.emit(event(&session_id, Phase::Error, skillstar_sync::Status::Fail, e.to_string()));
            to_sync_err(e)
        })
}

#[tauri::command]
pub async fn pull_cloud_manifest(
    target_id: String,
    app: AppHandle,
) -> Result<Vec<ManifestEntryView>, AppError> {
    let secrets = KeyringSecretStore;
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    pull_manifest(&target_id, &secrets, &session_id, &sink)
        .await
        .map_err(|e| {
            sink.emit(event(&session_id, Phase::Error, skillstar_sync::Status::Fail, e.to_string()));
            to_sync_err(e)
        })
}

#[tauri::command]
pub async fn install_from_cloud_manifest(
    target_id: String,
    entries: Vec<ManifestEntry>,
    app: AppHandle,
) -> Result<InstallSummary, AppError> {
    let secrets = KeyringSecretStore;
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    restore_entries(&target_id, &secrets, entries, &session_id, &sink)
        .await
        .map_err(|e| {
            sink.emit(event(&session_id, Phase::Error, skillstar_sync::Status::Fail, e.to_string()));
            to_sync_err(e)
        })
}

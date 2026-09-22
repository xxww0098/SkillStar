//! Local decision-model commands.
//!
//! Adapter only: DTOs, shared state, progress events and scheduling. Every
//! piece of real work — the checkpoint layout, the download, the tokenizer and
//! the forward pass — lives in `skillstar-decision`. This module builds no
//! HTTP client of its own either: the crate hands it a download entry point
//! that already carries the user's proxy settings, which is what the
//! command-layer boundary check exists to enforce.
//!
//! The engine is 1.2 GB of weights and a Metal/CPU context, so it is loaded
//! lazily on first use, kept in shared state, and run on a blocking thread:
//! a forward pass is CPU/GPU-bound and would otherwise stall the async
//! runtime's worker.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use skillstar_core::infra::error::AppError;
use skillstar_decision::{
    DownloadProgress, DecisionEngine, DecisionOutcome, EngineInfo, EngineOptions, ModelPaths,
    ModelStatus,
};
use tauri::{Emitter, State, Window};

/// Event channel for download progress.
const DOWNLOAD_PROGRESS_EVENT: &str = "decision://download-progress";

/// Shared decision-model state.
#[derive(Default)]
pub struct DecisionState {
    engine: tokio::sync::Mutex<Option<Arc<DecisionEngine>>>,
    cancel_download: AtomicBool,
}

impl DecisionState {
    /// Fresh state: nothing loaded, no download in flight.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop the loaded engine, releasing its weights and device buffers.
    pub async fn unload(&self) {
        let mut guard = self.engine.lock().await;
        *guard = None;
    }
}

/// Progress payload as it reaches the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgressEvent {
    downloaded: u64,
    total: u64,
    file: String,
}

impl From<DownloadProgress> for DownloadProgressEvent {
    fn from(progress: DownloadProgress) -> Self {
        Self {
            downloaded: progress.downloaded,
            total: progress.total,
            file: progress.file,
        }
    }
}

/// Where the checkpoint lives and whether it is complete.
#[tauri::command]
pub async fn decision_model_status() -> Result<ModelStatus, AppError> {
    Ok(ModelPaths::resolve().status())
}

/// Re-check every checkpoint digest. Slow by design: it reads 1.2 GB.
#[tauri::command]
pub async fn decision_verify_model() -> Result<(), AppError> {
    let paths = ModelPaths::resolve();
    tokio::task::spawn_blocking(move || paths.verify())
        .await?
        .map_err(|error| AppError::Other(error.to_string()))
}

/// Download the checkpoint, emitting [`DOWNLOAD_PROGRESS_EVENT`] as it goes.
#[tauri::command]
pub async fn decision_download_model(
    window: Window,
    state: State<'_, DecisionState>,
) -> Result<(), AppError> {
    let paths = ModelPaths::resolve();
    state.cancel_download.store(false, Ordering::SeqCst);
    let cancel = &state.cancel_download;

    skillstar_decision::download_with_shared_client(&paths, cancel, |progress| {
        let payload = DownloadProgressEvent::from(progress);
        // A failed emit must not abort a 1.2 GB transfer: the UI would rather
        // lose a progress tick than the download.
        let _ = window.emit(DOWNLOAD_PROGRESS_EVENT, payload);
    })
    .await
    .map_err(|error| AppError::Other(error.to_string()))
}

/// Ask an in-flight download to stop; the partial file is kept for resuming.
#[tauri::command]
pub async fn decision_cancel_download(state: State<'_, DecisionState>) -> Result<(), AppError> {
    state.cancel_download.store(true, Ordering::SeqCst);
    Ok(())
}

/// What the currently loaded engine runs as, or `null` when nothing is loaded.
#[tauri::command]
pub async fn decision_engine_info(state: State<'_, DecisionState>) -> Result<Option<EngineInfo>, AppError> {
    let guard = state.engine.lock().await;
    Ok(guard.as_ref().map(|engine| engine.info()))
}

/// Load the engine now instead of on the first question.
#[tauri::command]
pub async fn decision_load_engine(state: State<'_, DecisionState>) -> Result<EngineInfo, AppError> {
    Ok(load_engine(&state).await?.info())
}

/// Release the engine and its memory.
#[tauri::command]
pub async fn decision_unload_engine(state: State<'_, DecisionState>) -> Result<(), AppError> {
    state.unload().await;
    Ok(())
}

/// Answer a `agentjev.decision.v1` payload.
#[tauri::command]
pub async fn decision_evaluate(
    payload: serde_json::Value,
    state: State<'_, DecisionState>,
) -> Result<DecisionOutcome, AppError> {
    let engine = load_engine(&state).await?;
    tokio::task::spawn_blocking(move || engine.evaluate(&payload))
        .await?
        .map_err(|error| AppError::Other(error.to_string()))
}

/// Return the shared engine, loading it on first use.
///
/// The lock is released before the (multi-second) load so a second caller
/// waits on the load instead of deadlocking behind the first.
async fn load_engine(state: &State<'_, DecisionState>) -> Result<Arc<DecisionEngine>, AppError> {
    if let Some(engine) = state.engine.lock().await.clone() {
        return Ok(engine);
    }
    let paths = ModelPaths::resolve();
    let loaded = tokio::task::spawn_blocking(move || {
        DecisionEngine::load(&paths, EngineOptions::default())
    })
    .await?
    .map_err(|error| AppError::Other(error.to_string()))?;

    let loaded = Arc::new(loaded);
    let mut guard = state.engine.lock().await;
    // Another caller may have won the race; keep whichever landed first so one
    // process never holds two copies of the checkpoint.
    Ok(guard.get_or_insert_with(|| loaded.clone()).clone())
}

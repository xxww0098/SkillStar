//! App update commands with GitHub mirror support.
//!
//! The Tauri updater plugin's JS `check()` always uses the static endpoints
//! from `tauri.conf.json`.  When the user enables GitHub mirror acceleration,
//! those endpoints (which point to `github.com`) are unreachable.
//!
//! This module provides Rust-side commands that:
//! 1. Read the mirror config at runtime
//! 2. Rewrite the update endpoint URL through the active mirror
//! 3. Use `UpdaterExt::updater_builder()` to build a one-off updater
//! 4. Store the resulting `Update` object in app state for download/install

use std::sync::Mutex;

use serde::Serialize;
use tauri::Manager;
use tracing::{info, warn};

use crate::core::github_mirror;

/// The original (non-mirrored) update endpoint from tauri.conf.json.
const UPSTREAM_ENDPOINT: &str =
    "https://github.com/xxww0098/SkillStar/releases/latest/download/latest.json";

// ── State ──────────────────────────────────────────────────────────────

/// Holds the pending `Update` object between check → download → install steps.
pub struct PendingUpdate {
    inner: Mutex<Option<tauri_plugin_updater::Update>>,
}

impl PendingUpdate {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

// ── Response types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheckResult {
    pub available: bool,
    pub version: Option<String>,
    pub date: Option<String>,
    pub body: Option<String>,
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Build the effective endpoint list.
///
/// If mirror is enabled, prepend the mirror-rewritten URL so it's tried first,
/// then keep the direct GitHub URL as fallback.
fn effective_endpoints() -> Vec<url::Url> {
    let direct = url::Url::parse(UPSTREAM_ENDPOINT).expect("hardcoded URL is valid");

    if let Some(mirror_base) = github_mirror::effective_mirror_url() {
        // Mirror proxy pattern: `https://mirror.example/https://github.com/…`
        let mirrored_url_str = format!("{mirror_base}{UPSTREAM_ENDPOINT}");
        if let Ok(mirrored) = url::Url::parse(&mirrored_url_str) {
            info!(target: "updater", "using mirror endpoint: {mirrored}");
            return vec![mirrored, direct];
        }
        warn!(target: "updater", "failed to parse mirror URL, falling back to direct");
    }

    vec![direct]
}

// ── Commands ───────────────────────────────────────────────────────────

/// Check for an app update, using mirror-aware endpoints.
///
/// Returns update metadata.  The `Update` object is stored in app state so
/// `download_app_update` / `install_app_update` can use it later.
#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle) -> Result<UpdateCheckResult, String> {
    use tauri_plugin_updater::UpdaterExt;

    let endpoints = effective_endpoints();
    info!(target: "updater", "checking for update with {} endpoint(s)", endpoints.len());

    let updater = app
        .updater_builder()
        .endpoints(endpoints)
        .map_err(|e| format!("failed to set endpoints: {e}"))?
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to build updater: {e}"))?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("update check failed: {e}"))?;

    match update {
        Some(update) => {
            let result = UpdateCheckResult {
                available: true,
                version: Some(update.version.clone()),
                date: update.date.map(|d| d.to_string()),
                body: update.body.clone(),
            };

            // Store for later download/install
            let pending = app.state::<PendingUpdate>();
            if let Ok(mut slot) = pending.inner.lock() {
                *slot = Some(update);
            }

            info!(target: "updater", "update available: v{}", result.version.as_deref().unwrap_or("?"));
            Ok(result)
        }
        None => {
            info!(target: "updater", "already up to date");
            Ok(UpdateCheckResult {
                available: false,
                version: None,
                date: None,
                body: None,
            })
        }
    }
}

/// Download and install the pending update.
///
/// Emits `updater://download-progress` events with `{ chunk_length, content_length }`.
/// After download completes, installs the update automatically.
#[tauri::command]
pub async fn download_and_install_update(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Emitter;

    // Take the update out of the mutex — we own it for the remainder of this call.
    let update = {
        let pending = app.state::<PendingUpdate>();
        let mut slot = pending
            .inner
            .lock()
            .map_err(|e| format!("lock error: {e}"))?;
        slot.take().ok_or("no pending update to download")?
    };

    let app_for_events = app.clone();

    update
        .download_and_install(
            move |chunk_length, content_length| {
                let _ = app_for_events.emit(
                    "updater://download-progress",
                    serde_json::json!({
                        "chunk_length": chunk_length,
                        "content_length": content_length,
                    }),
                );
            },
            || {},
        )
        .await
        .map_err(|e| format!("download_and_install failed: {e}"))?;

    info!(target: "updater", "update downloaded and installed, ready for restart");
    Ok(())
}

/// Restart the app to apply the installed update.
#[tauri::command]
pub async fn restart_after_update(app: tauri::AppHandle) -> Result<(), String> {
    app.restart();
}

//! App update detection via `tauri-plugin-updater` (signed install path).
//!
//! Active path: query `plugins.updater.endpoints` (`latest.json` on GitHub
//! Releases), store a pending [`tauri_plugin_updater::Update`], and let the
//! UI download/install/restart through this module.
//!
//! Signing: private key lives only in GitHub Actions secrets
//! (`TAURI_SIGNING_PRIVATE_KEY` + password) and a local backup under
//! `~/.tauri/skillstar.key` — never in the repo. Public key is in
//! `tauri.conf.json`. See `docs/features/platform/README.md` § Updater 与发布.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use skillstar_core::infra::error::AppError;
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;
use tracing::info;

/// Fallback page when the UI wants a human-readable release link.
const RELEASES_PAGE: &str = "https://github.com/xxww0098/SkillStar/releases/latest";
const LATEST_JSON_URL: &str =
    "https://github.com/xxww0098/SkillStar/releases/latest/download/latest.json";

// ── State ──────────────────────────────────────────────────────────────

/// Holds the pending `Update` object between check → download → install.
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
    /// GitHub release page (informational; install goes through the plugin).
    pub release_url: Option<String>,
}

// ── Commands ───────────────────────────────────────────────────────────

/// Check configured updater endpoints for a newer signed release.
///
/// On success with an update, stores it in [`PendingUpdate`] for
/// [`download_and_install_update`]. Network / endpoint failures return `Err`
/// so the UI can surface an honest error (not a silent “up to date”).
#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle) -> Result<UpdateCheckResult, AppError> {
    let updater = app
        .updater()
        .map_err(|e| AppError::Other(format!("updater unavailable: {e}")))?;

    let update = match updater.check().await {
        Ok(update) => update,
        Err(plugin_err) => match mirrored_update_check(&app).await {
            Ok(result) => return Ok(result),
            Err(_) => {
                return Err(AppError::Other(format!(
                    "update check failed: {plugin_err}"
                )));
            }
        },
    };

    match update {
        None => {
            info!(target: "updater", "already up to date (plugin reported no update)");
            clear_pending(&app);
            Ok(UpdateCheckResult {
                available: false,
                version: None,
                date: None,
                body: None,
                release_url: Some(RELEASES_PAGE.to_string()),
            })
        }
        Some(u) => {
            let version = u.version.clone();
            let body = u.body.clone();
            let date = u.date.map(|d| d.to_string());

            info!(
                target: "updater",
                "update available: v{version} (current v{})",
                u.current_version
            );

            let pending = app.state::<PendingUpdate>();
            let mut slot = pending
                .inner
                .lock()
                .map_err(|e| AppError::Other(format!("lock error: {e}")))?;
            *slot = Some(u);

            Ok(UpdateCheckResult {
                available: true,
                version: Some(version),
                date,
                body,
                release_url: Some(RELEASES_PAGE.to_string()),
            })
        }
    }
}

/// Download and install the pending update from a prior [`check_app_update`].
///
/// Emits `updater://download-progress` events with `{ chunk_length, content_length }`.
#[tauri::command]
pub async fn download_and_install_update(app: tauri::AppHandle) -> Result<(), AppError> {
    use tauri::Emitter;

    let update = {
        let pending = app.state::<PendingUpdate>();
        let mut slot = pending
            .inner
            .lock()
            .map_err(|e| AppError::Other(format!("lock error: {e}")))?;
        slot.take()
            .ok_or_else(|| AppError::Other("no pending update to download".to_string()))?
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
        .map_err(|e| AppError::Other(format!("download_and_install failed: {e}")))?;

    info!(target: "updater", "update downloaded and installed, ready for restart");
    Ok(())
}

/// Restart the app to apply the installed update.
#[tauri::command]
pub async fn restart_after_update(app: tauri::AppHandle) -> Result<(), AppError> {
    app.restart();
}

fn clear_pending(app: &tauri::AppHandle) {
    let pending = app.state::<PendingUpdate>();
    if let Ok(mut slot) = pending.inner.lock() {
        *slot = None;
    }
}

/// When the updater plugin cannot reach GitHub Releases (typical under GFW),
/// fetch `latest.json` through the anonymous GitHub-family chain. A newer
/// version is reported with a Releases URL for manual download — we never
/// install a binary that came through a third-party accelerator.
async fn mirrored_update_check(app: &tauri::AppHandle) -> Result<UpdateCheckResult, AppError> {
    let payload = skillstar_core::infra::github_http::fetch_github_latest_json(
        LATEST_JSON_URL,
        Duration::from_secs(15),
    )
    .await
    .map_err(|e| AppError::Other(format!("mirrored update check failed: {e:#}")))?;
    let remote = payload
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let current = app.package_info().version.to_string();
    clear_pending(app);
    if remote.is_empty() || !version_is_newer(remote, &current) {
        info!(target: "updater", "already up to date (mirrored latest.json {remote})");
        return Ok(UpdateCheckResult {
            available: false,
            version: None,
            date: None,
            body: None,
            release_url: Some(RELEASES_PAGE.to_string()),
        });
    }
    info!(target: "updater", "update available via mirror: v{remote} (current v{current})");
    Ok(UpdateCheckResult {
        available: true,
        version: Some(remote.to_string()),
        date: payload
            .get("pub_date")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        body: Some(
            payload
                .get("notes")
                .and_then(|v| v.as_str())
                .unwrap_or("A newer SkillStar is available. Download it from GitHub Releases — the signed updater endpoint is blocked.")
                .to_string(),
        ),
        release_url: Some(RELEASES_PAGE.to_string()),
    })
}

fn version_is_newer(remote: &str, current: &str) -> bool {
    let parse = |value: &str| -> Option<(u64, u64, u64)> {
        let value = value.trim().trim_start_matches('v');
        let mut parts = value.split('.');
        Some((
            parts.next()?.parse().ok()?,
            parts.next().unwrap_or("0").parse().ok()?,
            parts.next().unwrap_or("0").parse().ok()?,
        ))
    };
    match (parse(remote), parse(current)) {
        (Some(remote), Some(current)) => remote > current,
        _ => remote.trim() != current.trim(),
    }
}

#[cfg(test)]
mod tests {
    use super::version_is_newer;

    #[test]
    fn version_is_newer_compares_semver_triples() {
        assert!(version_is_newer("0.0.5", "0.0.4"));
        assert!(version_is_newer("v1.2.0", "1.1.9"));
        assert!(!version_is_newer("0.0.4", "0.0.4"));
        assert!(!version_is_newer("0.0.3", "0.0.4"));
    }
}

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

use serde::Serialize;
use skillstar_core::infra::error::AppError;
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;
use tracing::info;

/// Fallback page when the UI wants a human-readable release link.
const RELEASES_PAGE: &str = "https://github.com/xxww0098/SkillStar/releases/latest";

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

    let update = updater
        .check()
        .await
        .map_err(|e| AppError::Other(format!("update check failed: {e}")))?;

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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    /// Strip a single leading `v`/`V` and parse `major.minor.patch`.
    fn parse_semver_triple(raw: &str) -> Option<(u64, u64, u64)> {
        let s = raw.trim().trim_start_matches(['v', 'V']);
        if s.is_empty() {
            return None;
        }
        let mut parts = s.split('.');
        let major = parse_numeric_prefix(parts.next()?)?;
        let minor = parse_numeric_prefix(parts.next()?)?;
        let patch = parse_numeric_prefix(parts.next()?)?;
        Some((major, minor, patch))
    }

    fn parse_numeric_prefix(part: &str) -> Option<u64> {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return None;
        }
        digits.parse().ok()
    }

    fn is_remote_newer(remote: &str, current: &str) -> bool {
        match (parse_semver_triple(remote), parse_semver_triple(current)) {
            (Some(r), Some(c)) => r > c,
            _ => {
                let r = remote.trim().trim_start_matches(['v', 'V']);
                let c = current.trim().trim_start_matches(['v', 'V']);
                !r.is_empty() && r != c
            }
        }
    }

    #[test]
    fn parse_semver_accepts_v_prefix_and_plain() {
        assert_eq!(parse_semver_triple("v0.0.3"), Some((0, 0, 3)));
        assert_eq!(parse_semver_triple("0.0.2"), Some((0, 0, 2)));
        assert_eq!(parse_semver_triple("V1.2.10"), Some((1, 2, 10)));
    }

    #[test]
    fn parse_semver_strips_prerelease_suffix() {
        assert_eq!(parse_semver_triple("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_semver_triple("v2.0.0+build"), Some((2, 0, 0)));
    }

    #[test]
    fn parse_semver_rejects_garbage() {
        assert_eq!(parse_semver_triple(""), None);
        assert_eq!(parse_semver_triple("v"), None);
        assert_eq!(parse_semver_triple("abc"), None);
        assert_eq!(parse_semver_triple("1.2"), None);
    }

    #[test]
    fn is_remote_newer_compares_triples() {
        assert!(is_remote_newer("0.0.3", "0.0.2"));
        assert!(is_remote_newer("v1.0.0", "0.9.9"));
        assert!(!is_remote_newer("0.0.2", "0.0.2"));
        assert!(!is_remote_newer("0.0.1", "0.0.2"));
        assert!(!is_remote_newer("v0.0.3", "0.0.3"));
    }
}

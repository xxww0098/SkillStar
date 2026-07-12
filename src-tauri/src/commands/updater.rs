//! App update detection via GitHub Releases (with optional mirror).
//!
//! Active path (0.0.3+): query GitHub's public Releases API, compare the
//! latest non-draft / non-prerelease tag with the running app version, and
//! return a `release_url` the UI can open in the system browser.
//!
//! Deferred path: `download_and_install_update` / `restart_after_update` still
//! use `tauri-plugin-updater` for a future signed-artifact install once
//! `TAURI_SIGNING_PRIVATE_KEY` + `createUpdaterArtifacts` are restored. See
//! `docs/backend.md` § Auto-Update.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use skillstar_core::config::github_mirror;
use skillstar_core::infra::error::AppError;
use skillstar_core::infra::http_client::probe_http_client;
use tauri::Manager;
use tracing::{info, warn};

/// Canonical GitHub Releases API for the SkillStar repo.
const RELEASES_LATEST_API: &str =
    "https://api.github.com/repos/xxww0098/SkillStar/releases/latest";

/// Fallback page when the API payload has no `html_url`.
const RELEASES_PAGE: &str = "https://github.com/xxww0098/SkillStar/releases/latest";

const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

// ── State ──────────────────────────────────────────────────────────────

/// Holds the pending `Update` object between check → download → install
/// steps for the deferred tauri-plugin-updater install path.
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
    /// GitHub release page the user can open to download installers.
    pub release_url: Option<String>,
}

// ── GitHub payload ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

// ── Pure helpers (unit-tested) ─────────────────────────────────────────

/// Strip a single leading `v`/`V` and parse `major.minor.patch`.
/// Extra pre-release/build suffixes after the third number are ignored for
/// the numeric triple (e.g. `1.2.3-beta` → `(1,2,3)`).
pub(crate) fn parse_semver_triple(raw: &str) -> Option<(u64, u64, u64)> {
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

/// True when `remote` is a strictly greater semver than `current`.
/// Falls back to string inequality (after stripping `v`) when either side
/// is not parseable as `x.y.z`.
pub(crate) fn is_remote_newer(remote: &str, current: &str) -> bool {
    match (parse_semver_triple(remote), parse_semver_triple(current)) {
        (Some(r), Some(c)) => r > c,
        _ => {
            let r = remote.trim().trim_start_matches(['v', 'V']);
            let c = current.trim().trim_start_matches(['v', 'V']);
            !r.is_empty() && r != c
        }
    }
}

/// Normalise a tag like `v0.0.3` → `0.0.3` for display / comparison.
pub(crate) fn strip_v_prefix(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).to_string()
}

/// Candidate URLs for the Releases API: mirror-rewritten first (if any),
/// then the direct GitHub URL as fallback.
fn release_api_candidates() -> Vec<String> {
    let mut urls = Vec::with_capacity(2);
    if let Some(mirror_base) = github_mirror::effective_mirror_url() {
        urls.push(format!("{mirror_base}{RELEASES_LATEST_API}"));
    }
    urls.push(RELEASES_LATEST_API.to_string());
    urls
}

async fn fetch_latest_release(client: &reqwest::Client, user_agent: &str) -> Result<GitHubRelease, AppError> {
    let mut last_err: Option<String> = None;

    for url in release_api_candidates() {
        info!(target: "updater", "fetching release metadata from {url}");
        match client
            .get(&url)
            .header("User-Agent", user_agent)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    let snippet: String = body.chars().take(200).collect();
                    last_err = Some(format!("GitHub API {status}: {snippet}"));
                    warn!(target: "updater", "release fetch failed ({status}) via {url}");
                    continue;
                }
                return resp
                    .json::<GitHubRelease>()
                    .await
                    .map_err(|e| AppError::Other(format!("failed to parse release JSON: {e}")));
            }
            Err(e) => {
                last_err = Some(e.to_string());
                warn!(target: "updater", "release fetch error via {url}: {e}");
            }
        }
    }

    Err(AppError::Other(format!(
        "update check failed: {}",
        last_err.unwrap_or_else(|| "no endpoints tried".into())
    )))
}

// ── Commands ───────────────────────────────────────────────────────────

/// Check GitHub Releases for a newer app version.
///
/// Network / API failures return `Err` so the UI can show an honest error
/// instead of silently claiming "up to date".
#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle) -> Result<UpdateCheckResult, AppError> {
    let current = app.package_info().version.to_string();
    let user_agent = format!("SkillStar/{current}");

    let client = probe_http_client(CHECK_TIMEOUT)
        .map_err(|e| AppError::Other(format!("failed to build HTTP client: {e}")))?;

    let release = fetch_latest_release(&client, &user_agent).await?;

    if release.draft || release.prerelease {
        info!(target: "updater", "latest release is draft/prerelease — treating as no update");
        return Ok(UpdateCheckResult {
            available: false,
            version: None,
            date: None,
            body: None,
            release_url: None,
        });
    }

    let remote_version = strip_v_prefix(&release.tag_name);
    if remote_version.is_empty() {
        return Err(AppError::Other("release has empty tag_name".into()));
    }

    let release_url = if release.html_url.trim().is_empty() {
        RELEASES_PAGE.to_string()
    } else {
        release.html_url
    };

    if is_remote_newer(&remote_version, &current) {
        info!(
            target: "updater",
            "update available: v{remote_version} (current v{current})"
        );
        Ok(UpdateCheckResult {
            available: true,
            version: Some(remote_version),
            date: release.published_at,
            body: release.body,
            release_url: Some(release_url),
        })
    } else {
        info!(
            target: "updater",
            "already up to date (current v{current}, remote v{remote_version})"
        );
        Ok(UpdateCheckResult {
            available: false,
            version: Some(remote_version),
            date: release.published_at,
            body: None,
            release_url: Some(release_url),
        })
    }
}

/// Download and install the pending update (deferred tauri-plugin path).
///
/// Emits `updater://download-progress` events with `{ chunk_length, content_length }`.
/// Only usable once a prior check stored a plugin `Update` in [`PendingUpdate`]
/// and signed updater artifacts are published.
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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{is_remote_newer, parse_semver_triple, strip_v_prefix};

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

    #[test]
    fn strip_v_prefix_normalises() {
        assert_eq!(strip_v_prefix("v0.0.3"), "0.0.3");
        assert_eq!(strip_v_prefix("0.0.3"), "0.0.3");
        assert_eq!(strip_v_prefix("  V1.2.3 "), "1.2.3");
    }
}

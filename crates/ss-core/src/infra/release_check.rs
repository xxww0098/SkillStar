//! Check-only SkillStar release detection against GitHub Releases (D-103).
//!
//! The app never downloads or replaces itself: binaries are unsigned and the
//! signed updater was retired with D-091. This module only answers "is there
//! a published release newer than `current`", through the anonymous GitHub
//! chain (accelerators first, direct fallback) so the check still works where
//! github.com is slow. Results persist to `state/app/release_check.json`;
//! the About section shows the last record and can run a manual check, and a
//! background wake re-checks at most once every 24 h.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::infra::fs_ops::atomic_write;
use crate::infra::github_api_cooldown;
use crate::infra::github_http;
use crate::infra::paths;

pub const RELEASES_LATEST_API_URL: &str =
    "https://api.github.com/repos/xxww0098/SkillStar/releases/latest";
pub const RELEASES_PAGE_URL: &str = "https://github.com/xxww0098/SkillStar/releases/latest";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// GitHub answers 403 without usable reset headers once the anonymous chain
/// has wrapped the request, so the deadline is a conservative fixed window —
/// one hour is the upper bound of a rate-limit reset.
const RATE_LIMIT_COOLDOWN_SECS: u64 = 60 * 60;
const AUTO_CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReleaseCheckOutcome {
    /// `current` is at least the latest published tag.
    UpToDate,
    /// `/releases/latest` 404s until a release is published manually.
    NoPublishedRelease,
    Available {
        tag: String,
        url: String,
    },
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseCheckRecord {
    pub last_checked_unix: u64,
    pub current_version: String,
    pub outcome: ReleaseCheckOutcome,
}

/// Strict `MAJOR.MINOR.PATCH` compare after stripping one `v`/`V` prefix.
/// Anything non-numeric (prerelease tags, malformed tags) counts as not
/// newer: a bad upstream tag must never produce a false upgrade nudge.
pub fn is_newer_version(current: &str, latest_tag: &str) -> bool {
    match (parse_triple(current), parse_triple(latest_tag)) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    }
}

fn parse_triple(text: &str) -> Option<(u64, u64, u64)> {
    let trimmed = text.trim().trim_start_matches(['v', 'V']);
    let mut parts = trimmed.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Compare `current` against a `/releases/latest` payload.
fn evaluate_body(current: &str, body: &str) -> ReleaseCheckOutcome {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return failed("release payload was not valid JSON");
    };
    let Some(tag) = value.get("tag_name").and_then(|tag| tag.as_str()) else {
        return failed("release payload had no tag_name");
    };
    if !is_newer_version(current, tag) {
        return ReleaseCheckOutcome::UpToDate;
    }
    let url = value
        .get("html_url")
        .and_then(|url| url.as_str())
        .unwrap_or(RELEASES_PAGE_URL);
    ReleaseCheckOutcome::Available {
        tag: tag.to_string(),
        url: url.to_string(),
    }
}

fn failed(reason: &str) -> ReleaseCheckOutcome {
    ReleaseCheckOutcome::Failed {
        reason: reason.to_string(),
    }
}

/// Query GitHub Releases once. Never downloads anything. The result is
/// persisted as the last record unless the shared rate-limit cooldown
/// blocked the request (a blocked check keeps the previous record intact).
pub async fn run_check(current: &str) -> ReleaseCheckOutcome {
    let now = github_api_cooldown::now_unix();
    if github_api_cooldown::active(now) {
        return failed("GitHub API rate-limit cooldown is active");
    }
    let user_agent = format!("skillstar/{current}");
    let outcome = match github_http::get_anonymous_with_headers(
        RELEASES_LATEST_API_URL,
        REQUEST_TIMEOUT,
        &[
            ("User-Agent", user_agent.as_str()),
            ("Accept", "application/vnd.github+json"),
        ],
    )
    .await
    {
        Ok(response) => match response.text().await {
            Ok(body) => evaluate_body(current, &body),
            Err(error) => failed(&format!("release payload could not be read: {error}")),
        },
        Err(error) => {
            let message = format!("{error:#}");
            if message.contains("HTTP 404") {
                ReleaseCheckOutcome::NoPublishedRelease
            } else if message.contains("HTTP 403") || message.contains("HTTP 429") {
                github_api_cooldown::record(now + RATE_LIMIT_COOLDOWN_SECS);
                failed("GitHub API rate limit reached")
            } else {
                failed(&message)
            }
        }
    };
    persist_record(current, outcome.clone());
    outcome
}

/// The last persisted check result, shown by the About section on open.
pub fn last_record() -> Option<ReleaseCheckRecord> {
    std::fs::read_to_string(paths::app_release_check_state_path())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
}

/// Whether the background wake should check now: no record yet, or the last
/// check is older than 24 h. Manual checks ignore this.
pub fn should_auto_check(now: u64) -> bool {
    match last_record() {
        None => true,
        Some(record) => now.saturating_sub(record.last_checked_unix) >= AUTO_CHECK_INTERVAL_SECS,
    }
}

fn persist_record(current: &str, outcome: ReleaseCheckOutcome) {
    let record = ReleaseCheckRecord {
        last_checked_unix: github_api_cooldown::now_unix(),
        current_version: current.to_string(),
        outcome,
    };
    let path = paths::app_release_check_state_path();
    let Ok(content) = serde_json::to_string(&record) else {
        return;
    };
    if let Err(error) = atomic_write(&path, content.as_bytes()) {
        tracing::warn!(target: "release_check", path = %path.display(), "unable to persist the release check result: {error}");
    }
}

#[cfg(test)]
#[path = "release_check_tests.rs"]
mod tests;

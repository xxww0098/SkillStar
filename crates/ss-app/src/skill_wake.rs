//! Minute-level generic Skill auto-update wake for the GUI process.
//!
//! The switch lives in Settings (ss-core config), the work in ss-skills
//! (`update::auto_update_locked_skills`). This task only keeps a clock alive
//! while the desktop shell is running, remembers when the last run happened,
//! and stays quiet unless the user asked for automatic updates. Closing the
//! application stops it; nothing runs while the process is gone.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ss_skills::git_skill::GitSkillFacade;
use tracing::{debug, info, warn};

/// How often the monitor wakes to ask whether a run is due. Short enough that
/// flipping the switch in Settings takes effect within a minute. The spacing
/// between runs is the interval stored in the Skill update preference.
const WAKE: Duration = Duration::from_secs(60);

pub fn spawn(handle: &tokio::runtime::Handle) {
    handle.spawn(async {
        let mut interval = tokio::time::interval(WAKE);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Automatic mode after manual mode is a fresh request: run on the next
        // wake instead of waiting out the interval from the previous run.
        let mut enabled_before = false;
        loop {
            interval.tick().await;
            let (enabled, check_every) = preference();
            if !enabled {
                enabled_before = false;
                continue;
            }
            let due = !enabled_before || is_due(last_run_at(), Utc::now(), check_every);
            enabled_before = true;
            if due {
                run_once().await;
            }
        }
    });
}

async fn run_once() {
    // Record the attempt before the work so a failing repository waits for the
    // next interval instead of being retried on every wake.
    record_run(Utc::now());
    let report = GitSkillFacade::from_file_store().auto_update_skills().await;

    if let Some(error) = &report.error {
        warn!(target: "skill_auto_update", "automatic skill update failed: {error}");
        return;
    }
    if report.updated.is_empty() && report.failed.is_empty() && report.kept_local.is_empty() {
        debug!(
            target: "skill_auto_update",
            checked = report.checked,
            "automatic skill update: nothing to apply"
        );
        return;
    }
    info!(
        target: "skill_auto_update",
        checked = report.checked,
        updated = report.updated.len(),
        skipped = report.skipped.len(),
        kept_local = report.kept_local.len(),
        failed = report.failed.len(),
        "automatic skill update finished"
    );
}

fn preference() -> (bool, Duration) {
    match ss_core::config::skill_updates::load_config() {
        Ok(config) => (
            config.auto_update,
            Duration::from_secs(config.interval_minutes * 60),
        ),
        Err(error) => {
            warn!(target: "skill_auto_update", "unable to read the Skill update preference: {error}");
            (
                false,
                Duration::from_secs(ss_core::config::skill_updates::DEFAULT_INTERVAL_MINUTES * 60),
            )
        }
    }
}

/// `state/skill_auto_update.json` — scheduling state only. Counts are logged,
/// not stored: this file must never become a second update-state store.
#[derive(Debug, Default, Serialize, Deserialize)]
struct RunState {
    #[serde(default)]
    last_run_at: Option<String>,
}

fn read_state() -> RunState {
    let path = ss_core::infra::paths::skill_auto_update_state_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return RunState::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn record_run(at: DateTime<Utc>) {
    let path = ss_core::infra::paths::skill_auto_update_state_path();
    let state = RunState {
        last_run_at: Some(at.to_rfc3339()),
    };
    let Ok(content) = serde_json::to_string(&state) else {
        return;
    };
    if let Err(error) = ss_core::infra::fs_ops::atomic_write(&path, content.as_bytes()) {
        warn!(target: "skill_auto_update", "unable to record the automatic update run: {error}");
    }
}

fn last_run_at() -> Option<DateTime<Utc>> {
    parse_last_run(read_state().last_run_at.as_deref())
}

fn parse_last_run(raw: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw?)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// A missing, malformed or future timestamp counts as due: the interval exists
/// to bound traffic, not to make the user wait out a bad clock. A due run
/// records the current time first, so a clock that went backwards resets the
/// schedule once instead of spinning.
fn is_due(last: Option<DateTime<Utc>>, now: DateTime<Utc>, interval: Duration) -> bool {
    let Some(last) = last else {
        return true;
    };
    let elapsed = now.signed_duration_since(last).num_seconds();
    elapsed < 0 || elapsed >= interval.as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::{is_due, parse_last_run};
    use chrono::{DateTime, Duration, Utc};
    use std::time::Duration as StdDuration;

    fn every(minutes: u64) -> StdDuration {
        StdDuration::from_secs(minutes * 60)
    }

    fn at(seconds_ago: i64) -> DateTime<Utc> {
        Utc::now() - Duration::seconds(seconds_ago)
    }

    #[test]
    fn a_missing_run_is_due() {
        assert!(is_due(None, Utc::now(), every(30)));
    }

    #[test]
    fn a_recent_run_is_not_due() {
        assert!(!is_due(Some(at(60)), Utc::now(), every(30)));
    }

    #[test]
    fn a_run_older_than_the_interval_is_due() {
        let interval = every(30).as_secs() as i64;
        assert!(!is_due(Some(at(interval - 1)), Utc::now(), every(30)));
        assert!(is_due(Some(at(interval + 1)), Utc::now(), every(30)));
    }

    #[test]
    fn a_shorter_interval_is_due_before_the_default() {
        let now = Utc::now();
        let last = now - Duration::minutes(20);
        assert!(is_due(Some(last), now, every(15)));
        assert!(!is_due(Some(last), now, every(30)));
    }

    #[test]
    fn a_future_timestamp_after_a_clock_rollback_is_due_once() {
        let now = Utc::now();
        assert!(is_due(Some(now + Duration::seconds(3600)), now, every(30)));
        // The run records `now`; the next wake a minute later is not due.
        assert!(!is_due(Some(now), now + Duration::seconds(60), every(30)));
    }

    #[test]
    fn only_rfc3339_timestamps_parse() {
        assert!(parse_last_run(Some("2026-10-07T09:00:00Z")).is_some());
        assert!(parse_last_run(Some("2026-10-07T09:00:00+08:00")).is_some());
        assert!(parse_last_run(Some("yesterday")).is_none());
        assert!(parse_last_run(None).is_none());
    }
}

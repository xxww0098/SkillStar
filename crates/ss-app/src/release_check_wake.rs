//! Daily SkillStar release check wake for the GUI process.
//!
//! Policy stays in `ss-core::infra::release_check`. Like `channel_wake`,
//! this task only keeps a clock alive while the desktop shell is running:
//! it re-checks at most once every 24 h (and never while the shared GitHub
//! API cooldown is active), traces the outcome, and never emits UI events —
//! the About section reads the persisted record when opened.

use std::time::Duration;

use ss_core::infra::{github_api_cooldown, release_check};
use tracing::{debug, info};

/// Startup delay so the first check never competes with boot I/O.
const STARTUP_DELAY: Duration = Duration::from_secs(30);
/// How often the wake re-evaluates; the 24 h gate lives in `release_check`.
const WAKE: Duration = Duration::from_secs(60 * 60);

pub fn spawn(handle: &tokio::runtime::Handle, product_version: &'static str) {
    handle.spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        let mut interval = tokio::time::interval(WAKE);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            run_once(product_version).await;
        }
    });
}

async fn run_once(product_version: &str) {
    let now = github_api_cooldown::now_unix();
    if !release_check::should_auto_check(now) || github_api_cooldown::active(now) {
        return;
    }
    match release_check::run_check(product_version).await {
        release_check::ReleaseCheckOutcome::Available { tag, .. } => {
            info!(target: "release_check", tag = %tag, "a newer SkillStar release is published");
        }
        release_check::ReleaseCheckOutcome::Failed { reason } => {
            debug!(target: "release_check", "release check failed: {reason}");
        }
        _ => {}
    }
}

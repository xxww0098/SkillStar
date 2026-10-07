//! Minute-level shared-channel auto-update wake for the GUI process.
//!
//! Policy stays in ss-skills. This task only keeps a clock alive while the
//! desktop shell is running. It does not emit UI events; results are traced.

use std::time::Duration;

use ss_skills::channels::shared_channels::{
    ChannelSubscriptionRegistry, DiskChannelSubscriptionRegistry,
};
use ss_skills::github_auth::GitHubAuthErrorCode;
use tracing::{debug, info, warn};

const WAKE: Duration = Duration::from_secs(60);

pub fn spawn(handle: &tokio::runtime::Handle) {
    handle.spawn(async {
        let mut interval = tokio::time::interval(WAKE);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            run_once().await;
        }
    });
}

async fn run_once() {
    let registry = DiskChannelSubscriptionRegistry;
    let has_subscriptions = match registry.load_mutable() {
        Ok(store) => !store.subscriptions.is_empty(),
        Err(error) => {
            warn!(target: "channel_auto_update", "unable to read subscriptions: {error}");
            return;
        }
    };
    if !has_subscriptions {
        return;
    }

    let facade = match crate::channel_facade::production_facade() {
        Ok(facade) => facade,
        Err(error) if error.code == GitHubAuthErrorCode::NotAuthenticated => {
            debug!(target: "channel_auto_update", "skipping automatic update: {error}");
            return;
        }
        Err(error) => {
            warn!(target: "channel_auto_update", "unable to read GitHub credential: {error}");
            return;
        }
    };
    match facade.run_due_auto_updates().await {
        Ok(executions) if executions.is_empty() => {}
        Ok(executions) => {
            info!(
                target: "channel_auto_update",
                count = executions.len(),
                "automatic channel update finished"
            );
        }
        Err(error) => {
            warn!(target: "channel_auto_update", "automatic update failed: {error}");
        }
    }
}

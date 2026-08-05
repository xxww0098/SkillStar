use super::github_auth::GitHubAuthState;
use skillstar_skills::shared_channels::{
    ChannelSubscriptionRegistry, DiskChannelSubscriptionRegistry, SharedChannelErrorCode,
};
use tauri::{Emitter, Manager};
use tracing::{debug, warn};

const AUTO_UPDATE_WAKE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
pub const AUTO_UPDATE_EVENT: &str = "shared-channels://auto-update-completed";

pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(AUTO_UPDATE_WAKE_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            run_once(&app).await;
        }
    });
}

async fn run_once(app: &tauri::AppHandle) {
    let registry = DiskChannelSubscriptionRegistry;
    let has_subscriptions = registry
        .load_mutable()
        .map(|store| !store.subscriptions.is_empty())
        .unwrap_or(false);
    if !has_subscriptions {
        return;
    }

    let state = app.state::<GitHubAuthState>();
    let git_facade = match state.begin_git_operation(app.clone(), None) {
        Ok(facade) => facade,
        Err(error) => {
            warn!(target: "channel_auto_update", "unable to register automatic Git operation: {error}");
            return;
        }
    };
    let session_id = git_facade.session().id().to_string();
    let facade = match state.channel_subscription_facade(git_facade) {
        Ok(facade) => facade,
        Err(error) => {
            state.finish_git_operation(&session_id);
            if error.code == SharedChannelErrorCode::NotAuthenticated {
                debug!(target: "channel_auto_update", "waiting for GitHub authentication");
            } else {
                warn!(target: "channel_auto_update", "unable to start automatic updates: {error}");
            }
            return;
        }
    };
    let result = facade.run_due_auto_updates().await;
    state.finish_git_operation(&session_id);
    match result {
        Ok(executions) if !executions.is_empty() => {
            if let Err(error) = app.emit(AUTO_UPDATE_EVENT, executions) {
                warn!(target: "channel_auto_update", "unable to emit completion event: {error}");
            }
        }
        Ok(_) => {}
        Err(error) => {
            warn!(target: "channel_auto_update", "automatic update cycle failed: {error}");
        }
    }
}

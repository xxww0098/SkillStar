//! In-memory registry of pending OAuth login sessions.
//!
//! A login session is created by `start_oauth_login` and consumed by
//! `await_oauth_completion`. We do NOT persist these across restarts — if the
//! user kills the app mid-login the session is dropped.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use tokio::sync::oneshot;

use crate::subscription::Subscription;
use crate::{UsageError, UsageResult};

/// A pending login waiting for the user to complete the browser flow.
pub struct PendingLogin {
    pub catalog_id: String,
    pub region: Option<String>,
    pub target_subscription_id: Option<String>,
    pub auth_url: String,
    pub started_at: Instant,
    /// Sender resolved when the OAuth completes (success or failure).
    pub completion: Option<oneshot::Sender<UsageResult<Subscription>>>,
    /// Receiver consumed by `await_oauth_completion`.
    pub receiver: Option<oneshot::Receiver<UsageResult<Subscription>>>,
}

static REGISTRY: LazyLock<Mutex<HashMap<String, PendingLogin>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a new pending login. Returns the generated pending_id and the
/// auth_url the caller should open in a browser.
pub fn register(catalog_id: &str, region: Option<&str>, auth_url: String) -> String {
    let pending_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    REGISTRY.lock().unwrap().insert(
        pending_id.clone(),
        PendingLogin {
            catalog_id: catalog_id.to_string(),
            region: region.map(str::to_string),
            target_subscription_id: None,
            auth_url,
            started_at: Instant::now(),
            completion: Some(tx),
            receiver: Some(rx),
        },
    );
    pending_id
}

/// Attach an existing subscription id to a login session. OAuth fetchers that
/// support reauthorization can use this to refresh the old row instead of
/// creating a duplicate subscription.
pub fn set_target_subscription_id(pending_id: &str, subscription_id: Option<String>) {
    if let Some(pending) = REGISTRY.lock().unwrap().get_mut(pending_id) {
        pending.target_subscription_id = subscription_id;
    }
}

pub fn target_subscription_id(pending_id: &str) -> Option<String> {
    REGISTRY
        .lock()
        .unwrap()
        .get(pending_id)
        .and_then(|p| p.target_subscription_id.clone())
}

/// Look up the auth_url for a pending session.
pub fn auth_url(pending_id: &str) -> Option<String> {
    REGISTRY
        .lock()
        .unwrap()
        .get(pending_id)
        .map(|p| p.auth_url.clone())
}

/// Take the receiver half; caller awaits this. Idempotent — second take returns None.
pub fn take_receiver(pending_id: &str) -> Option<oneshot::Receiver<UsageResult<Subscription>>> {
    REGISTRY
        .lock()
        .unwrap()
        .get_mut(pending_id)
        .and_then(|p| p.receiver.take())
}

/// Take the sender half; the spawned OAuth worker resolves the login through this.
pub fn take_sender(pending_id: &str) -> Option<oneshot::Sender<UsageResult<Subscription>>> {
    REGISTRY
        .lock()
        .unwrap()
        .get_mut(pending_id)
        .and_then(|p| p.completion.take())
}

pub fn remove(pending_id: &str) {
    REGISTRY.lock().unwrap().remove(pending_id);
}

/// Cancel an in-flight login. Notifies the awaiter with `AuthRequired`.
pub fn cancel(pending_id: &str) -> UsageResult<()> {
    let mut reg = REGISTRY.lock().unwrap();
    let pending = reg
        .remove(pending_id)
        .ok_or_else(|| UsageError::NotFound(pending_id.to_string()))?;
    if let Some(tx) = pending.completion {
        let _ = tx.send(Err(UsageError::Other("用户取消登录".to_string())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_target_subscription_id_for_pending_login() {
        let pending_id = register("opencode", None, "https://auth.example.test".to_string());

        assert_eq!(target_subscription_id(&pending_id), None);

        set_target_subscription_id(&pending_id, Some("sub-opencode-old".to_string()));

        assert_eq!(
            target_subscription_id(&pending_id).as_deref(),
            Some("sub-opencode-old")
        );

        remove(&pending_id);
    }
}

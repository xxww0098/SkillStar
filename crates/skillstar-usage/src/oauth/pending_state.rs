//! In-memory registry of pending OAuth login sessions.
//!
//! A login session is created by `start_oauth_login` and consumed by
//! `await_oauth_completion`. We do NOT persist these across restarts — if the
//! user kills the app mid-login the session is dropped.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use tokio::sync::{mpsc, oneshot};

use crate::fetchers::oauth::OAuthFlow;
use crate::subscription::Subscription;
use crate::{UsageError, UsageResult};

/// A pending login waiting for the user (or a remote poll) to finish.
pub struct PendingLogin {
    pub catalog_id: String,
    pub region: Option<String>,
    pub target_subscription_id: Option<String>,
    pub auth_url: String,
    /// Local callback port when the OAuth flow binds one (Codex / Antigravity).
    pub callback_port: Option<u16>,
    /// How a pasted value, if any, is delivered. Defaults to [`OAuthFlow::LocalCallback`].
    pub flow: OAuthFlow,
    /// SchemePaste delivery. `None` for every other flow.
    pub manual_inbox_tx: Option<mpsc::UnboundedSender<String>>,
    pub started_at: Instant,
    /// Sender resolved when the OAuth completes (success or failure).
    pub completion: Option<oneshot::Sender<UsageResult<Subscription>>>,
    /// Receiver consumed by `await_oauth_completion`.
    pub receiver: Option<oneshot::Receiver<UsageResult<Subscription>>>,
}

static REGISTRY: LazyLock<Mutex<HashMap<String, PendingLogin>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Acquire the registry lock, recovering from a poisoned state instead of
/// panicking. A single panic while holding `REGISTRY` would otherwise poison
/// the lock and break every subsequent OAuth command (`start_oauth_login` /
/// `await_oauth_completion` / `cancel_oauth_login`). Mirrors the `into_inner`
/// recovery pattern used elsewhere in the codebase (`core::patrol`,
/// `skillstar_core::infra::paths`, etc.).
fn lock_registry() -> std::sync::MutexGuard<'static, HashMap<String, PendingLogin>> {
    REGISTRY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Register a new pending login. Returns the generated pending_id and the
/// auth_url the caller should open in a browser.
///
/// The session is [`OAuthFlow::LocalCallback`]. Callers that finish some other
/// way use [`register_with_flow`] or [`register_scheme_paste`].
pub fn register(catalog_id: &str, region: Option<&str>, auth_url: String) -> String {
    register_with_callback_port(catalog_id, region, auth_url, None)
}

/// Like [`register`], but records the bound local callback port for cancel cleanup.
pub fn register_with_callback_port(
    catalog_id: &str,
    region: Option<&str>,
    auth_url: String,
    callback_port: Option<u16>,
) -> String {
    insert(
        catalog_id,
        region,
        auth_url,
        callback_port,
        OAuthFlow::LocalCallback,
        None,
    )
}

/// Register a non-scheme session (`RemotePoll`, `Immediate`, or an explicit
/// `LocalCallback`). Scheme paste has to go through [`register_scheme_paste`]
/// so the caller can hold the receiving end.
pub fn register_with_flow(
    catalog_id: &str,
    region: Option<&str>,
    auth_url: String,
    flow: OAuthFlow,
) -> String {
    debug_assert!(
        !matches!(flow, OAuthFlow::SchemePaste { .. }),
        "SchemePaste sessions must use register_scheme_paste"
    );
    insert(catalog_id, region, auth_url, None, flow, None)
}

/// SchemePaste session. The receiver is what the provider task waits on;
/// [`crate::oauth::manual_callback::deliver_manual_input`] sends the pasted URL.
pub fn register_scheme_paste(
    catalog_id: &str,
    region: Option<&str>,
    auth_url: String,
    scheme_prefix: impl Into<String>,
) -> (String, mpsc::UnboundedReceiver<String>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let pending_id = insert(
        catalog_id,
        region,
        auth_url,
        None,
        OAuthFlow::SchemePaste {
            scheme_prefix: scheme_prefix.into(),
        },
        Some(tx),
    );
    (pending_id, rx)
}

fn insert(
    catalog_id: &str,
    region: Option<&str>,
    auth_url: String,
    callback_port: Option<u16>,
    flow: OAuthFlow,
    manual_inbox_tx: Option<mpsc::UnboundedSender<String>>,
) -> String {
    let pending_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    lock_registry().insert(
        pending_id.clone(),
        PendingLogin {
            catalog_id: catalog_id.to_string(),
            region: region.map(str::to_string),
            target_subscription_id: None,
            auth_url,
            callback_port,
            flow,
            manual_inbox_tx,
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
    if let Some(pending) = lock_registry().get_mut(pending_id) {
        pending.target_subscription_id = subscription_id;
    }
}

pub fn target_subscription_id(pending_id: &str) -> Option<String> {
    lock_registry()
        .get(pending_id)
        .and_then(|p| p.target_subscription_id.clone())
}

/// Look up the auth_url for a pending session.
pub fn auth_url(pending_id: &str) -> Option<String> {
    lock_registry().get(pending_id).map(|p| p.auth_url.clone())
}

pub fn flow(pending_id: &str) -> Option<OAuthFlow> {
    lock_registry().get(pending_id).map(|p| p.flow.clone())
}

pub fn catalog_id(pending_id: &str) -> Option<String> {
    lock_registry()
        .get(pending_id)
        .map(|p| p.catalog_id.clone())
}

/// Hand one pasted scheme URL to the provider task waiting on this session.
pub fn send_manual_inbox(pending_id: &str, input: String) -> UsageResult<()> {
    let tx = {
        let reg = lock_registry();
        let pending = reg
            .get(pending_id)
            .ok_or_else(|| UsageError::NotFound(pending_id.to_string()))?;
        pending
            .manual_inbox_tx
            .clone()
            .ok_or_else(|| UsageError::Other("登录会话没有手动回调通道".into()))?
    };
    tx.send(input)
        .map_err(|_| UsageError::Other("登录会话已结束".into()))
}

/// Take the receiver half; caller awaits this. Idempotent — second take returns None.
pub fn take_receiver(pending_id: &str) -> Option<oneshot::Receiver<UsageResult<Subscription>>> {
    lock_registry()
        .get_mut(pending_id)
        .and_then(|p| p.receiver.take())
}

/// Take the sender half; the spawned OAuth worker resolves the login through this.
pub fn take_sender(pending_id: &str) -> Option<oneshot::Sender<UsageResult<Subscription>>> {
    lock_registry()
        .get_mut(pending_id)
        .and_then(|p| p.completion.take())
}

pub fn remove(pending_id: &str) {
    lock_registry().remove(pending_id);
}

/// Cancel an in-flight login. Notifies the awaiter with `AuthRequired`.
pub fn cancel(pending_id: &str) -> UsageResult<()> {
    let mut reg = lock_registry();
    let pending = reg
        .remove(pending_id)
        .ok_or_else(|| UsageError::NotFound(pending_id.to_string()))?;
    release_callback_listener(&pending);
    if let Some(tx) = pending.completion {
        let _ = tx.send(Err(UsageError::Other("用户取消登录".to_string())));
    }
    Ok(())
}

fn release_callback_listener(pending: &PendingLogin) {
    use crate::oauth::local_server;
    if let Some(port) = pending.callback_port {
        let _ = local_server::request_cancel(port);
    }
    if pending.catalog_id == "codex" {
        let _ = local_server::request_cancel(1455);
        let _ = local_server::request_cancel(1457);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::error::TryRecvError;

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

    #[test]
    fn register_defaults_to_local_callback() {
        let plain = register("codex", None, "https://auth.example/plain".into());
        let with_port = register_with_callback_port(
            "codex",
            None,
            "https://auth.example/port".into(),
            Some(1455),
        );

        assert_eq!(flow(&plain), Some(OAuthFlow::LocalCallback));
        assert_eq!(flow(&with_port), Some(OAuthFlow::LocalCallback));
        assert_eq!(catalog_id(&plain).as_deref(), Some("codex"));

        remove(&plain);
        remove(&with_port);
    }

    #[test]
    fn flow_round_trips() {
        let remote = register_with_flow(
            "qoder",
            None,
            "https://auth.example/poll".into(),
            OAuthFlow::RemotePoll,
        );
        let immediate = register_with_flow(
            "anthropic",
            None,
            "https://claude.ai".into(),
            OAuthFlow::Immediate,
        );
        let (scheme, inbox) =
            register_scheme_paste("zcode", None, "zcode://login".into(), "zcode://");

        assert_eq!(flow(&remote), Some(OAuthFlow::RemotePoll));
        assert_eq!(flow(&immediate), Some(OAuthFlow::Immediate));
        assert_eq!(
            flow(&scheme),
            Some(OAuthFlow::SchemePaste {
                scheme_prefix: "zcode://".into()
            })
        );
        drop(inbox);

        remove(&remote);
        remove(&immediate);
        remove(&scheme);
    }

    #[test]
    fn manual_inbox_sends_and_receives() {
        let (id, mut inbox) =
            register_scheme_paste("zcode", None, "zcode://login".into(), "zcode://");

        send_manual_inbox(&id, "zcode://callback?ok=1".into()).unwrap();

        assert_eq!(inbox.try_recv().unwrap(), "zcode://callback?ok=1");
        remove(&id);
    }

    #[test]
    fn cancel_drops_the_session_and_inbox() {
        let (id, mut inbox) =
            register_scheme_paste("zcode", None, "zcode://login".into(), "zcode://");
        let mut awaiter = take_receiver(&id).expect("awaiter");

        cancel(&id).unwrap();

        assert!(flow(&id).is_none());
        assert!(matches!(inbox.try_recv(), Err(TryRecvError::Disconnected)));
        match awaiter.try_recv() {
            Ok(Err(err)) => assert!(err.to_string().contains("取消"), "{err}"),
            other => panic!("cancel should wake the awaiter, got {other:?}"),
        }
    }

    #[test]
    fn remove_after_timeout_drops_the_session_and_inbox() {
        // The listener reports timeout on the completion channel. The awaiter
        // then `remove`s the session — that drop is what closes the inbox.
        let (id, mut inbox) =
            register_scheme_paste("zcode", None, "zcode://login".into(), "zcode://");
        let sender = take_sender(&id).expect("completion sender");
        sender
            .send(Err(UsageError::Other("OAuth 回调超时".into())))
            .unwrap();

        remove(&id);

        assert!(flow(&id).is_none());
        assert!(matches!(inbox.try_recv(), Err(TryRecvError::Disconnected)));
    }
}

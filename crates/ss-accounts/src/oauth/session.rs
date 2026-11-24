//! Pending login sessions: what a started login looks like to the UI, and the
//! ids that key the in-process registry.
//!
//! The old implementation kept a single global flow slot, so two logins could
//! never run at once. Here each login has its own id and state: a login is a
//! pair of channels (a pasted callback goes in, the outcome comes out) plus the
//! descriptor the UI renders. The finishing work runs on its own thread, which
//! is why complete() can block on a plain std channel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use serde::{Deserialize, Serialize};

use crate::oauth::LoginOutcome;
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::loopback::QueryMap;

/// The identity of one in-flight login.
pub type LoginId = String;

/// How the UI has to drive this login.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoginFlow {
    /// A loopback listener receives the callback; the user may also paste the
    /// callback URL.
    LocalCallback,
    /// The backend polls the provider; the UI shows a code and a link.
    RemotePoll,
    /// A custom URL scheme; the process parses the pasted URL itself.
    SchemePaste,
    /// Finishes in place with no user action.
    Immediate,
}

impl LoginFlow {
    /// Whether a pasted callback URL or bare authorization code can finish this
    /// flow. The UI uses it to decide whether to render the paste row.
    pub fn accepts_paste(self) -> bool {
        matches!(self, Self::LocalCallback | Self::SchemePaste)
    }
}

/// A login the UI can render.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStart {
    pub login_id: LoginId,
    pub flow: LoginFlow,
    pub auth_url: String,
    #[serde(default)]
    pub user_code: Option<String>,
    #[serde(default)]
    pub verification_uri: Option<String>,
    #[serde(default)]
    pub interval_secs: Option<u64>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

/// What a caller may tune when starting a login.
#[derive(Debug, Clone, Default)]
pub struct StartOptions {
    /// A preferred loopback port (the provider fixed a redirect URI).
    pub prefer_port: Option<u16>,
    /// An explicit provider login leg selector (e.g. `"portal"` / `"idc"` for Kiro).
    pub leg: Option<String>,
    /// A provider region/workspace selector.
    pub region: Option<String>,
    /// An explicit IAM Identity Center start URL (Kiro IdC).
    pub start_url: Option<String>,
}

/// How the finishing thread learns that the user cancelled.
#[derive(Debug, Clone, Default)]
pub(crate) struct CancelSignal(Arc<AtomicBool>);

impl CancelSignal {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn trigger(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// The finishing thread's half of one login: where it sends the result and how
/// it notices a cancel.
pub(crate) struct LoginChannel {
    pub(crate) results: mpsc::Sender<OAuthResult<LoginOutcome>>,
    pub(crate) cancel: CancelSignal,
}

/// The registry's half of one login: the way in for a pasted callback and the
/// way out for the outcome.
pub(crate) struct PendingLogin {
    callbacks: Option<mpsc::Sender<QueryMap>>,
    /// A custom URL scheme's callback keeps its host and path, so it cannot be
    /// flattened into a query map: those flows get the pasted text verbatim.
    texts: Option<mpsc::Sender<String>>,
    results: Mutex<Option<mpsc::Receiver<OAuthResult<LoginOutcome>>>>,
    /// Kept so cancel() can push a cancellation into whoever is waiting.
    result_sender: mpsc::Sender<OAuthResult<LoginOutcome>>,
    cancel: CancelSignal,
}

impl PendingLogin {
    /// Build the registry-side handle and the finishing thread's handle.
    ///
    /// The callbacks sender is None for flows that cannot be finished by
    /// pasting anything; submit_callback then rejects instead of hanging.
    pub(crate) fn pair(callbacks: Option<mpsc::Sender<QueryMap>>) -> (Self, LoginChannel) {
        Self::build(callbacks, None)
    }

    /// The same pair for a custom-URL-scheme flow: the caller gets the pasted
    /// text as it was typed, because the URL's host is part of what it means.
    pub(crate) fn pair_scheme() -> (Self, LoginChannel, mpsc::Receiver<String>) {
        let (texts_tx, texts_rx) = mpsc::channel();
        let (pending, channel) = Self::build(None, Some(texts_tx));
        (pending, channel, texts_rx)
    }

    fn build(
        callbacks: Option<mpsc::Sender<QueryMap>>,
        texts: Option<mpsc::Sender<String>>,
    ) -> (Self, LoginChannel) {
        let (result_sender, results) = mpsc::channel();
        let cancel = CancelSignal::default();
        let pending = Self {
            callbacks,
            texts,
            results: Mutex::new(Some(results)),
            result_sender: result_sender.clone(),
            cancel: cancel.clone(),
        };
        (
            pending,
            LoginChannel {
                results: result_sender,
                cancel,
            },
        )
    }
}

struct Entry {
    start: LoginStart,
    pending: PendingLogin,
}

/// The in-process pending-login registry. One map keyed by login id, so any
/// number of logins can be in flight at once.
#[derive(Default)]
pub struct Registry {
    sessions: Mutex<HashMap<LoginId, Entry>>,
}

impl Registry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a started login.
    pub(crate) fn insert(&self, start: LoginStart, pending: PendingLogin) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(start.login_id.clone(), Entry { start, pending });
        }
    }

    /// The pending login's descriptor, if it is still pending.
    pub fn get(&self, login_id: &str) -> Option<LoginStart> {
        self.lock()
            .ok()
            .and_then(|sessions| sessions.get(login_id).map(|entry| entry.start.clone()))
    }

    /// How many logins are pending.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.lock().map(|sessions| sessions.len()).unwrap_or(0)
    }

    /// Drop a login; returns whether it was there.
    pub fn remove(&self, login_id: &str) -> bool {
        self.lock()
            .map(|mut sessions| sessions.remove(login_id).is_some())
            .unwrap_or(false)
    }

    /// Take the outcome channel out of a pending login.
    ///
    /// A second call on the same id fails: the receiver is already with the
    /// first caller, and two waiters would race for one result.
    pub(crate) fn take_receiver(
        &self,
        login_id: &str,
    ) -> OAuthResult<mpsc::Receiver<OAuthResult<LoginOutcome>>> {
        let sessions = self.lock()?;
        let entry = sessions
            .get(login_id)
            .ok_or_else(|| OAuthError::UnknownSession(login_id.to_string()))?;
        let mut slot = entry
            .pending
            .results
            .lock()
            .map_err(|_| OAuthError::Other("登录会话锁已中毒".to_string()))?;
        slot.take()
            .ok_or_else(|| OAuthError::Other("该登录的结果已被取走".to_string()))
    }

    /// Inject a pasted callback's query map into the login that waits for it.
    pub(crate) fn deliver(&self, login_id: &str, query: QueryMap) -> OAuthResult<()> {
        let sessions = self.lock()?;
        let entry = sessions
            .get(login_id)
            .ok_or_else(|| OAuthError::UnknownSession(login_id.to_string()))?;
        let Some(callbacks) = entry.pending.callbacks.as_ref() else {
            return Err(OAuthError::Rejected(
                "该登录流程只能由浏览器回调完成，不支持粘贴".to_string(),
            ));
        };
        callbacks
            .send(query)
            .map_err(|_| OAuthError::UnknownSession(format!("{login_id}: 登录会话已结束")))
    }

    /// Inject a pasted custom-scheme callback URL into the login that waits for it.
    pub(crate) fn deliver_text(&self, login_id: &str, text: String) -> OAuthResult<()> {
        let sessions = self.lock()?;
        let entry = sessions
            .get(login_id)
            .ok_or_else(|| OAuthError::UnknownSession(login_id.to_string()))?;
        let Some(texts) = entry.pending.texts.as_ref() else {
            return Err(OAuthError::Rejected(
                "该登录流程不接受自定义协议回调".to_string(),
            ));
        };
        texts
            .send(text)
            .map_err(|_| OAuthError::UnknownSession(format!("{login_id}: 登录会话已结束")))
    }

    /// The flow of a pending login.
    pub(crate) fn flow_of(&self, login_id: &str) -> OAuthResult<LoginFlow> {
        self.get(login_id)
            .map(|start| start.flow)
            .ok_or_else(|| OAuthError::UnknownSession(login_id.to_string()))
    }

    /// Forget a login and make its waiter return a cancellation error.
    pub fn cancel(&self, login_id: &str) -> bool {
        let Ok(mut sessions) = self.lock() else {
            return false;
        };
        let Some(entry) = sessions.remove(login_id) else {
            return false;
        };
        entry.pending.cancel.trigger();
        // Dropping the callback senders unblocks an idle finishing thread; the
        // send below unblocks one that is parked in complete().
        drop(entry.pending.callbacks);
        drop(entry.pending.texts);
        let _ = entry
            .pending
            .result_sender
            .send(Err(OAuthError::Rejected("登录已取消".to_string())));
        true
    }

    fn lock(&self) -> OAuthResult<std::sync::MutexGuard<'_, HashMap<LoginId, Entry>>> {
        self.sessions
            .lock()
            .map_err(|_| OAuthError::Other("登录注册表锁已中毒".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::{Identity, TokenSet};
    use std::time::Duration;

    fn start(login_id: &str, flow: LoginFlow) -> LoginStart {
        LoginStart {
            login_id: login_id.to_string(),
            flow,
            auth_url: format!("https://example.invalid/{login_id}"),
            user_code: None,
            verification_uri: None,
            interval_secs: None,
            expires_at: None,
        }
    }

    fn outcome(agent: &str) -> LoginOutcome {
        LoginOutcome {
            agent: agent.to_string(),
            tokens: TokenSet {
                access_token: "access".to_string(),
                ..TokenSet::default()
            },
            identity: Identity::default(),
            provider_state: None,
        }
    }

    #[test]
    fn two_logins_run_side_by_side() {
        let registry = Registry::new();
        let (pending_a, channel_a) = PendingLogin::pair(None);
        let (pending_b, channel_b) = PendingLogin::pair(None);
        registry.insert(start("a", LoginFlow::LocalCallback), pending_a);
        registry.insert(start("b", LoginFlow::RemotePoll), pending_b);
        assert_eq!(registry.len(), 2);

        let receiver_a = registry.take_receiver("a").expect("receiver a");
        let receiver_b = registry.take_receiver("b").expect("receiver b");
        channel_b.results.send(Ok(outcome("b"))).expect("send b");
        channel_a.results.send(Ok(outcome("a"))).expect("send a");

        assert_eq!(
            receiver_a
                .recv_timeout(Duration::from_secs(5))
                .expect("a")
                .expect("ok")
                .agent,
            "a"
        );
        assert_eq!(
            receiver_b
                .recv_timeout(Duration::from_secs(5))
                .expect("b")
                .expect("ok")
                .agent,
            "b"
        );
    }

    #[test]
    fn second_complete_is_an_error() {
        let registry = Registry::new();
        let (pending, _channel) = PendingLogin::pair(None);
        registry.insert(start("a", LoginFlow::LocalCallback), pending);

        assert!(registry.take_receiver("a").is_ok());
        assert!(
            registry.take_receiver("a").is_err(),
            "the receiver can only leave once"
        );
        assert!(matches!(
            registry.take_receiver("nope"),
            Err(OAuthError::UnknownSession(_))
        ));
    }

    #[test]
    fn deliver_reaches_the_flow_and_refuses_the_others() {
        let registry = Registry::new();
        let (callback_tx, callback_rx) = mpsc::channel();
        let (pending, _channel) = PendingLogin::pair(Some(callback_tx));
        registry.insert(start("codex", LoginFlow::LocalCallback), pending);
        let (pending_poll, _channel_poll) = PendingLogin::pair(None);
        registry.insert(start("cursor", LoginFlow::RemotePoll), pending_poll);

        let mut query = QueryMap::new();
        query.insert("code".to_string(), "abc".to_string());
        registry
            .deliver("codex", query.clone())
            .expect("paste into a local-callback flow");
        assert_eq!(
            callback_rx.recv_timeout(Duration::from_secs(5)).expect("query"),
            query
        );

        let error = registry
            .deliver("cursor", query.clone())
            .expect_err("a remote poll flow cannot be finished by pasting");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
        assert!(error.to_string().contains("不支持粘贴"));

        assert!(matches!(
            registry.deliver("unknown", query),
            Err(OAuthError::UnknownSession(_))
        ));
    }

    #[test]
    fn cancel_wakes_the_waiter_with_a_cancellation_error() {
        let registry = Registry::new();
        let (pending, _channel) = PendingLogin::pair(None);
        registry.insert(start("a", LoginFlow::RemotePoll), pending);
        let receiver = registry.take_receiver("a").expect("receiver");

        let waiter = std::thread::spawn(move || receiver.recv_timeout(Duration::from_secs(10)));
        assert!(registry.cancel("a"));
        let result = waiter.join().expect("waiter").expect("a message");
        assert!(matches!(result, Err(OAuthError::Rejected(_))), "got {result:?}");
        assert!(registry.get("a").is_none());
        assert!(!registry.cancel("a"), "a cancelled login is gone");
    }

    #[test]
    fn cancel_signals_the_finishing_thread() {
        let registry = Registry::new();
        let (pending, channel) = PendingLogin::pair(None);
        registry.insert(start("a", LoginFlow::RemotePoll), pending);
        assert!(!channel.cancel.is_cancelled());
        assert!(registry.cancel("a"));
        assert!(channel.cancel.is_cancelled());
    }

    #[test]
    fn flows_know_whether_they_accept_a_paste() {
        assert!(LoginFlow::LocalCallback.accepts_paste());
        assert!(LoginFlow::SchemePaste.accepts_paste());
        assert!(!LoginFlow::RemotePoll.accepts_paste());
        assert!(!LoginFlow::Immediate.accepts_paste());
    }
}

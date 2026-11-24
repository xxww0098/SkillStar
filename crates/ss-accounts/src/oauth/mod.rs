//! OAuth login and token capability for the accounts crate.
//!
//! Owns the OAuth primitives (PKCE, loopback callback, device code, remote
//! poll, token endpoint exchange and refresh), the pending-login registry, and
//! the per-agent login differences. Persistence belongs to `store` and `record`;
//! quota belongs to `crate::usage`.
//!
//! Agent identity crosses this module as the `ss-skills` agent registry id
//! string (`codex`, `claude-code`, `cursor`), matching `crate::Agent`.
//!
//! # Flows
//!
//! | agent | flow | how it finishes |
//! | --- | --- | --- |
//! | `codex` | LocalCallback | a 127.0.0.1 listener, or a pasted callback URL / code |
//! | `cursor` | RemotePoll | the provider is polled every 2s for up to 5 minutes |
//! | `antigravity` | LocalCallback | a localhost listener on /oauth-callback (Google OAuth, no PKCE) |
//! | `claude-code` | Immediate | no browser leg: complete() returns AuthRequired |
//!
//! Claude Code has no OAuth login here, on purpose: the CLI exposes no
//! authorize endpoint a third-party process can drive, so its credentials only
//! enter the account library by importing the local store or pasting a token.
//! What this crate does offer for `claude-code` is `refresh`.
//!
//! # Threading
//!
//! `start` returns as soon as the flow is armed; a worker thread does the
//! waiting. `complete` blocks the calling thread until that worker reports, so
//! call it from a blocking context (the GUI does). Every login holds its own
//! id and channels, so several can be in flight at once.

use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

// 切号写回也要读 agent 自己的常量（Grok 的 scope key 与清单），
// 所以这一层对 crate 内可见，对外仍然私有。
pub(crate) mod agents;
pub mod error;
pub(crate) mod jwt;
pub(crate) mod loopback;
pub mod pkce;
pub(crate) mod rt;
pub mod session;
pub mod token;

pub use error::OAuthError;
pub use session::{LoginFlow, LoginStart, StartOptions};

/// How long `complete` waits for a worker before giving up. Longer than any
/// flow's own timeout so the flow reports first.
const COMPLETE_TIMEOUT: Duration = Duration::from_secs(660);

/// How early a token counts as expired for the agents that did not ask for
/// anything else: a switch must never hand a client a credential that dies
/// before its first request.
pub(crate) const DEFAULT_REFRESH_SKEW_SECONDS: i64 = 120;

/// The refresh lead time for one agent. Google's desktop token is hour-scale
/// and its refresh is cheap, so Antigravity refreshes a full 15 minutes ahead.
pub fn refresh_skew_seconds(agent: &str) -> i64 {
    crate::Agent::from_id(agent)
        .map(|parsed| crate::agents::spec(parsed).oauth.refresh_skew_seconds)
        .unwrap_or(DEFAULT_REFRESH_SKEW_SECONDS)
}

static REGISTRY: OnceLock<session::Registry> = OnceLock::new();

fn registry() -> &'static session::Registry {
    REGISTRY.get_or_init(session::Registry::new)
}

/// Unix seconds, the unit every timestamp in this crate uses.
pub(crate) fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The agents this module can log in.
#[cfg(test)]
pub fn supported_agents() -> &'static [&'static str] {
    &crate::agents::SUPPORTED_IDS
}

/// Whether `agent` (an `ss-skills` agent id) is registered in `ss-accounts`.
pub fn supports(agent: &str) -> bool {
    crate::Agent::from_id(agent).is_ok()
}

/// One set of tokens as a provider handed them over.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    /// Unix seconds. `None` when the provider does not say.
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub token_type: String,
}

impl TokenSet {
    /// Whether the access token is expired or inside `skew_secs` of expiring.
    pub fn needs_refresh(&self, now: i64, skew_secs: i64) -> bool {
        match self.expires_at {
            Some(at) => at <= now + skew_secs,
            None => false,
        }
    }
}

/// What the provider told us about the account behind the tokens.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Identity {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
}

/// A finished login: tokens plus whatever identity the flow could read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginOutcome {
    pub agent: String,
    pub tokens: TokenSet,
    pub identity: Identity,
    /// Provider-private refresh context (an IdC registration, a machine id...).
    /// Opaque outside provider-specific refresh code; `record` stores it encrypted.
    #[serde(default)]
    pub provider_state: Option<serde_json::Value>,
}

/// Tokens after a refresh, plus a rotated provider state if the refresh
/// changed one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshOutcome {
    pub tokens: TokenSet,
    #[serde(default)]
    pub provider_state: Option<serde_json::Value>,
}

/// Start a login for `agent`. The returned session stays pending until
/// `complete` resolves it or `cancel` drops it.
pub fn start(agent: &str, opts: &StartOptions) -> Result<LoginStart, OAuthError> {
    if !supports(agent) {
        return Err(OAuthError::UnsupportedAgent(agent.to_string()));
    }
    let login_id = uuid::Uuid::new_v4().to_string();
    let started = agents::start(agent, &login_id, opts)?;
    let start = started.start.clone();
    tracing::debug!(agent, login_id = %start.login_id, flow = ?start.flow, "login started");
    registry().insert(started.start, started.pending);
    Ok(start)
}

/// Feed a pasted callback URL or bare authorization code into a pending login.
///
/// A flow that cannot be finished by pasting (a remote poll, an immediate
/// login) rejects instead of hanging, and the text itself must be a callback
/// for this machine.
pub fn submit_callback(login_id: &str, text: &str) -> Result<(), OAuthError> {
    let flow = registry().flow_of(login_id)?;
    if !flow.accepts_paste() {
        return Err(OAuthError::Rejected(format!(
            "该登录流程（{flow:?}）不支持粘贴回调"
        )));
    }
    // 自定义 URL scheme 的回调带 host：压成 query map 会丢掉它，所以这类流程
    // 拿原文（由 agent 自己校验协议、主机与 state）。
    if flow == LoginFlow::SchemePaste {
        return registry().deliver_text(login_id, text.trim().to_string());
    }
    let query = loopback::parse_callback_text(text)?;
    registry().deliver(login_id, query)
}

/// Block until the pending login finishes, then take its outcome. A second
/// call on the same id rejects: the receiver was already taken.
pub fn complete(login_id: &str) -> Result<LoginOutcome, OAuthError> {
    let receiver = registry().take_receiver(login_id)?;
    let received = receiver.recv_timeout(COMPLETE_TIMEOUT);
    // Whoever completes a login owns it; the descriptor must not linger.
    registry().remove(login_id);
    match received {
        Ok(outcome) => outcome,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(OAuthError::Rejected("等待登录结果超时".to_string()))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(OAuthError::Other(
            "登录会话在返回结果前结束".to_string(),
        )),
    }
}

/// Forget a pending login. Unknown ids are a no-op. A caller already blocked
/// in `complete` gets a cancellation error.
pub fn cancel(login_id: &str) {
    registry().cancel(login_id);
}

/// Refresh `tokens` for `agent`, carrying `provider_state` through.
///
/// Failures keep the three-way split: `AuthRequired` clears the credential,
/// `Transient` keeps the last good state, `Rejected` is a real refusal. A
/// provider that does not rotate a field keeps the previous value.
pub fn refresh(
    agent: &str,
    tokens: &TokenSet,
    provider_state: Option<&serde_json::Value>,
) -> Result<RefreshOutcome, OAuthError> {
    if !supports(agent) {
        return Err(OAuthError::UnsupportedAgent(agent.to_string()));
    }
    let tokens = agents::refresh(agent, tokens, provider_state)?;
    Ok(RefreshOutcome {
        tokens,
        provider_state: provider_state.cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_agents_are_the_registry_ids_in_rail_order() {
        // 顺序 = rail 顺序（crate::Agent::supported），具体列表由 agents.rs 的
        // SUPPORTED_IDS 唯一守卫，不再在此重复硬编码。
        let expected: Vec<&str> = crate::Agent::supported()
            .iter()
            .map(|agent| agent.id())
            .collect();
        assert_eq!(supported_agents(), expected.as_slice());
        for agent in supported_agents() {
            assert!(supports(agent), "{agent}");
        }
        assert!(!supports("droid"));
    }

    #[test]
    fn unknown_agents_cannot_start_or_refresh() {
        let opts = StartOptions::default();
        assert!(matches!(
            start("droid", &opts),
            Err(OAuthError::UnsupportedAgent(_))
        ));
        assert!(matches!(
            refresh("droid", &TokenSet::default(), None),
            Err(OAuthError::UnsupportedAgent(_))
        ));
    }

    #[test]
    fn codex_login_is_a_local_callback_with_a_paste_door() {
        let started = start("codex", &StartOptions::default()).expect("start");
        assert_eq!(started.flow, LoginFlow::LocalCallback);
        assert!(
            started
                .auth_url
                .starts_with("https://auth.openai.com/oauth/authorize?")
        );
        assert!(started.auth_url.contains("code_challenge_method=S256"));
        assert!(started.auth_url.contains("codex_cli_simplified_flow=true"));
        assert!(started.expires_at.is_some());

        // A callback link for some other host is refused, not read as a code.
        let error = submit_callback(&started.login_id, "https://evil.example/cb?code=abc")
            .expect_err("foreign callback host");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
        assert!(matches!(
            submit_callback("no-such-login", "abc"),
            Err(OAuthError::UnknownSession(_))
        ));
        cancel(&started.login_id);
    }

    #[test]
    fn two_logins_of_the_same_agent_are_pending_at_once() {
        let opts = StartOptions::default();
        let first = start("codex", &opts).expect("first");
        let second = start("codex", &opts).expect("second");
        assert_ne!(first.login_id, second.login_id);
        assert_ne!(first.auth_url, second.auth_url, "each login has its own state");
        cancel(&first.login_id);
        cancel(&second.login_id);
    }

    #[test]
    fn cursor_login_is_polled_and_refuses_a_paste() {
        let started = start("cursor", &StartOptions::default()).expect("start");
        assert_eq!(started.flow, LoginFlow::RemotePoll);
        assert!(
            started
                .auth_url
                .starts_with("https://cursor.com/loginDeepControl?")
        );
        assert_eq!(started.interval_secs, Some(2));
        let error = submit_callback(&started.login_id, "code-1")
            .expect_err("a poll flow has nothing to paste into");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
        cancel(&started.login_id);
    }

    #[test]
    fn claude_code_login_completes_as_auth_required() {
        let started = start("claude-code", &StartOptions::default()).expect("start");
        assert_eq!(started.flow, LoginFlow::Immediate);
        assert!(started.auth_url.is_empty());

        match complete(&started.login_id) {
            Err(OAuthError::AuthRequired(message)) => {
                assert!(message.contains("从本机导入"), "message: {message}");
                assert!(message.contains("粘贴令牌"), "message: {message}");
            }
            other => panic!("expected AuthRequired, got {other:?}"),
        }
        // Completing takes the login away, so a second call cannot hang.
        assert!(matches!(
            complete(&started.login_id),
            Err(OAuthError::UnknownSession(_))
        ));
        assert!(matches!(
            submit_callback(&started.login_id, "abc"),
            Err(OAuthError::UnknownSession(_))
        ));
    }

    #[test]
    fn cancel_ends_a_blocked_complete_with_an_error() {
        let started = start("cursor", &StartOptions::default()).expect("start");
        let login_id = started.login_id.clone();
        let waiter = std::thread::spawn(move || complete(&login_id));
        std::thread::sleep(Duration::from_millis(150));
        cancel(&started.login_id);
        let result = waiter.join().expect("waiter thread");
        assert!(result.is_err(), "a cancelled login never yields tokens");
    }

    #[test]
    fn refresh_without_a_refresh_token_asks_for_a_new_login() {
        for agent in supported_agents() {
            let error = refresh(agent, &TokenSet::default(), None).expect_err("nothing to refresh");
            assert!(
                matches!(error, OAuthError::AuthRequired(_)),
                "{agent}: got {error:?}"
            );
        }
    }

    #[test]
    fn tokens_know_when_they_need_refreshing() {
        let tokens = TokenSet {
            access_token: "access".to_string(),
            expires_at: Some(1_000),
            ..TokenSet::default()
        };
        assert!(tokens.needs_refresh(1_000, 0));
        assert!(tokens.needs_refresh(700, 300));
        assert!(!tokens.needs_refresh(600, 300));
        assert!(!TokenSet::default().needs_refresh(1_000_000, 300));
    }
}


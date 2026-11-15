//! Per-agent login and refresh differences.
//!
//! The agents differ in more than endpoints. Codex finishes through a loopback
//! listener (with a paste fallback), Cursor is polled remotely, Antigravity is a
//! Google loopback redirect without PKCE, and Claude Code has no browser leg at
//! all: its credentials only arrive by importing the local client store or
//! pasting a token. The public entry points in lib.rs stay agent-agnostic and
//! dispatch here.

pub(crate) mod antigravity;
pub(crate) mod antigravity_config;
pub(crate) mod claude_code;
pub(crate) mod codex;
pub(crate) mod cursor;
pub(crate) mod gemini_cli;
pub(crate) mod gemini_cli_config;
pub(crate) mod grok;
pub(crate) mod kiro;
pub(crate) mod zcode;
pub(crate) mod devin_desktop;

use crate::oauth::TokenSet;
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::loopback::QueryMap;
use crate::oauth::session::{LoginFlow, LoginStart, PendingLogin, StartOptions};

/// One started login: the descriptor the UI gets plus the registry's half.
pub(crate) struct Started {
    pub(crate) start: LoginStart,
    pub(crate) pending: PendingLogin,
}

/// 为无独立浏览器授权端点的本地客户端生成立即报错 `AuthRequired` 的登录流。
pub(crate) fn immediate_auth_required(login_id: &str, error: OAuthError) -> OAuthResult<Started> {
    let (pending, channel) = PendingLogin::pair(None);
    let _ = channel.results.send(Err(error));
    Ok(Started {
        start: LoginStart {
            login_id: login_id.to_string(),
            flow: LoginFlow::Immediate,
            auth_url: String::new(),
            user_code: None,
            verification_uri: None,
            interval_secs: None,
            expires_at: None,
        },
        pending,
    })
}

/// Start the login flow this agent supports.
pub(crate) fn start(agent: &str, login_id: &str, opts: &StartOptions) -> OAuthResult<Started> {
    let parsed = crate::Agent::from_id(agent)
        .map_err(|_| OAuthError::UnsupportedAgent(agent.to_string()))?;
    (crate::agents::spec(parsed).oauth.start)(login_id, opts)
}

/// Refresh an agent's tokens. The provider's own spelling of the response is
/// each module's business; the caller only sees a TokenSet.
///
/// `provider_state` is carried because one agent cannot refresh without it: Kiro's
/// AWS IAM Identity Center leg needs the client registration (clientId /
/// clientSecret / region) that its login persisted. Every other agent ignores it,
/// so their behaviour is unchanged.
pub(crate) fn refresh(
    agent: &str,
    tokens: &TokenSet,
    provider_state: Option<&serde_json::Value>,
) -> OAuthResult<TokenSet> {
    let parsed = crate::Agent::from_id(agent)
        .map_err(|_| OAuthError::UnsupportedAgent(agent.to_string()))?;
    (crate::agents::spec(parsed).oauth.refresh)(tokens, provider_state)
}

/// Pull the authorization code out of a callback, refusing a denied or
/// mismatched one. A bare pasted code carries no state, so it is only checked
/// for presence.
///
/// Two providers redirect to a loopback listener with the same callback shape
/// (Codex on `/auth/callback`, Antigravity on `/oauth-callback`), so the
/// extraction lives here rather than in either flow.
pub(crate) fn code_from_query(query: &QueryMap, expected_state: &str) -> OAuthResult<String> {
    if let Some(error) = query.get("error").filter(|value| !value.is_empty()) {
        let description = query.get("error_description").cloned().unwrap_or_default();
        return Err(OAuthError::Rejected(format!(
            "授权被拒绝: {error} {description}"
        )));
    }
    if let Some(state) = query.get("state").filter(|value| !value.is_empty())
        && state != expected_state
    {
        return Err(OAuthError::Rejected(
            "回调的 state 与本次登录不匹配".to_string(),
        ));
    }
    query
        .get("code")
        .filter(|code| !code.is_empty())
        .cloned()
        .ok_or_else(|| OAuthError::Rejected("回调里没有授权码".to_string()))
}

/// The refresh token a refresh needs, or an AuthRequired the caller can act on.
pub(crate) fn refresh_token_of(agent: &str, tokens: &TokenSet) -> OAuthResult<String> {
    tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            OAuthError::AuthRequired(format!("{agent} 账号没有 refresh token，需要重新登录"))
        })
}

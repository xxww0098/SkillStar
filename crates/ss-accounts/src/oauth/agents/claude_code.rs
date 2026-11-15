//! Claude Code: refresh only, no browser login leg.
//!
//! The Claude Code CLI does not expose an OAuth authorize endpoint that a
//! third-party process can drive, so this crate does not pretend to log one in.
//! start() answers with an Immediate flow that completes as AuthRequired, which
//! tells the UI to offer the two doors that do work: importing the machine's
//! own credential, or pasting a token. Refresh is fully supported.

use serde_json::Value;

use crate::oauth::TokenSet;
use crate::oauth::agents::{Started, immediate_auth_required};
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::session::StartOptions;
use crate::oauth::token::{self, TokenFlavor};

pub(crate) const AGENT_ID: &str = "claude-code";

const TOKEN_ENDPOINT: &str = "https://console.anthropic.com/v1/oauth/token";
/// The Claude Code CLI's public OAuth client id.
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// The message complete() returns for this agent.
pub(crate) const LOGIN_REQUIRED: &str = "Claude Code 请用「从本机导入」或「粘贴令牌」";

/// Open an Immediate login that resolves to AuthRequired right away.
pub(crate) fn start(login_id: &str, _opts: &StartOptions) -> OAuthResult<Started> {
    immediate_auth_required(login_id, OAuthError::AuthRequired(LOGIN_REQUIRED.to_string()))
}

/// The refresh request body, pure for testing.
pub(crate) fn refresh_body(refresh_token: &str) -> Value {
    serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLIENT_ID,
    })
}

/// Refresh a Claude Code credential.
pub(crate) fn refresh(tokens: &TokenSet) -> OAuthResult<TokenSet> {
    let refresh_token = crate::oauth::agents::refresh_token_of(AGENT_ID, tokens)?;
    let body = refresh_body(&refresh_token);
    let refreshed = token::post_json_tokens(TOKEN_ENDPOINT, &body, TokenFlavor::Snake)?;
    Ok(token::carry_over(tokens, refreshed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::session::LoginFlow;
    use crate::oauth::token::classify_status;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn start_is_immediate_and_carries_no_url() {
        let started = start("login-1", &StartOptions::default()).expect("start");
        assert_eq!(started.start.flow, LoginFlow::Immediate);
        assert!(started.start.auth_url.is_empty());
        assert_eq!(started.start.login_id, "login-1");
        assert!(started.start.expires_at.is_none());
    }

    #[test]
    fn complete_answers_auth_required_with_both_doors() {
        let started = start("login-1", &StartOptions::default()).expect("start");
        let registry = crate::oauth::session::Registry::new();
        registry.insert(started.start, started.pending);
        let receiver = registry.take_receiver("login-1").expect("receiver");
        let result = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("message is already buffered");
        match result {
            Err(OAuthError::AuthRequired(message)) => {
                assert!(message.contains("从本机导入"), "message: {message}");
                assert!(message.contains("粘贴令牌"), "message: {message}");
            }
            other => panic!("expected AuthRequired, got {other:?}"),
        }
    }

    #[test]
    fn refresh_request_body_shape() {
        assert_eq!(
            refresh_body("refresh-1"),
            json!({
                "grant_type": "refresh_token",
                "refresh_token": "refresh-1",
                "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
            })
        );
        assert_eq!(TOKEN_ENDPOINT, "https://console.anthropic.com/v1/oauth/token");
    }

    #[test]
    fn refresh_without_a_refresh_token_requires_a_login() {
        let error = refresh(&TokenSet::default()).expect_err("no refresh token");
        assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");
    }

    #[test]
    fn failure_classification_keeps_5xx_retryable() {
        assert!(matches!(
            classify_status(401, "{}"),
            OAuthError::AuthRequired(_)
        ));
        assert!(matches!(
            classify_status(400, "{\"error\":\"invalid_grant\"}"),
            OAuthError::AuthRequired(_)
        ));
        for status in [429, 500, 503] {
            assert!(
                matches!(classify_status(status, ""), OAuthError::Transient(_)),
                "{status} must stay retryable"
            );
        }
        assert!(matches!(
            classify_status(403, ""),
            OAuthError::Rejected(_)
        ));
    }
}


//! Cursor login: a remote-poll flow with no local listener.
//!
//! The browser finishes on cursor.com and our side polls api2.cursor.sh until
//! the tokens appear (150 attempts, two seconds apart). The poll carries the
//! plaintext PKCE verifier, so the login completes without the user pasting
//! anything: RemotePoll, not LocalCallback.
//!
//! Identity is best effort: cursor.com/api/auth/me needs the session cookie
//! Cursor's own web app uses, which we can build from the access token. A
//! failure there leaves the identity empty and the login untouched.

use std::time::Duration;

use serde_json::Value;
use url::Url;

use crate::oauth::agents::Started;
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::pkce;
use crate::oauth::session::{CancelSignal, LoginFlow, LoginStart, PendingLogin, StartOptions};
use crate::oauth::token::{self, TokenFlavor};
use crate::oauth::{Identity, LoginOutcome, TokenSet, jwt, rt};

pub(crate) const AGENT_ID: &str = "cursor";

const LOGIN_URL: &str = "https://cursor.com/loginDeepControl";
const POLL_ENDPOINT: &str = "https://api2.cursor.sh/auth/poll";
const TOKEN_ENDPOINT: &str = "https://api2.cursor.sh/oauth/token";
const PROFILE_ENDPOINT: &str = "https://cursor.com/api/auth/me";
/// The Cursor desktop app's public OAuth client id.
const CLIENT_ID: &str = "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_POLLS: u32 = 150;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Start a Cursor login: hand the user the link and poll in the background.
pub(crate) fn start(login_id: &str, _opts: &StartOptions) -> OAuthResult<Started> {
    let pkce = pkce::generate();
    let login_uuid = uuid::Uuid::new_v4().to_string();
    let auth_url = authorize_url(&pkce.challenge, &login_uuid);

    // Nothing can be pasted into a RemotePoll flow.
    let (pending, channel) = PendingLogin::pair(None);
    let verifier = pkce.verifier;
    std::thread::spawn(move || {
        let outcome = poll_for_tokens(&login_uuid, &verifier, &channel.cancel);
        let _ = channel.results.send(outcome);
    });

    Ok(Started {
        start: LoginStart {
            login_id: login_id.to_string(),
            flow: LoginFlow::RemotePoll,
            auth_url,
            user_code: None,
            verification_uri: None,
            interval_secs: Some(POLL_INTERVAL.as_secs()),
            expires_at: Some(
                crate::oauth::now_secs() + i64::from(MAX_POLLS) * POLL_INTERVAL.as_secs() as i64,
            ),
        },
        pending,
    })
}

/// The link the user opens. Pure so a test can check the parameters.
pub(crate) fn authorize_url(challenge: &str, login_uuid: &str) -> String {
    let mut url = Url::parse(LOGIN_URL).expect("static login endpoint parses");
    url.query_pairs_mut()
        .append_pair("challenge", challenge)
        .append_pair("uuid", login_uuid)
        .append_pair("mode", "login");
    url.to_string()
}

/// The poll URL. The provider wants the verifier in plaintext here.
pub(crate) fn poll_url(login_uuid: &str, verifier: &str) -> String {
    let mut url = Url::parse(POLL_ENDPOINT).expect("static poll endpoint parses");
    url.query_pairs_mut()
        .append_pair("uuid", login_uuid)
        .append_pair("verifier", verifier);
    url.to_string()
}

/// The refresh request body, pure for testing.
pub(crate) fn refresh_body(refresh_token: &str) -> Value {
    serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": CLIENT_ID,
        "refresh_token": refresh_token,
    })
}

/// Refresh a Cursor credential.
pub(crate) fn refresh(tokens: &TokenSet) -> OAuthResult<TokenSet> {
    let refresh_token = crate::oauth::agents::refresh_token_of(AGENT_ID, tokens)?;
    let body = refresh_body(&refresh_token);
    let refreshed = token::post_json_tokens(TOKEN_ENDPOINT, &body, TokenFlavor::Camel)?;
    Ok(token::carry_over(tokens, refreshed))
}

/// Read one poll answer: only a body that carries both tokens finishes the
/// login, anything else means "keep polling".
pub(crate) fn parse_poll_body(body: &str) -> Option<(TokenSet, Option<String>)> {
    let value: Value = serde_json::from_str(body).ok()?;
    let access_token = string_field(&value, &["accessToken", "access_token"])?;
    let refresh_token = string_field(&value, &["refreshToken", "refresh_token"])?;
    let auth_id = string_field(&value, &["authId", "auth_id"]);
    Some((
        TokenSet {
            access_token,
            refresh_token: Some(refresh_token),
            id_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
        },
        auth_id,
    ))
}

/// The WorkOS user id inside a Cursor access token: the text after the last
/// '|' of the sub claim, and only when it really is a user id.
pub(crate) fn user_id_from_access_token(access_token: &str) -> Option<String> {
    let payload = jwt::payload(access_token)?;
    let subject = payload.get("sub")?.as_str()?.trim();
    let user_id = subject.rsplit('|').next()?.trim();
    user_id.starts_with("user_").then(|| user_id.to_string())
}

/// The cookie Cursor's own web app sends along with the access token.
pub(crate) fn session_cookie(access_token: &str) -> Option<String> {
    let user_id = user_id_from_access_token(access_token)?;
    Some(format!(
        "WorkosCursorSessionToken={user_id}%3A%3A{access_token}"
    ))
}

fn poll_for_tokens(
    login_uuid: &str,
    verifier: &str,
    cancel: &CancelSignal,
) -> OAuthResult<LoginOutcome> {
    let runtime = rt::runtime()?;
    let url = poll_url(login_uuid, verifier);
    runtime.block_on(async move {
        let client = ss_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT)
            .map_err(|error| OAuthError::Other(format!("无法构建 HTTP 客户端: {error}")))?;
        for attempt in 0..MAX_POLLS {
            // Wait before each poll, including the first: the user has not even
            // opened the link yet, and a cancelled login never touches the wire.
            tokio::time::sleep(POLL_INTERVAL).await;
            if cancel.is_cancelled() {
                return Err(OAuthError::Rejected("登录已取消".to_string()));
            }
            if let Some((tokens, auth_id)) = poll_once(&client, &url).await {
                let identity = read_identity(&client, &tokens, auth_id).await;
                return Ok(LoginOutcome {
                    agent: AGENT_ID.to_string(),
                    tokens,
                    identity,
                    provider_state: None,
                });
            }
            tracing::debug!(attempt, "cursor login still waiting for the browser");
        }
        Err(OAuthError::Rejected(
            "Cursor 登录轮询超时，请重新发起登录".to_string(),
        ))
    })
}

/// One poll. A refusal, an empty answer or a transport hiccup all mean the same
/// thing here: the user has not finished in the browser yet.
async fn poll_once(client: &reqwest::Client, url: &str) -> Option<(TokenSet, Option<String>)> {
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    parse_poll_body(&response.text().await.ok()?)
}

async fn read_identity(
    client: &reqwest::Client,
    tokens: &TokenSet,
    auth_id: Option<String>,
) -> Identity {
    let fallback_email = auth_id.filter(|value| value.contains('@'));
    let Some(cookie) = session_cookie(&tokens.access_token) else {
        return Identity {
            email: fallback_email,
            ..Identity::default()
        };
    };
    let profile = client
        .get(PROFILE_ENDPOINT)
        .header("Accept", "application/json")
        .header("Cookie", cookie)
        .send()
        .await;
    let email = match profile {
        Ok(response) if response.status().is_success() => response
            .json::<Value>()
            .await
            .ok()
            .and_then(|value| jwt::claim(&value, &["email"])),
        _ => None,
    };
    Identity {
        email: email.or(fallback_email),
        ..Identity::default()
    }
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::token::classify_status;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn query_of(url: &str) -> BTreeMap<String, String> {
        Url::parse(url)
            .expect("url parses")
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect()
    }

    #[test]
    fn authorize_url_carries_the_challenge_and_the_login_uuid() {
        let url = authorize_url("challenge-1", "uuid-1");
        let parsed = Url::parse(&url).expect("url");
        assert_eq!(parsed.host_str(), Some("cursor.com"));
        assert_eq!(parsed.path(), "/loginDeepControl");

        let query = query_of(&url);
        assert_eq!(
            query.get("challenge").map(String::as_str),
            Some("challenge-1")
        );
        assert_eq!(query.get("uuid").map(String::as_str), Some("uuid-1"));
        assert_eq!(query.get("mode").map(String::as_str), Some("login"));
    }

    #[test]
    fn poll_url_carries_the_plaintext_verifier() {
        let url = poll_url("uuid-1", "verifier-1");
        let parsed = Url::parse(&url).expect("url");
        assert_eq!(parsed.host_str(), Some("api2.cursor.sh"));
        assert_eq!(parsed.path(), "/auth/poll");
        let query = query_of(&url);
        assert_eq!(query.get("uuid").map(String::as_str), Some("uuid-1"));
        assert_eq!(
            query.get("verifier").map(String::as_str),
            Some("verifier-1")
        );
    }

    #[test]
    fn refresh_request_body_shape() {
        let body = refresh_body("refresh-1");
        assert_eq!(
            body,
            json!({
                "grant_type": "refresh_token",
                "client_id": "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB",
                "refresh_token": "refresh-1",
            })
        );
        assert_eq!(TOKEN_ENDPOINT, "https://api2.cursor.sh/oauth/token");
    }

    #[test]
    fn refresh_without_a_refresh_token_requires_a_login() {
        let error = refresh(&TokenSet::default()).expect_err("no refresh token");
        assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");
    }

    #[test]
    fn failure_classification_keeps_5xx_retryable() {
        assert!(matches!(
            classify_status(401, ""),
            OAuthError::AuthRequired(_)
        ));
        assert!(matches!(
            classify_status(400, "{\"error\":\"invalid_refresh_token\"}"),
            OAuthError::AuthRequired(_)
        ));
        for status in [429, 500, 503] {
            assert!(
                matches!(classify_status(status, ""), OAuthError::Transient(_)),
                "{status} must stay retryable"
            );
        }
    }

    #[test]
    fn poll_body_needs_both_tokens() {
        let body = json!({
            "accessToken": "access-1",
            "refreshToken": "refresh-1",
            "authId": "user-1"
        })
        .to_string();
        let (tokens, auth_id) = parse_poll_body(&body).expect("both tokens");
        assert_eq!(tokens.access_token, "access-1");
        assert_eq!(tokens.refresh_token.as_deref(), Some("refresh-1"));
        assert_eq!(auth_id.as_deref(), Some("user-1"));

        assert!(parse_poll_body(&json!({ "accessToken": "a" }).to_string()).is_none());
        assert!(parse_poll_body(&json!({ "refreshToken": "r" }).to_string()).is_none());
        assert!(parse_poll_body("").is_none());
        assert!(parse_poll_body("not json").is_none());
    }

    #[test]
    fn user_id_comes_from_the_sub_claim_after_the_last_pipe() {
        let token = crate::oauth::jwt::test_token(&json!({ "sub": "auth0|user_123" }));
        assert_eq!(user_id_from_access_token(&token).as_deref(), Some("user_123"));

        let plain = crate::oauth::jwt::test_token(&json!({ "sub": "user_456" }));
        assert_eq!(user_id_from_access_token(&plain).as_deref(), Some("user_456"));

        let not_a_user = crate::oauth::jwt::test_token(&json!({ "sub": "auth0|abc" }));
        assert_eq!(user_id_from_access_token(&not_a_user), None);

        let no_subject = crate::oauth::jwt::test_token(&json!({ "email": "a@b.c" }));
        assert_eq!(user_id_from_access_token(&no_subject), None);
        assert_eq!(user_id_from_access_token("not-a-jwt"), None);
    }

    #[test]
    fn session_cookie_pairs_the_user_id_with_the_token() {
        let token = crate::oauth::jwt::test_token(&json!({ "sub": "auth0|user_123" }));
        let cookie = session_cookie(&token).expect("cookie");
        assert_eq!(
            cookie,
            format!("WorkosCursorSessionToken=user_123%3A%3A{token}")
        );

        let anonymous = crate::oauth::jwt::test_token(&json!({ "sub": "auth0|abc" }));
        assert_eq!(session_cookie(&anonymous), None);
    }
}


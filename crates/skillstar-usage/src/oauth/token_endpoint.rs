//! The one place this crate talks to an OAuth 2.0 token endpoint.
//!
//! Codex, Antigravity (Google), Grok and OpenCode all POST the same
//! `application/x-www-form-urlencoded` grant and all have to answer the same
//! question about the response: *is this a revoked grant the user must
//! re-authorize, or a hiccup we should retry?* Four hand-written copies gave
//! four different answers — Codex treated every non-2xx as a revoked grant
//! (so one OpenAI 5xx wiped the card and demanded re-login), while Google's
//! refresh treated none of them as one (so a genuinely revoked Antigravity
//! grant never surfaced a re-authorize button, only a raw JSON body). The
//! verdict now lives here once.
//!
//! Contract (see `docs/features/usage/README.md`):
//!
//! | upstream                                          | error                        |
//! |---------------------------------------------------|------------------------------|
//! | 401, or `error` ∈ {`invalid_grant`,`invalid_token`}| [`UsageError::AuthRequired`] |
//! | 429 / 5xx / transport failure                     | [`UsageError::Transient`]    |
//! | any other non-2xx                                 | [`UsageError::Fetcher`]      |
//!
//! Only the first row may latch `requires_reauth`. Google reports a revoked
//! refresh token as **400** + `{"error":"invalid_grant"}`, which is exactly
//! why the status code alone is not the test.

use chrono::Utc;
use serde::Deserialize;

use crate::oauth::token_refresh;
use crate::{UsageError, UsageResult};

/// The subset of an RFC 6749 token response every provider here consumes.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct TokenResponse {
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

impl TokenResponse {
    /// Non-blank access token.
    pub fn access_token(&self) -> Option<&str> {
        non_blank(self.access_token.as_deref())
    }

    /// Non-blank refresh token, when the provider rotated one.
    pub fn refresh_token(&self) -> Option<&str> {
        non_blank(self.refresh_token.as_deref())
    }

    /// Non-blank OIDC id token, when the grant included one.
    pub fn id_token(&self) -> Option<&str> {
        non_blank(self.id_token.as_deref())
    }

    /// Absolute expiry as a **UTC epoch second**, matching
    /// `Subscription::access_token_expires_at`.
    ///
    /// `expires_in` is authoritative; the JWT `exp` claims are the same
    /// best-effort fallbacks the providers used to spell out one at a time.
    pub fn expires_at(&self) -> Option<i64> {
        self.expires_in
            .map(|seconds| Utc::now().timestamp() + seconds)
            .or_else(|| self.access_token().and_then(token_refresh::jwt_exp))
            .or_else(|| self.id_token().and_then(token_refresh::jwt_exp))
    }
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

/// POST a grant to `url` and classify the outcome.
///
/// `label` prefixes every error message (e.g. `"Codex refresh"`), so the user
/// still learns which provider and which leg of the flow failed.
pub async fn post_token(
    url: &str,
    form: &[(&str, &str)],
    label: &str,
) -> UsageResult<TokenResponse> {
    let client = crate::http_client::usage_http_client()?;
    let response = client
        .post(url)
        .form(form)
        .send()
        .await
        .map_err(|error| UsageError::transport(label, error))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    parse_token_body(status, &body, label)
}

/// Status + body → token response or a classified error. Split out from
/// [`post_token`] so the classification is testable without a socket.
pub fn parse_token_body(status: u16, body: &str, label: &str) -> UsageResult<TokenResponse> {
    if !(200..300).contains(&status) {
        return Err(classify_failure(status, body, label));
    }
    let tokens: TokenResponse = serde_json::from_str(body)
        .map_err(|error| UsageError::Fetcher(format!("{label} 响应解析失败: {error}")))?;
    // A 2xx that carries no usable access token is an unusable grant, not a
    // provider outage: the user has to log in again.
    if tokens.access_token().is_none() {
        return Err(UsageError::AuthRequired);
    }
    Ok(tokens)
}

fn classify_failure(status: u16, body: &str, label: &str) -> UsageError {
    let revoked = matches!(
        oauth_error_code(body).as_deref(),
        Some("invalid_grant" | "invalid_token")
    );
    if status == 401 || revoked {
        return UsageError::AuthRequired;
    }
    UsageError::http_status(label, status, body)
}

/// The RFC 6749 `error` field, when the body is the JSON object the spec asks
/// for. Anything else (HTML error pages, proxies) yields `None`.
fn oauth_error_code(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("error")
        .and_then(|error| {
            // Some gateways nest it as `{"error":{"status":"..."}}`.
            error
                .as_str()
                .map(str::to_string)
                .or_else(|| error.get("status")?.as_str().map(str::to_string))
        })
        .map(|code| code.trim().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revoked_grant_is_auth_required_on_any_status() {
        // Google reports a revoked refresh token as 400, not 401.
        let error = parse_token_body(
            400,
            r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#,
            "Google refresh",
        )
        .unwrap_err();
        assert!(matches!(error, UsageError::AuthRequired), "{error:?}");

        let error = parse_token_body(401, "not json at all", "Codex refresh").unwrap_err();
        assert!(matches!(error, UsageError::AuthRequired), "{error:?}");

        let error =
            parse_token_body(400, r#"{"error":"invalid_token"}"#, "Grok refresh").unwrap_err();
        assert!(matches!(error, UsageError::AuthRequired), "{error:?}");
    }

    #[test]
    fn rate_limit_and_server_errors_are_transient_never_auth() {
        for status in [429_u16, 500, 502, 503] {
            let error = parse_token_body(status, "upstream is sad", "Codex refresh").unwrap_err();
            assert!(
                matches!(error, UsageError::Transient(_)),
                "status {status} must stay retryable, got {error:?}"
            );
            assert!(error.is_transient());
        }
    }

    #[test]
    fn other_client_errors_stay_plain_fetcher_errors() {
        let error = parse_token_body(
            400,
            r#"{"error":"unsupported_grant_type"}"#,
            "Codex token",
        )
        .unwrap_err();
        assert!(matches!(error, UsageError::Fetcher(_)), "{error:?}");
        assert!(!error.is_transient());
        assert!(error.to_string().contains("unsupported_grant_type"));
    }

    #[test]
    fn success_without_access_token_demands_reauthorization() {
        let error = parse_token_body(200, r#"{"refresh_token":"rt"}"#, "Grok refresh").unwrap_err();
        assert!(matches!(error, UsageError::AuthRequired), "{error:?}");

        let error = parse_token_body(200, r#"{"access_token":"   "}"#, "Grok refresh").unwrap_err();
        assert!(matches!(error, UsageError::AuthRequired), "{error:?}");
    }

    #[test]
    fn expires_at_prefers_expires_in_over_jwt_exp() {
        let tokens = parse_token_body(
            200,
            r#"{"access_token":"at","expires_in":3600}"#,
            "Codex token",
        )
        .unwrap();
        let expected = Utc::now().timestamp() + 3600;
        let actual = tokens.expires_at().expect("expires_in yields an expiry");
        assert!((actual - expected).abs() <= 2, "{actual} vs {expected}");

        // No `expires_in` and an opaque access token → unknown, not a guess.
        let tokens = parse_token_body(200, r#"{"access_token":"opaque"}"#, "Codex token").unwrap();
        assert_eq!(tokens.expires_at(), None);
    }

    #[test]
    fn blank_optional_tokens_read_as_absent() {
        let tokens = parse_token_body(
            200,
            r#"{"access_token":" at ","refresh_token":"","id_token":"  "}"#,
            "Codex token",
        )
        .unwrap();
        assert_eq!(tokens.access_token(), Some("at"));
        assert_eq!(tokens.refresh_token(), None);
        assert_eq!(tokens.id_token(), None);
    }
}

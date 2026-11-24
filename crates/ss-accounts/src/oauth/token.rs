//! Token endpoint exchange and refresh, with the error classification every
//! caller branches on.
//!
//! Standard form-grant exchanges go through `post_token`; a provider with a
//! non-standard token leg keeps its own function next to its login flow, but
//! classifies failures through the same table.
//!
//! Every request goes through `probe_http_client` so the user's proxy settings
//! apply. The three classifiers are contractual: `AuthRequired` clears the
//! stored credential, `Transient` keeps the last good state and retries later,
//! `Rejected` is the provider saying no to this particular request.

use std::time::Duration;

use serde_json::Value;

use crate::oauth::TokenSet;
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::rt;

/// Timeout for one token-endpoint round trip.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(20);

/// Provider error codes that mean "this credential is gone", not "retry later".
const AUTH_ERROR_CODES: [&str; 4] = [
    "invalid_grant",
    "invalid_token",
    "refresh_token_expired",
    "invalid_refresh_token",
];

/// A form-encoded application/x-www-form-urlencoded grant.
#[derive(Debug, Clone, Default)]
pub struct FormGrant {
    pub url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub refresh_token: Option<String>,
    pub code_verifier: Option<String>,
    pub extra: Vec<(String, String)>,
}

/// Whether the grant is an authorization-code exchange or a refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantKind {
    AuthorizationCode,
    RefreshToken,
}

impl GrantKind {
    fn grant_type(self) -> &'static str {
        match self {
            Self::AuthorizationCode => "authorization_code",
            Self::RefreshToken => "refresh_token",
        }
    }
}

/// How a provider spells the token response fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenFlavor {
    /// `access_token` / `refresh_token` / `expires_in`.
    Snake,
    /// `accessToken` / `refreshToken` (Cursor).
    Camel,
}

/// The form body of `grant`, in a fixed order, as pure data so a test can
/// assert the exact shape without touching the network.
pub fn form_pairs(grant: &FormGrant, kind: GrantKind) -> Vec<(String, String)> {
    let mut pairs = vec![("grant_type".to_string(), kind.grant_type().to_string())];
    if !grant.client_id.is_empty() {
        pairs.push(("client_id".to_string(), grant.client_id.clone()));
    }
    let optional = [
        ("client_secret", grant.client_secret.as_ref()),
        ("code", grant.code.as_ref()),
        ("redirect_uri", grant.redirect_uri.as_ref()),
        ("refresh_token", grant.refresh_token.as_ref()),
        ("code_verifier", grant.code_verifier.as_ref()),
    ];
    for (key, value) in optional {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            pairs.push((key.to_string(), value.clone()));
        }
    }
    pairs.extend(grant.extra.iter().cloned());
    pairs
}

/// POST a form grant and parse the token response.
pub fn post_token(grant: &FormGrant, kind: GrantKind) -> OAuthResult<TokenSet> {
    let pairs = form_pairs(grant, kind);
    let url = grant.url.clone();
    let value = rt::block_on(async move { post_form(&url, &pairs).await })??;
    tokens_from_json(&value, TokenFlavor::Snake)
}

/// POST a JSON body to a provider's token endpoint and read the tokens back.
pub(crate) fn post_json_tokens(
    url: &str,
    body: &Value,
    flavor: TokenFlavor,
) -> OAuthResult<TokenSet> {
    let value = rt::block_on(async { post_json(url, body).await })??;
    tokens_from_json(&value, flavor)
}

/// Read a token response. `access_token` is the only required field: a provider
/// that omits `refresh_token` keeps the caller's old one, and a provider that
/// omits `expires_in` simply never says when the token dies.
pub(crate) fn tokens_from_json(value: &Value, flavor: TokenFlavor) -> OAuthResult<TokenSet> {
    let keys: &[&str] = match flavor {
        TokenFlavor::Snake => &["access_token", "refresh_token", "id_token", "expires_in", "token_type"],
        TokenFlavor::Camel => &[
            "accessToken",
            "refreshToken",
            "idToken",
            "expiresIn",
            "tokenType",
        ],
    };
    let field = |index: usize| match flavor {
        TokenFlavor::Snake => string_field(value, &[keys[index]]),
        TokenFlavor::Camel => string_field(value, &[keys[index], snake_of(keys[index])]),
    };
    let access_token = field(0)
        .ok_or_else(|| OAuthError::Rejected("令牌响应缺少 access_token".to_string()))?;
    let expires_at = expires_in(value, keys[3]).map(|seconds| crate::oauth::now_secs() + seconds);
    Ok(TokenSet {
        access_token,
        refresh_token: field(1),
        id_token: field(2),
        expires_at,
        token_type: field(4).unwrap_or_else(|| "Bearer".to_string()),
    })
}

/// Keep the previous refresh token (and id token) when a refresh response does
/// not rotate them. `expires_at` comes from the new response only: an absent
/// `expires_in` means "the provider did not say", not "still valid forever".
pub(crate) fn carry_over(previous: &TokenSet, refreshed: TokenSet) -> TokenSet {
    TokenSet {
        access_token: refreshed.access_token,
        refresh_token: refreshed
            .refresh_token
            .or_else(|| previous.refresh_token.clone()),
        id_token: refreshed.id_token.or_else(|| previous.id_token.clone()),
        expires_at: refreshed.expires_at,
        token_type: if refreshed.token_type.is_empty() {
            previous.token_type.clone()
        } else {
            refreshed.token_type
        },
    }
}

/// Classify a non-2xx token-endpoint answer. 401 and the credential-gone error
/// codes mean the credential is gone; 429/5xx mean the provider is busy;
/// everything else is a rejection.
pub fn classify_status(status: u16, body: &str) -> OAuthError {
    let code = error_code(body);
    if status == 401
        || code
            .as_deref()
            .is_some_and(|code| AUTH_ERROR_CODES.contains(&code))
    {
        return OAuthError::AuthRequired(match code {
            Some(code) => format!("token endpoint 状态码 {status}: {code}"),
            None => format!("token endpoint 状态码 {status}"),
        });
    }
    if status == 429 || (500..600).contains(&status) {
        return OAuthError::Transient(format!("token endpoint 状态码 {status}"));
    }
    OAuthError::Rejected(format!(
        "token endpoint 状态码 {status}: {}",
        truncate(body)
    ))
}

async fn post_form(url: &str, pairs: &[(String, String)]) -> OAuthResult<Value> {
    let client = client(url)?;
    let response = client
        .post(url)
        .header("Accept", "application/json")
        .form(pairs)
        .send()
        .await
        .map_err(transport)?;
    read_token_response(response, "form").await
}

async fn post_json(url: &str, body: &Value) -> OAuthResult<Value> {
    let client = client(url)?;
    let response = client
        .post(url)
        .header("Accept", "application/json")
        .json(body)
        .send()
        .await
        .map_err(transport)?;
    read_token_response(response, "json").await
}

async fn read_token_response(response: reqwest::Response, kind: &str) -> OAuthResult<Value> {
    let status = response.status().as_u16();
    let body = response.text().await.map_err(transport)?;
    if !(200..300).contains(&status) {
        return Err(classify_status(status, &body));
    }
    serde_json::from_str(&body)
        .map_err(|error| OAuthError::Rejected(format!("{kind} 令牌响应不是合法 JSON: {error}")))
}

fn client(url: &str) -> OAuthResult<reqwest::Client> {
    ss_core::infra::http_client::probe_http_client_for_url(url, TOKEN_TIMEOUT)
        .map_err(|error| OAuthError::Other(format!("无法构建 HTTP 客户端: {error}")))
}

pub(crate) fn transport_error(prefix: &str, error: reqwest::Error) -> OAuthError {
    ss_core::infra::http_client::invalidate_probe_http_client();
    OAuthError::Transient(format!(
        "{prefix}: {}",
        ss_core::infra::http_client::format_error_chain(&error)
    ))
}

fn transport(error: reqwest::Error) -> OAuthError {
    transport_error("令牌端点请求失败", error)
}

fn error_code(body: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(body).ok()?;
    match value.get("error") {
        Some(Value::String(code)) => Some(code.clone()),
        Some(Value::Object(inner)) => inner
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn expires_in(value: &Value, key: &str) -> Option<i64> {
    let raw = value.get(key).or_else(|| value.get("expires_in"))?;
    raw.as_i64()
        .or_else(|| raw.as_f64().map(|seconds| seconds as i64))
        .or_else(|| raw.as_str().and_then(|text| text.trim().parse().ok()))
        .filter(|seconds| *seconds > 0)
}

fn snake_of(key: &str) -> &'static str {
    match key {
        "accessToken" => "access_token",
        "refreshToken" => "refresh_token",
        "idToken" => "id_token",
        "expiresIn" => "expires_in",
        _ => "token_type",
    }
}

fn truncate(body: &str) -> String {
    body.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refresh_grant() -> FormGrant {
        FormGrant {
            url: "https://example.invalid/oauth/token".to_string(),
            client_id: "client-1".to_string(),
            refresh_token: Some("refresh-1".to_string()),
            ..FormGrant::default()
        }
    }

    fn as_map(pairs: &[(String, String)]) -> std::collections::BTreeMap<&str, &str> {
        pairs
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect()
    }

    #[test]
    fn refresh_form_shape() {
        let pairs = form_pairs(&refresh_grant(), GrantKind::RefreshToken);
        let map = as_map(&pairs);
        assert_eq!(pairs.len(), 3, "refresh form is exactly grant_type/client_id/refresh_token");
        assert_eq!(map.get("grant_type"), Some(&"refresh_token"));
        assert_eq!(map.get("client_id"), Some(&"client-1"));
        assert_eq!(map.get("refresh_token"), Some(&"refresh-1"));
    }

    #[test]
    fn authorization_code_form_shape() {
        let grant = FormGrant {
            client_id: "client-1".to_string(),
            code: Some("code-1".to_string()),
            redirect_uri: Some("http://localhost:1455/auth/callback".to_string()),
            code_verifier: Some("verifier-1".to_string()),
            ..FormGrant::default()
        };
        let pairs = form_pairs(&grant, GrantKind::AuthorizationCode);
        let map = as_map(&pairs);
        assert_eq!(map.get("grant_type"), Some(&"authorization_code"));
        assert_eq!(map.get("code"), Some(&"code-1"));
        assert_eq!(
            map.get("redirect_uri"),
            Some(&"http://localhost:1455/auth/callback")
        );
        assert_eq!(map.get("code_verifier"), Some(&"verifier-1"));
        assert!(!map.contains_key("refresh_token"));
    }

    #[test]
    fn classification_table() {
        assert!(matches!(
            classify_status(401, "{}"),
            OAuthError::AuthRequired(_)
        ));
        for code in AUTH_ERROR_CODES {
            let body = format!("{{\"error\":\"{code}\"}}");
            assert!(
                matches!(classify_status(400, &body), OAuthError::AuthRequired(_)),
                "{code} must clear the credential"
            );
        }
        assert!(matches!(
            classify_status(429, ""),
            OAuthError::Transient(_)
        ));
        for status in [500, 502, 503, 504] {
            assert!(
                matches!(classify_status(status, ""), OAuthError::Transient(_)),
                "{status} must stay transient, never fail a login"
            );
        }
        for status in [400, 403, 404, 409] {
            assert!(
                matches!(classify_status(status, "{\"error\":\"invalid_request\"}"), OAuthError::Rejected(_)),
                "{status} is a rejection"
            );
        }
        assert!(classify_status(400, "not json").to_string().contains("not json"));
    }

    #[test]
    fn snake_and_camel_responses_parse() {
        let snake = serde_json::json!({
            "access_token": "a", "refresh_token": "r", "id_token": "i",
            "expires_in": 3600, "token_type": "Bearer"
        });
        let tokens = tokens_from_json(&snake, TokenFlavor::Snake).expect("snake");
        assert_eq!(tokens.access_token, "a");
        assert_eq!(tokens.refresh_token.as_deref(), Some("r"));
        assert_eq!(tokens.id_token.as_deref(), Some("i"));
        assert_eq!(tokens.token_type, "Bearer");
        assert!(tokens.expires_at.is_some());

        let camel = serde_json::json!({ "accessToken": "a2", "refreshToken": "r2" });
        let tokens = tokens_from_json(&camel, TokenFlavor::Camel).expect("camel");
        assert_eq!(tokens.access_token, "a2");
        assert_eq!(tokens.refresh_token.as_deref(), Some("r2"));
        assert_eq!(tokens.expires_at, None, "no expires_in means no expiry claim");
        assert_eq!(tokens.token_type, "Bearer");
    }

    #[test]
    fn missing_access_token_is_rejected() {
        let error = tokens_from_json(&serde_json::json!({ "refresh_token": "r" }), TokenFlavor::Snake)
            .expect_err("access_token is required");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
    }

    #[test]
    fn refresh_keeps_old_rotation_fields() {
        let previous = TokenSet {
            access_token: "old-access".to_string(),
            refresh_token: Some("old-refresh".to_string()),
            id_token: Some("old-id".to_string()),
            expires_at: Some(1),
            token_type: "Bearer".to_string(),
        };
        let fresh = TokenSet {
            access_token: "new-access".to_string(),
            expires_at: Some(9),
            ..TokenSet::default()
        };
        let merged = carry_over(&previous, fresh);
        assert_eq!(merged.access_token, "new-access");
        assert_eq!(merged.refresh_token.as_deref(), Some("old-refresh"));
        assert_eq!(merged.id_token.as_deref(), Some("old-id"));
        assert_eq!(merged.expires_at, Some(9));
        assert_eq!(merged.token_type, "Bearer");

        let rotated = TokenSet {
            access_token: "newer".to_string(),
            refresh_token: Some("new-refresh".to_string()),
            id_token: Some("new-id".to_string()),
            ..TokenSet::default()
        };
        let merged = carry_over(&previous, rotated);
        assert_eq!(merged.refresh_token.as_deref(), Some("new-refresh"));
        assert_eq!(merged.id_token.as_deref(), Some("new-id"));
        assert_eq!(merged.expires_at, None, "absent expires_in is not the old expiry");
    }
}

//! Browser login for Windsurf.
//!
//! Implicit grant (`response_type=token`) on `www.windsurf.com/windsurf/signin`.
//! Cockpit has no `windsurf://` scheme, so this stays
//! [`super::super::OAuthFlow::LocalCallback`]. `redirect_parameters_type=query`
//! puts `access_token` on the loopback query. The shared listener only accepts
//! `code`, so the callback socket lives here. A pasted fragment is already
//! merged into that query by `oauth::manual_callback`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiny_http::{Header, Response, Server};
use url::Url;

use super::quota::{self, seat_call};
use super::{AUTH_BASE, CALLBACK_PATH, CATALOG_ID, CLIENT_ID, REGISTER_BASE, WindsurfState};
use crate::fetchers::oauth::common::{carry_over_user_metadata, reauth_target};
use crate::subscription::Subscription;
use crate::{UsageError, UsageResult};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImplicitGrant {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExchangedLogin {
    pub api_key: String,
    pub api_server_url: String,
    pub auth_token: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
}

pub(crate) async fn start_login(
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::super::OAuthStartInfo> {
    let state = crate::oauth::pkce::random_state();
    let server = Server::http("127.0.0.1:0")
        .map_err(|err| UsageError::Other(format!("无法监听 Windsurf 回调: {err}")))?;
    let port = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| UsageError::Other("Windsurf 回调没有 TCP 端口".into()))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
    let auth_url = build_auth_url(&redirect_uri, &state);

    let pending_id = crate::oauth::pending_state::register_with_callback_port(
        CATALOG_ID,
        None,
        auth_url.clone(),
        Some(port),
    );
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );

    let pid = pending_id.clone();
    let server = Arc::new(server);
    tokio::spawn(async move {
        let target = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(server, state, target).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::super::OAuthStartInfo::browser(auth_url, pending_id))
}

pub(crate) fn build_auth_url(redirect_uri: &str, state: &str) -> String {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("response_type", "token");
    params.append_pair("client_id", CLIENT_ID);
    params.append_pair("redirect_uri", redirect_uri);
    params.append_pair("state", state);
    params.append_pair("prompt", "login");
    params.append_pair("redirect_parameters_type", "query");
    params.append_pair("workflow", "onboarding");
    format!("{AUTH_BASE}/windsurf/signin?{}", params.finish())
}

async fn drive_login(
    server: Arc<Server>,
    state: String,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let grant = listen_for_grant(server, &state, LOGIN_TIMEOUT).await?;
    let exchanged = exchange_firebase_token(&grant.access_token, REGISTER_BASE).await?;
    // `expires_in` is not stored: there is no refresh exchange, and a past
    // timestamp would look like a dead session on the next fetch.
    let _ = grant.expires_in;
    let imported = exchanged_to_imported(&exchanged, grant.refresh_token);
    let mut sub = super::import::oauth_row_from_imported(imported)?;
    if let Some(existing) = reauth_target(CATALOG_ID, target_subscription_id.as_deref()) {
        carry_over_user_metadata(&mut sub, &existing, &["Windsurf"]);
    }
    if let Ok(usage) = quota::fetch_quota(&mut sub).await {
        crate::storage::save_usage_snapshot(usage).ok();
    }
    crate::storage::upsert_subscription(sub)
        .map_err(|err| UsageError::Other(format!("Windsurf 订阅保存失败：{err}")))
}

pub(crate) fn exchanged_to_imported(
    exchanged: &ExchangedLogin,
    refresh_token: Option<String>,
) -> crate::token_import::ImportedToken {
    let state = WindsurfState {
        api_key: Some(exchanged.api_key.clone()),
        api_server_url: Some(exchanged.api_server_url.clone()),
        auth1_token: None,
    };
    let email = exchanged
        .email
        .clone()
        .filter(|value| crate::fetchers::oauth::common::looks_like_email(value));
    crate::token_import::ImportedToken {
        display_name: email
            .clone()
            .or_else(|| exchanged.name.clone())
            .unwrap_or_else(|| "Windsurf".to_string()),
        access_token: exchanged.auth_token.clone().unwrap_or_default(),
        refresh_token,
        expires_at: None,
        oauth_account_id: email,
        provider_state: state.to_json(),
        currency: None,
        oauth_region: None,
    }
}

pub(crate) async fn exchange_firebase_token(
    id_token: &str,
    register_base: &str,
) -> UsageResult<ExchangedLogin> {
    let registered = seat_call(
        register_base,
        "RegisterUser",
        json!({ "firebase_id_token": id_token }),
    )
    .await?;
    let api_key = super::pick_string(Some(&registered), &["apiKey", "api_key"])
        .ok_or_else(|| UsageError::Fetcher("Windsurf RegisterUser 响应缺少 apiKey".into()))?;
    let api_server_url = super::pick_string(Some(&registered), &["apiServerUrl", "api_server_url"])
        .unwrap_or_else(|| super::DEFAULT_API_SERVER.to_string());
    let name = super::pick_string(Some(&registered), &["name"]);

    let auth_token = match seat_call(
        &api_server_url,
        "GetOneTimeAuthToken",
        json!({ "firebaseIdToken": id_token }),
    )
    .await
    {
        Ok(value) => super::pick_string(Some(&value), &["authToken", "auth_token"]),
        Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
        Err(_) => None,
    };

    let email = if let Some(token) = auth_token.as_deref() {
        match seat_call(
            &api_server_url,
            "GetCurrentUser",
            json!({ "authToken": token, "includeSubscription": true }),
        )
        .await
        {
            Ok(user) => email_of(&user),
            Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(ExchangedLogin {
        api_key,
        api_server_url,
        auth_token,
        email,
        name,
    })
}

fn email_of(value: &Value) -> Option<String> {
    super::pick_string(value.get("user"), &["email"])
        .or_else(|| super::pick_string(Some(value), &["email"]))
        .filter(|email| crate::fetchers::oauth::common::looks_like_email(email))
}

pub(crate) async fn listen_for_grant(
    server: Arc<Server>,
    expected_state: &str,
    timeout: Duration,
) -> UsageResult<ImplicitGrant> {
    let expected_state = expected_state.to_string();
    let server_for_thread = Arc::clone(&server);
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + timeout;
        let mut tx = Some(tx);
        loop {
            if tx.is_none() {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(Err(UsageError::Other("Windsurf 登录超时".into())));
                }
                break;
            }
            match server_for_thread.recv_timeout(remaining.min(Duration::from_millis(200))) {
                Ok(Some(request)) => {
                    let url = request.url().to_string();
                    let outcome = outcome_for(&url, &expected_state);
                    respond(request, &outcome);
                    match outcome {
                        Outcome::Grant(grant) => {
                            if let Some(tx) = tx.take() {
                                let _ = tx.send(Ok(grant));
                            }
                            break;
                        }
                        Outcome::Cancelled => {
                            if let Some(tx) = tx.take() {
                                let _ = tx.send(Err(UsageError::Other("用户取消登录".into())));
                            }
                            break;
                        }
                        Outcome::Denied(error) => {
                            if let Some(tx) = tx.take() {
                                let _ = tx.send(Err(error));
                            }
                            break;
                        }
                        Outcome::Failed | Outcome::Ignored => {}
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(Err(UsageError::Other(format!(
                            "Windsurf 回调监听失败: {err}"
                        ))));
                    }
                    break;
                }
            }
        }
        server_for_thread.unblock();
    });
    rx.await
        .map_err(|_| UsageError::Other("Windsurf 回调监听已退出".into()))?
}

enum Outcome {
    Grant(ImplicitGrant),
    Cancelled,
    Denied(UsageError),
    Failed,
    Ignored,
}

fn outcome_for(url: &str, expected_state: &str) -> Outcome {
    let path = url.split('?').next().unwrap_or(url);
    if path == "/cancel" {
        return Outcome::Cancelled;
    }
    if !url.contains('?') {
        return Outcome::Ignored;
    }
    match parse_implicit_callback(url, expected_state) {
        Ok(grant) => Outcome::Grant(grant),
        Err(UsageError::AuthRequired) => Outcome::Denied(UsageError::AuthRequired),
        Err(error) if error.to_string().contains("state 不匹配") => Outcome::Failed,
        Err(error) if error.to_string().contains("缺少 access_token") => Outcome::Failed,
        Err(error) => Outcome::Denied(error),
    }
}

fn respond(request: tiny_http::Request, outcome: &Outcome) {
    let (status, body): (u16, &str) = match outcome {
        Outcome::Grant(_) => (200, "ok"),
        Outcome::Cancelled => (200, "cancelled"),
        Outcome::Denied(_) | Outcome::Failed => (400, "fail"),
        Outcome::Ignored => (404, "missing"),
    };
    let response = Response::from_string(body)
        .with_status_code(status)
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..]).unwrap(),
        );
    let _ = request.respond(response);
}

/// Query or fragment. Fragment keys override the query, matching a pasted
/// implicit redirect after `manual_callback` merges `#...` into the query.
pub(crate) fn parse_implicit_callback(
    raw: &str,
    expected_state: &str,
) -> UsageResult<ImplicitGrant> {
    let params = callback_params(raw)?;
    if let Some(error) = params
        .get("error")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if error.eq_ignore_ascii_case("invalid_grant") {
            return Err(UsageError::AuthRequired);
        }
        let description = params
            .get("error_description")
            .map(|value| value.trim())
            .unwrap_or("");
        let message = if description.is_empty() {
            format!("Windsurf 授权失败: {error}")
        } else {
            format!("Windsurf 授权失败: {error} ({description})")
        };
        return Err(UsageError::Other(message));
    }

    let state = params.get("state").map(|value| value.trim()).unwrap_or("");
    if state.is_empty() {
        return Err(UsageError::Other("Windsurf 回调缺少 state".into()));
    }
    if state != expected_state {
        return Err(UsageError::Other("Windsurf 回调 state 不匹配".into()));
    }
    let access_token = params
        .get("access_token")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UsageError::Other("Windsurf 回调缺少 access_token".into()))?
        .to_string();
    let refresh_token = params
        .get("refresh_token")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let expires_in = params
        .get("expires_in")
        .and_then(|value| value.parse().ok());
    Ok(ImplicitGrant {
        access_token,
        refresh_token,
        expires_in,
    })
}

fn callback_params(raw: &str) -> UsageResult<HashMap<String, String>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(UsageError::Other("Windsurf 回调为空".into()));
    }
    let (query, fragment) = if raw.contains("://") || raw.starts_with('/') {
        let parsed = if raw.starts_with('/') {
            Url::parse(&format!("http://127.0.0.1{raw}"))
        } else {
            Url::parse(raw)
        }
        .map_err(|err| UsageError::Other(format!("Windsurf 回调链接无效: {err}")))?;
        (
            parsed.query().unwrap_or("").to_string(),
            parsed.fragment().unwrap_or("").to_string(),
        )
    } else {
        let bare = raw.trim_start_matches(['?', '#']);
        if !bare.contains('=') {
            return Err(UsageError::Other("Windsurf 回调缺少参数".into()));
        }
        (bare.to_string(), String::new())
    };

    let mut params = HashMap::new();
    extend_params(&mut params, &query);
    extend_params(&mut params, &fragment);
    Ok(params)
}

fn extend_params(params: &mut HashMap<String, String>, raw: &str) {
    if raw.is_empty() {
        return;
    }
    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        params.insert(key.into_owned(), value.into_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn authorize_url_is_a_local_implicit_grant() {
        let url = build_auth_url("http://127.0.0.1:9/windsurf-auth-callback", "state-1");
        let parsed = Url::parse(&url).unwrap();
        assert_eq!(parsed.path(), "/windsurf/signin");
        assert!(!url.contains("windsurf://"));
        let pairs: HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(
            pairs.get("response_type").map(String::as_str),
            Some("token")
        );
        assert_eq!(pairs.get("client_id").map(String::as_str), Some(CLIENT_ID));
        assert_eq!(
            pairs.get("redirect_parameters_type").map(String::as_str),
            Some("query")
        );
        assert_eq!(
            pairs.get("redirect_uri").map(String::as_str),
            Some("http://127.0.0.1:9/windsurf-auth-callback")
        );
        assert_eq!(pairs.get("state").map(String::as_str), Some("state-1"));
    }

    #[test]
    fn fragment_and_query_callbacks_yield_the_access_token() {
        let fragment = parse_implicit_callback(
            "http://localhost:3333/windsurf-auth-callback#access_token=tok%20en&state=s3&refresh_token=ref",
            "s3",
        )
        .unwrap();
        assert_eq!(fragment.access_token, "tok en");
        assert_eq!(fragment.refresh_token.as_deref(), Some("ref"));

        let query = parse_implicit_callback(
            "/windsurf-auth-callback?access_token=session&state=s3&expires_in=3600",
            "s3",
        )
        .unwrap();
        assert_eq!(query.access_token, "session");
        assert_eq!(query.expires_in, Some(3600));
    }

    #[test]
    fn bad_callback_state_and_invalid_grant_are_classified() {
        let mismatch = parse_implicit_callback(
            "http://127.0.0.1:9/windsurf-auth-callback?access_token=tok&state=nope",
            "s3",
        )
        .unwrap_err();
        assert!(mismatch.to_string().contains("state 不匹配"));

        let revoked = parse_implicit_callback(
            "http://127.0.0.1:9/windsurf-auth-callback?error=invalid_grant&state=s3",
            "s3",
        )
        .unwrap_err();
        assert!(matches!(revoked, UsageError::AuthRequired));
    }

    #[tokio::test]
    async fn start_login_reports_local_callback_and_cancels() {
        let info = start_login(None, None).await.expect("start");
        assert_eq!(info.flow, crate::fetchers::oauth::OAuthFlow::LocalCallback);
        assert!(info.auth_url.contains("response_type=token"));
        assert!(!info.auth_url.contains("windsurf://"));
        let parsed = Url::parse(&info.auth_url).unwrap();
        let redirect = parsed
            .query_pairs()
            .find_map(|(key, value)| (key == "redirect_uri").then(|| value.into_owned()))
            .unwrap();
        let port: u16 = Url::parse(&redirect).unwrap().port().unwrap();
        crate::oauth::local_server::request_cancel(port).expect("cancel listener");
        tokio::time::sleep(Duration::from_millis(400)).await;
        crate::oauth::pending_state::remove(&info.pending_id);
    }

    #[tokio::test]
    async fn loopback_accepts_an_implicit_query() {
        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let wait = listen_for_grant(server, "s3", Duration::from_secs(5));
        let client = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            reqwest::get(format!(
                "http://127.0.0.1:{port}/windsurf-auth-callback?access_token=firebase-id&state=s3&refresh_token=firebase-refresh"
            ))
            .await
            .unwrap()
        });
        let grant = wait.await.expect("grant");
        client.await.unwrap();
        assert_eq!(grant.access_token, "firebase-id");
        assert_eq!(grant.refresh_token.as_deref(), Some("firebase-refresh"));
    }

    #[tokio::test]
    async fn firebase_exchange_stores_api_key_and_session() {
        let mock = super::super::test_support::MockSeat::start(|base, url| {
            if url.contains("RegisterUser") {
                (
                    200,
                    format!(
                        r#"{{"apiKey":"sk-ws-testkey12","apiServerUrl":"{base}","name":"Ada"}}"#
                    ),
                )
            } else if url.contains("GetOneTimeAuthToken") {
                (200, r#"{"authToken":"session-token"}"#.into())
            } else if url.contains("GetCurrentUser") {
                (200, r#"{"user":{"email":"ada@wind.dev"}}"#.into())
            } else {
                (404, url.into())
            }
        });

        let exchanged = exchange_firebase_token("firebase-id", &mock.base)
            .await
            .expect("exchange");
        assert_eq!(exchanged.api_key, "sk-ws-testkey12");
        assert_eq!(exchanged.api_server_url, mock.base);
        assert_eq!(exchanged.auth_token.as_deref(), Some("session-token"));
        assert_eq!(exchanged.email.as_deref(), Some("ada@wind.dev"));

        let imported = exchanged_to_imported(&exchanged, Some("firebase-refresh".into()));
        assert_eq!(imported.access_token, "session-token");
        assert_eq!(imported.refresh_token.as_deref(), Some("firebase-refresh"));
        assert_eq!(imported.oauth_account_id.as_deref(), Some("ada@wind.dev"));
        let state = WindsurfState::parse(imported.provider_state.as_deref().unwrap());
        assert_eq!(state.api_key.as_deref(), Some("sk-ws-testkey12"));
        assert_eq!(state.api_server_url.as_deref(), Some(mock.base.as_str()));
    }

    #[tokio::test]
    async fn one_time_token_failure_keeps_the_api_key() {
        let mock = super::super::test_support::MockSeat::start(|base, url| {
            if url.contains("RegisterUser") {
                (
                    200,
                    format!(r#"{{"apiKey":"sk-ws-testkey12","apiServerUrl":"{base}"}}"#),
                )
            } else if url.contains("GetOneTimeAuthToken") {
                (503, "unavailable".into())
            } else {
                (404, url.into())
            }
        });
        let exchanged = exchange_firebase_token("firebase-id", &mock.base)
            .await
            .expect("degraded exchange");
        assert_eq!(exchanged.api_key, "sk-ws-testkey12");
        assert!(exchanged.auth_token.is_none());
    }

    #[tokio::test]
    async fn register_user_401_is_auth_required() {
        let mock = super::super::test_support::MockSeat::start(|_base, _url| (401, "nope".into()));
        let error = exchange_firebase_token("firebase-id", &mock.base)
            .await
            .expect_err("401");
        assert!(matches!(error, UsageError::AuthRequired), "{error:?}");
    }
}

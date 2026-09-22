//! Builder ID / portal login: PKCE + loopback callback.
//!
//! The authorize URL is `app.kiro.dev/signin`. The token POST is JSON to
//! `prod.us-east-1.auth.desktop.kiro.dev/oauth/token`. Only
//! `/oauth/callback` and `/signin/callback` are accepted. `loginOption`
//! `builderid` / `awsidc` / `internal` without a code is a hard error —
//! those legs need the IDC device flow, not this callback.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::json;
use tiny_http::{Request, Response, Server};

use super::http::{self, TokenGrant};
use super::{CATALOG_ID, PORTAL_ORIGIN, PORTAL_REFRESH_URL, PORTAL_TOKEN_URL, resolve_region};
use crate::oauth::pkce::PkcePair;
use crate::{UsageError, UsageResult};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const CALLBACK_PATHS: [&str; 2] = ["/oauth/callback", "/signin/callback"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PortalCallback {
    pub path: String,
    pub login_option: String,
    pub code: String,
    pub region: Option<String>,
    pub issuer_url: Option<String>,
    pub client_id: Option<String>,
    pub login_hint: Option<String>,
}

pub(super) enum CallbackDecision {
    Ignore,
    Cancelled,
    Failed(String),
    Ready(PortalCallback),
}

pub(super) async fn start_portal(
    target_subscription_id: Option<&str>,
) -> UsageResult<super::super::OAuthStartInfo> {
    let pkce = PkcePair::generate();
    let state = crate::oauth::pkce::random_state();
    let server = Server::http("127.0.0.1:0")
        .map_err(|err| UsageError::Other(format!("无法监听 Kiro 回调: {err}")))?;
    let port = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| UsageError::Other("Kiro 回调没有 TCP 端口".into()))?
        .port();
    let redirect_base = format!("http://localhost:{port}");
    let auth_url = build_portal_auth_url(&redirect_base, &state, &pkce.challenge);

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
    let verifier = pkce.verifier;
    tokio::spawn(async move {
        let target = crate::oauth::pending_state::target_subscription_id(&pid);
        let result =
            drive_portal(server, state, verifier, redirect_base, pid.clone(), target).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::super::OAuthStartInfo::browser(auth_url, pending_id))
}

pub(super) fn build_portal_auth_url(redirect_uri: &str, state: &str, challenge: &str) -> String {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("state", state);
    params.append_pair("code_challenge", challenge);
    params.append_pair("code_challenge_method", "S256");
    params.append_pair("redirect_uri", redirect_uri);
    params.append_pair("redirect_from", "KiroIDE");
    format!("{PORTAL_ORIGIN}?{}", params.finish())
}

pub(super) fn token_redirect_uri(base: &str, path: &str, login_option: &str) -> String {
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let option: String = url::form_urlencoded::byte_serialize(login_option.as_bytes()).collect();
    format!("{}{path}?login_option={option}", base.trim_end_matches('/'))
}

pub(super) async fn exchange_code(
    client: &reqwest::Client,
    token_url: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> UsageResult<TokenGrant> {
    http::post_json_token(
        client,
        token_url,
        &json!({
            "code": code,
            "code_verifier": verifier,
            "redirect_uri": redirect_uri,
        }),
        "Kiro token",
    )
    .await
}

pub(super) async fn refresh_portal_token(
    client: &reqwest::Client,
    refresh_token: &str,
) -> UsageResult<TokenGrant> {
    http::post_json_token(
        client,
        PORTAL_REFRESH_URL,
        &json!({ "refreshToken": refresh_token }),
        "Kiro refresh",
    )
    .await
}

pub(super) fn decide_callback(url: &str, expected_state: &str) -> CallbackDecision {
    let path = url.split('?').next().unwrap_or(url);
    if path == "/cancel" {
        return CallbackDecision::Cancelled;
    }
    if !CALLBACK_PATHS.contains(&path) {
        return CallbackDecision::Ignore;
    }
    let query = url.split_once('?').map(|(_, query)| query).unwrap_or("");
    let params = parse_query(query);
    if let Some(error) = nonempty(params.get("error")) {
        let description = nonempty(params.get("error_description")).unwrap_or("");
        let message = if description.is_empty() {
            format!("授权失败: {error}")
        } else {
            format!("授权失败: {error} ({description})")
        };
        return CallbackDecision::Failed(message);
    }
    let state = nonempty(params.get("state")).unwrap_or("");
    if state.is_empty() || state != expected_state {
        return CallbackDecision::Failed("授权状态校验失败，请重新发起登录".into());
    }
    let login_option = nonempty(
        params
            .get("login_option")
            .or_else(|| params.get("loginOption")),
    )
    .unwrap_or("")
    .to_ascii_lowercase();
    let Some(code) = nonempty(params.get("code")).map(str::to_string) else {
        return CallbackDecision::Failed(missing_code_message(&login_option).to_string());
    };
    CallbackDecision::Ready(PortalCallback {
        path: path.to_string(),
        login_option,
        code,
        region: nonempty(params.get("idc_region").or_else(|| params.get("idcRegion")))
            .map(str::to_string),
        issuer_url: nonempty(params.get("issuer_url").or_else(|| params.get("issuerUrl")))
            .map(str::to_string),
        client_id: nonempty(params.get("client_id").or_else(|| params.get("clientId")))
            .map(str::to_string),
        login_hint: nonempty(params.get("login_hint").or_else(|| params.get("loginHint")))
            .map(str::to_string),
    })
}

pub(super) fn missing_code_message(login_option: &str) -> &'static str {
    match login_option.trim().to_ascii_lowercase().as_str() {
        "builderid" | "awsidc" | "internal" => {
            "当前登录方式需要 Kiro 客户端后续认证流程，暂不支持直接导入，请改用 Google/GitHub 登录。"
        }
        "external_idp" => "当前登录方式为 External IdP，未返回授权 code，暂不支持自动导入。",
        _ => "回调缺少授权 code，无法完成登录。",
    }
}

async fn drive_portal(
    server: Server,
    state: String,
    verifier: String,
    redirect_base: String,
    pending_id: String,
    target_subscription_id: Option<String>,
) -> UsageResult<crate::subscription::Subscription> {
    let callback = wait_portal(server, &state, Some(pending_id), LOGIN_TIMEOUT).await?;
    let redirect = token_redirect_uri(&redirect_base, &callback.path, &callback.login_option);
    let client = crate::fetchers::http_client()?;
    let grant = exchange_code(
        &client,
        PORTAL_TOKEN_URL,
        &callback.code,
        &verifier,
        &redirect,
    )
    .await?;
    let mut kiro_state = super::KiroState {
        client_id: callback.client_id.clone(),
        region: Some(resolve_region(
            callback.region.as_deref(),
            super::string_field(Some(&grant.raw), &["profileArn", "profile_arn", "arn"]).as_deref(),
        )),
        start_url: callback.issuer_url.clone(),
        ..super::KiroState::default()
    };
    kiro_state.overlay_token(&grant.raw);
    if kiro_state.region.is_none() {
        kiro_state.region = Some(resolve_region(None, kiro_state.profile_arn.as_deref()));
    }
    let email = callback
        .login_hint
        .as_deref()
        .filter(|value| crate::fetchers::oauth::common::looks_like_email(value))
        .map(str::to_string);
    super::import::finish_login(grant, kiro_state, email, target_subscription_id.as_deref()).await
}

pub(super) async fn wait_portal(
    server: Server,
    expected_state: &str,
    pending_id: Option<String>,
    timeout: Duration,
) -> UsageResult<PortalCallback> {
    let expected_state = expected_state.to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let join = tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + timeout;
        let mut tx = Some(tx);
        loop {
            if tx.is_none() {
                break;
            }
            if pending_id
                .as_deref()
                .is_some_and(|id| crate::oauth::pending_state::flow(id).is_none())
            {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(Err(UsageError::Other("用户取消登录".into())));
                }
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                if let Some(tx) = tx.take() {
                    let _ = tx.send(Err(UsageError::Other(
                        "等待 Kiro 登录超时，请重新发起授权".into(),
                    )));
                }
                break;
            }
            match server.recv_timeout(remaining.min(Duration::from_millis(200))) {
                Ok(Some(request)) => {
                    let url = request.url().to_string();
                    let decision = decide_callback(&url, &expected_state);
                    respond(request, &decision);
                    match decision {
                        CallbackDecision::Ignore => {}
                        CallbackDecision::Cancelled => {
                            if let Some(tx) = tx.take() {
                                let _ = tx.send(Err(UsageError::Other("用户取消登录".into())));
                            }
                            break;
                        }
                        CallbackDecision::Failed(message) => {
                            if let Some(tx) = tx.take() {
                                let _ = tx.send(Err(UsageError::Other(message)));
                            }
                            break;
                        }
                        CallbackDecision::Ready(callback) => {
                            if let Some(tx) = tx.take() {
                                let _ = tx.send(Ok(callback));
                            }
                            break;
                        }
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    if let Some(tx) = tx.take() {
                        let _ =
                            tx.send(Err(UsageError::Other(format!("Kiro 回调监听失败: {err}"))));
                    }
                    break;
                }
            }
        }
        server.unblock();
    });
    let result = rx
        .await
        .map_err(|_| UsageError::Other("Kiro 回调监听已结束".into()))?;
    let _ = join.await;
    result
}

fn respond(request: Request, decision: &CallbackDecision) {
    let (status, body): (u16, &str) = match decision {
        CallbackDecision::Ready(_) => (200, "ok"),
        CallbackDecision::Cancelled => (200, "cancelled"),
        CallbackDecision::Failed(_) => (400, "failed"),
        CallbackDecision::Ignore => (404, "not found"),
    };
    let _ = request.respond(Response::from_string(body).with_status_code(status));
}

fn parse_query(query: &str) -> HashMap<String, String> {
    url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn nonempty(value: Option<&String>) -> Option<&str> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiro_callback_without_code_names_the_idc_login_options() {
        for option in ["builderid", "awsidc", "internal"] {
            let url = format!("/oauth/callback?loginOption={option}&state=s");
            match decide_callback(&url, "s") {
                CallbackDecision::Failed(message) => {
                    assert!(
                        message.contains("Kiro 客户端后续认证流程"),
                        "{option}: {message}"
                    );
                }
                CallbackDecision::Ignore
                | CallbackDecision::Cancelled
                | CallbackDecision::Ready(_) => {
                    panic!("{option} should fail");
                }
            }
        }
        match decide_callback("/signin/callback?login_option=external_idp&state=s", "s") {
            CallbackDecision::Failed(message) => {
                assert!(message.contains("External IdP"), "{message}")
            }
            _ => panic!("external idp"),
        }
        match decide_callback("/oauth/callback?loginOption=google&state=s", "s") {
            CallbackDecision::Failed(message) => {
                assert!(message.contains("缺少授权 code"), "{message}")
            }
            _ => panic!("google"),
        }
    }

    #[test]
    fn kiro_callback_accepts_only_the_two_paths_and_checks_state() {
        assert!(matches!(
            decide_callback("/favicon.ico", "s"),
            CallbackDecision::Ignore
        ));
        assert!(matches!(
            decide_callback("/cancel", "s"),
            CallbackDecision::Cancelled
        ));
        match decide_callback("/oauth/callback?code=abc&state=nope", "s") {
            CallbackDecision::Failed(message) => {
                assert!(message.contains("授权状态校验失败"), "{message}")
            }
            _ => panic!("state"),
        }
        match decide_callback(
            "/signin/callback?code=abc&state=s&loginOption=Github&idcRegion=eu-central-1",
            "s",
        ) {
            CallbackDecision::Ready(callback) => {
                assert_eq!(callback.code, "abc");
                assert_eq!(callback.path, "/signin/callback");
                assert_eq!(callback.login_option, "github");
                assert_eq!(callback.region.as_deref(), Some("eu-central-1"));
            }
            _ => panic!("ready"),
        }
    }

    #[test]
    fn kiro_portal_auth_url_is_pkce_on_the_signin_page() {
        let url = build_portal_auth_url("http://localhost:9", "state-1", "challenge");
        assert!(url.starts_with("https://app.kiro.dev/signin?"), "{url}");
        assert!(url.contains("code_challenge_method=S256"), "{url}");
        assert!(url.contains("redirect_from=KiroIDE"), "{url}");
        assert!(url.contains("state=state-1"), "{url}");
        let redirect = token_redirect_uri("http://localhost:9", "/oauth/callback", "google");
        assert_eq!(
            redirect,
            "http://localhost:9/oauth/callback?login_option=google"
        );
    }

    #[tokio::test]
    async fn kiro_builder_id_callback_without_code_ends_the_listener() {
        let server = Server::http("127.0.0.1:0").expect("bind");
        let port = server.server_addr().to_ip().expect("tcp").port();
        let wait = tokio::spawn(async move {
            wait_portal(server, "state-1", None, Duration::from_secs(5)).await
        });
        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "http://127.0.0.1:{port}/oauth/callback?loginOption=builderid&state=state-1"
            ))
            .send()
            .await
            .expect("callback");
        assert_eq!(response.status(), 400);
        let err = wait.await.expect("join").expect_err("missing code");
        assert!(err.to_string().contains("Kiro 客户端后续认证流程"), "{err}");
    }

    #[tokio::test]
    async fn kiro_portal_login_is_local_callback() {
        let info = start_portal(None).await.expect("start");
        assert_eq!(info.flow, super::super::super::OAuthFlow::LocalCallback);
        assert!(info.user_code.is_none());
        assert!(
            info.auth_url.contains("https://app.kiro.dev/signin?"),
            "{}",
            info.auth_url
        );
        assert!(info.auth_url.contains("code_challenge_method=S256"));
        crate::oauth::pending_state::cancel(&info.pending_id).expect("cancel");
    }

    #[tokio::test]
    async fn kiro_portal_token_post_classifies_errors() {
        let server = super::super::http::scripted::ScriptedHttp::start(vec![(
            400,
            r#"{"error":"invalid_grant"}"#.into(),
        )]);
        let client = reqwest::Client::new();
        let url = format!("{}/oauth/token", server.base);
        let err = exchange_code(&client, &url, "code", "verifier", "http://localhost/cb")
            .await
            .expect_err("revoked");
        assert!(matches!(err, UsageError::AuthRequired), "{err:?}");
        assert!(
            server.seen()[0].contains("POST /oauth/token"),
            "{:?}",
            server.seen()
        );
    }
}

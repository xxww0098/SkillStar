//! Antigravity（Google IDE）登录：Google OAuth 授权码 + 本机回调换票。
//!
//! 和 Codex 的差别有两处，都来自"照 Cockpit Tools 复刻这家 agent"这件事：
//! 授权 URL 不带 PKCE（Google 的安装型 client 不需要 code_challenge），回调
//! 路径是客户端注册好的 `/oauth-callback`、端口优先 51121。换票与刷新都带
//! native OAuth 的 UA，Google 侧按 UA 判断请求来源。
//!
//! 身份只来自 userinfo：email 是账号身份，必取；显示名 name → given+family。

use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;
use url::Url;

use crate::oauth::agents::antigravity_config::{self, ClientConfig};
use crate::oauth::agents::{Started, code_from_query};
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::loopback::{CALLBACK_TIMEOUT, LoopbackServer, QueryMap};
use crate::oauth::session::{CancelSignal, LoginFlow, LoginStart, PendingLogin, StartOptions};
use crate::oauth::token::{self, TokenFlavor};
use crate::oauth::{Identity, LoginOutcome, TokenSet, rt};

pub(crate) const AGENT_ID: &str = "antigravity";

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const USERINFO_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
/// Antigravity 桌面客户端注册的回调路径。
const CALLBACK_PATH: &str = "/oauth-callback";
/// 它注册的端口；占用时退回随机端口（redirect_uri 跟着实际监听端口走）。
const FIXED_PORTS: [u16; 1] = [51121];

/// Google 要的 7 条 scope，空格分隔，顺序与 Cockpit Tools 一致。
const SCOPES: &str = concat!(
    "openid ",
    "https://www.googleapis.com/auth/cloud-platform ",
    "https://www.googleapis.com/auth/userinfo.email ",
    "https://www.googleapis.com/auth/userinfo.profile ",
    "https://www.googleapis.com/auth/cclog ",
    "https://www.googleapis.com/auth/experimentsandconfigs ",
    "https://www.googleapis.com/auth/aicode",
);

/// native OAuth 客户端 UA：不做版本探测，Authorization 路径上不发现远端。
const NATIVE_USER_AGENT: &str = "vscode/1.X.X (Antigravity/4.3.0)";

/// 提前 15 分钟刷新：Google 的 access token 按小时计，留够一次额度读取。
pub(crate) const REFRESH_SKEW_SECONDS: i64 = 900;

/// Google 在并发刷新时会短暂回 invalid_grant，退避后确认一次再判失效。
const INVALID_GRANT_BACKOFF: Duration = Duration::from_millis(500);

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// 启动一次 Antigravity 登录：绑定本机回调，等待 Google 把授权码送回来。
pub(crate) fn start(login_id: &str, opts: &StartOptions) -> OAuthResult<Started> {
    start_with(login_id, opts, antigravity_config::client_config())
}

/// start 的配置接缝：secret 缺失时在打开浏览器之前就报错。
fn start_with(login_id: &str, opts: &StartOptions, config: ClientConfig) -> OAuthResult<Started> {
    config.require_secret()?;
    let state = uuid::Uuid::new_v4().to_string();
    let server = LoopbackServer::bind_with(&ports(opts), CALLBACK_PATH)?;
    let redirect_uri = server.redirect_uri();
    let auth_url = authorize_url(&config.client_id, &redirect_uri, &state);

    let (callback_tx, callback_rx) = mpsc::channel();
    let (pending, channel) = PendingLogin::pair(Some(callback_tx));
    std::thread::spawn(move || {
        let outcome = receive_and_exchange(
            server,
            callback_rx,
            &state,
            &redirect_uri,
            &config,
            &channel.cancel,
        );
        let _ = channel.results.send(outcome);
    });

    Ok(Started {
        start: LoginStart {
            login_id: login_id.to_string(),
            flow: LoginFlow::LocalCallback,
            auth_url,
            user_code: None,
            verification_uri: None,
            interval_secs: None,
            expires_at: Some(crate::oauth::now_secs() + CALLBACK_TIMEOUT.as_secs() as i64),
        },
        pending,
    })
}

/// 授权 URL。纯函数，测试据此逐项核对参数（也确认没有 PKCE 参数）。
pub(crate) fn authorize_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    let mut url = Url::parse(AUTH_ENDPOINT).expect("static authorize endpoint parses");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", SCOPES)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("include_granted_scopes", "true")
        .append_pair("state", state);
    url.to_string()
}

/// 这个登录要用的 7 条 scope，供测试和文档核对。
#[cfg(test)]
pub(crate) fn scopes() -> Vec<&'static str> {
    SCOPES.split(' ').collect()
}

/// 刷新一个 Antigravity 凭据。
pub(crate) fn refresh(tokens: &TokenSet) -> OAuthResult<TokenSet> {
    let refresh_token = crate::oauth::agents::refresh_token_of(AGENT_ID, tokens)?;
    let config = antigravity_config::client_config();
    let secret = config.require_secret()?.to_string();
    let refreshed = refresh_once(&refresh_token, &config.client_id, &secret)?;
    Ok(token::carry_over(tokens, refreshed))
}

/// 刷新一次；只有 invalid_grant 会在退避后重试一次，仍失败就是凭据失效。
fn refresh_once(refresh_token: &str, client_id: &str, secret: &str) -> OAuthResult<TokenSet> {
    let pairs = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
        ("client_secret", secret),
    ];
    match post_form(&pairs).and_then(|body| tokens_from_response(&body)) {
        Ok(refreshed) => Ok(refreshed),
        Err(error) if is_invalid_grant(&error) => {
            std::thread::sleep(INVALID_GRANT_BACKOFF);
            let body = post_form(&pairs)?;
            tokens_from_response(&body)
        }
        Err(other) => Err(other),
    }
}

/// Google 用 400 + `invalid_grant` 表示这个 refresh token 已经不能用了
/// （共享的分类表正是把它翻成 AuthRequired），而并发刷新也会短暂这样回一次。
fn is_invalid_grant(error: &OAuthError) -> bool {
    matches!(error, OAuthError::AuthRequired(message) if message.contains("invalid_grant"))
}

/// 回调到换票的整条链路。
fn receive_and_exchange(
    server: LoopbackServer,
    callbacks: mpsc::Receiver<QueryMap>,
    state: &str,
    redirect_uri: &str,
    config: &ClientConfig,
    cancel: &CancelSignal,
) -> OAuthResult<LoginOutcome> {
    let query = server.wait_for_query(&callbacks, CALLBACK_TIMEOUT, || cancel.is_cancelled())?;
    let code = code_from_query(&query, state)?;
    let tokens = exchange_code(&code, redirect_uri, config)?;
    let identity = read_identity_blocking(&tokens.access_token)?;
    Ok(LoginOutcome {
        agent: AGENT_ID.to_string(),
        tokens,
        identity,
        provider_state: None,
    })
}

fn exchange_code(code: &str, redirect_uri: &str, config: &ClientConfig) -> OAuthResult<TokenSet> {
    let secret = config.require_secret()?;
    let pairs = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", config.client_id.as_str()),
        ("client_secret", secret),
        ("redirect_uri", redirect_uri),
    ];
    let body = post_form(&pairs)?;
    tokens_from_response(&body)
}

/// POST 一个 Google 表单换票请求。
///
/// 不走 oauth::token::post_token：Google 要求 native OAuth 的 UA，而那个共享
/// 入口不带 UA；错误分类仍然共用同一张表。
fn post_form(pairs: &[(&str, &str)]) -> OAuthResult<Value> {
    let body: Vec<(String, String)> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    let answer = rt::block_on(async move {
        let client = ss_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT)
            .map_err(|error| OAuthError::Other(format!("无法构建 HTTP 客户端: {error}")))?;
        let response = client
            .post(TOKEN_ENDPOINT)
            .header("Accept", "application/json")
            .header("User-Agent", NATIVE_USER_AGENT)
            .form(&body)
            .send()
            .await
            .map_err(|error| OAuthError::Transient(format!("Google token 请求失败: {error}")))?;
        let status = response.status().as_u16();
        let text = response.text().await.map_err(|error| {
            OAuthError::Transient(format!("Google token 响应读取失败: {error}"))
        })?;
        Ok::<_, OAuthError>((status, text))
    })??;
    let (status, text) = answer;
    if !(200..300).contains(&status) {
        return Err(token::classify_status(status, &text));
    }
    serde_json::from_str(&text)
        .map_err(|error| OAuthError::Rejected(format!("Google token 响应不是合法 JSON: {error}")))
}

/// 换票响应到 TokenSet。2xx 却没有 access_token 说明这次授权拿不到可用凭据，
/// 按凭据失效处理，UI 才会给出"重新登录"而不是"稍后重试"。
fn tokens_from_response(body: &Value) -> OAuthResult<TokenSet> {
    token::tokens_from_json(body, TokenFlavor::Snake).map_err(|error| match error {
        OAuthError::Rejected(message) => OAuthError::AuthRequired(message),
        other => other,
    })
}

fn read_identity_blocking(access_token: &str) -> OAuthResult<Identity> {
    let token = access_token.to_string();
    rt::block_on(async move {
        let client = ss_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT)
            .map_err(|error| OAuthError::Other(format!("无法构建 HTTP 客户端: {error}")))?;
        read_identity(&client, &token).await
    })?
}

async fn read_identity(client: &reqwest::Client, access_token: &str) -> OAuthResult<Identity> {
    let response = client
        .get(USERINFO_ENDPOINT)
        .header("Accept", "application/json")
        .header("User-Agent", NATIVE_USER_AGENT)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|error| OAuthError::Transient(format!("Google userinfo 请求失败: {error}")))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|error| OAuthError::Transient(format!("Google userinfo 读取失败: {error}")))?;
    if !(200..300).contains(&status) {
        return Err(token::classify_status(status, &body));
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| OAuthError::Rejected(format!("Google userinfo 不是合法 JSON: {error}")))?;
    identity_from_userinfo(&value)
}

/// userinfo 到身份。email 必取：没有它这条账号就没有可用的身份键。
pub(crate) fn identity_from_userinfo(value: &Value) -> OAuthResult<Identity> {
    let email = text_field(value, "email").ok_or_else(|| {
        OAuthError::AuthRequired("Google userinfo 没有返回 email，无法识别账号".to_string())
    })?;
    Ok(Identity {
        email: Some(email),
        display_name: display_name_of(value),
        plan: None,
        account_id: None,
    })
}

/// 显示名：name 优先，否则 given_name + family_name 拼接，只有一半就用那一半。
fn display_name_of(value: &Value) -> Option<String> {
    if let Some(name) = text_field(value, "name") {
        return Some(name);
    }
    match (
        text_field(value, "given_name"),
        text_field(value, "family_name"),
    ) {
        (Some(given), Some(family)) => Some(format!("{given} {family}")),
        (given, family) => given.or(family),
    }
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// 候选回调端口：显式请求优先，然后是客户端注册的那个。
fn ports(opts: &StartOptions) -> Vec<u16> {
    match opts.prefer_port {
        Some(port) if !FIXED_PORTS.contains(&port) => {
            let mut ports = vec![port];
            ports.extend(FIXED_PORTS);
            ports
        }
        Some(port) => FIXED_PORTS
            .iter()
            .copied()
            .map(|fixed| if fixed == port { port } else { fixed })
            .collect(),
        None => FIXED_PORTS.to_vec(),
    }
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
    fn authorize_url_carries_every_required_parameter_and_no_pkce() {
        let url = authorize_url(
            "client-id.apps.googleusercontent.com",
            "http://localhost:51121/oauth-callback",
            "state-1",
        );
        let parsed = Url::parse(&url).expect("url");
        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("accounts.google.com"));
        assert_eq!(parsed.path(), "/o/oauth2/v2/auth");

        let query = query_of(&url);
        assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
        assert_eq!(
            query.get("client_id").map(String::as_str),
            Some("client-id.apps.googleusercontent.com")
        );
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some("http://localhost:51121/oauth-callback")
        );
        assert_eq!(query.get("access_type").map(String::as_str), Some("offline"));
        assert_eq!(query.get("prompt").map(String::as_str), Some("consent"));
        assert_eq!(
            query.get("include_granted_scopes").map(String::as_str),
            Some("true")
        );
        assert_eq!(query.get("state").map(String::as_str), Some("state-1"));
        assert!(!query.contains_key("code_challenge"), "{query:?}");
        assert!(!query.contains_key("code_challenge_method"), "{query:?}");
    }

    #[test]
    fn the_scope_list_is_the_seven_google_scopes() {
        let scopes = scopes();
        assert_eq!(scopes.len(), 7, "{scopes:?}");
        assert_eq!(scopes[0], "openid");
        for scope in [
            "https://www.googleapis.com/auth/cloud-platform",
            "https://www.googleapis.com/auth/userinfo.email",
            "https://www.googleapis.com/auth/userinfo.profile",
            "https://www.googleapis.com/auth/cclog",
            "https://www.googleapis.com/auth/experimentsandconfigs",
            "https://www.googleapis.com/auth/aicode",
        ] {
            assert!(scopes.contains(&scope), "{scope} missing from {scopes:?}");
        }
    }

    #[test]
    fn a_login_without_a_client_secret_fails_before_the_browser() {
        let started = start_with(
            "login-1",
            &StartOptions::default(),
            ClientConfig::for_test("client-id", None),
        )
        .err()
        .expect("no secret means no login");
        let message = started.to_string();
        assert!(message.contains("client secret"), "{message}");
    }

    #[test]
    fn a_login_binds_the_registered_callback_and_can_be_cancelled() {
        let started = start_with(
            "login-1",
            &StartOptions::default(),
            ClientConfig::for_test("client-id", Some("client-secret")),
        )
        .expect("start");
        assert_eq!(started.start.flow, LoginFlow::LocalCallback);
        let query = query_of(&started.start.auth_url);
        let redirect = query.get("redirect_uri").expect("redirect_uri");
        assert!(redirect.ends_with("/oauth-callback"), "{redirect}");
        assert!(redirect.starts_with("http://localhost:"), "{redirect}");
        assert!(started.start.expires_at.is_some());
        // Complete the login away so the listener thread does not linger.
        crate::oauth::cancel(&started.start.login_id);
    }

    #[test]
    fn refresh_without_a_refresh_token_requires_a_login() {
        let error = refresh(&TokenSet::default()).expect_err("no refresh token");
        assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");
    }

    #[test]
    fn tokens_without_an_access_token_require_a_login() {
        let error = tokens_from_response(&json!({ "refresh_token": "r" })).expect_err("no access");
        assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");

        let tokens = tokens_from_response(&json!({
            "access_token": "at-1",
            "refresh_token": "rt-1",
            "expires_in": 3599,
            "token_type": "Bearer"
        }))
        .expect("tokens");
        assert_eq!(tokens.access_token, "at-1");
        assert_eq!(tokens.refresh_token.as_deref(), Some("rt-1"));
        assert_eq!(tokens.token_type, "Bearer");
        assert!(tokens.expires_at.is_some());
    }

    #[test]
    fn invalid_grant_is_the_one_retryable_refusal() {
        assert!(is_invalid_grant(&classify_status(
            400,
            "{\"error\":\"invalid_grant\"}"
        )));
        assert!(!is_invalid_grant(&classify_status(429, "")));
        assert!(!is_invalid_grant(&classify_status(401, "")));
        assert!(!is_invalid_grant(&classify_status(500, "")));
        assert!(!is_invalid_grant(&OAuthError::Rejected("no".to_string())));
    }

    #[test]
    fn userinfo_identity_needs_the_email() {
        let identity = identity_from_userinfo(&json!({
            "email": "dev@example.com",
            "name": "Dev Example",
            "given_name": "Dev",
            "family_name": "Example"
        }))
        .expect("identity");
        assert_eq!(identity.email.as_deref(), Some("dev@example.com"));
        assert_eq!(identity.display_name.as_deref(), Some("Dev Example"));

        let error = identity_from_userinfo(&json!({ "name": "Dev" })).expect_err("no email");
        assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");
    }

    #[test]
    fn the_display_name_falls_back_to_the_name_parts() {
        let joined = identity_from_userinfo(&json!({
            "email": "dev@example.com",
            "given_name": "Dev",
            "family_name": "Example"
        }))
        .expect("identity");
        assert_eq!(joined.display_name.as_deref(), Some("Dev Example"));

        let given_only = identity_from_userinfo(&json!({
            "email": "dev@example.com",
            "given_name": "Dev"
        }))
        .expect("identity");
        assert_eq!(given_only.display_name.as_deref(), Some("Dev"));

        let none = identity_from_userinfo(&json!({ "email": "dev@example.com" })).expect("identity");
        assert_eq!(none.display_name, None);
    }

    #[test]
    fn callback_ports_start_with_the_registered_one() {
        assert_eq!(ports(&StartOptions::default()), vec![51121]);
        let preferred = StartOptions {
            prefer_port: Some(43001),
            ..StartOptions::default()
        };
        assert_eq!(ports(&preferred), vec![43001, 51121]);
        let registered = StartOptions {
            prefer_port: Some(51121),
            ..StartOptions::default()
        };
        assert_eq!(ports(&registered), vec![51121]);
    }
}

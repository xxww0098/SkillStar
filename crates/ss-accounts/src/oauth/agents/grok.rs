//! Grok（xAI）登录：设备码为主，本机回调为兜底。
//!
//! 与 Cockpit Tools 一致的两条腿：
//!
//! 1. **设备码**（默认）：`POST https://auth.x.ai/oauth2/device/code` 拿
//!    `device_code` + `user_code`，把验证页交给 UI（`RemotePoll`），然后按
//!    `interval` 轮询 `POST https://auth.x.ai/oauth2/token` 的
//!    `urn:ietf:params:oauth:grant-type:device_code` 授权。
//! 2. **本机回调**：设备端点 404 的部署才退回 `127.0.0.1:56121/callback` 的
//!    授权码 + PKCE（与 Codex 同一种 `LocalCallback` 形状）。
//!
//! 身份来自令牌本身的 JWT（email / sub）：xAI 没有 userinfo 端点，CLI 也只读
//! 自己的 `auth.json`，所以这里不额外打一次网络。
//!
//! 刷新是标准 `refresh_token` form grant，错误分类沿用 `oauth::token` 那张表。

use std::sync::{LazyLock, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;
use url::Url;

use crate::oauth::agents::{Started, code_from_query};
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::loopback::{CALLBACK_TIMEOUT, LoopbackServer, QueryMap};
use crate::oauth::session::{CancelSignal, LoginFlow, LoginStart, PendingLogin, StartOptions};
use crate::oauth::token::{self, FormGrant, GrantKind};
use crate::oauth::{Identity, LoginOutcome, TokenSet, pkce, rt};

pub(crate) const AGENT_ID: &str = "grok";

const AUTHORIZE_URL: &str = "https://auth.x.ai/oauth2/authorize";
const DEVICE_CODE_URL: &str = "https://auth.x.ai/oauth2/device/code";
pub(crate) const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";

/// Grok CLI 注册的安装型 client id（公开值；env 可覆盖）。
const BUILTIN_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const CLIENT_ID_ENV: &str = "SKILLSTAR_XAI_CLIENT_ID";

/// Grok CLI 自己的 scope 集合：少一条 `grok-cli:access` 的令牌能过计费但让每条
/// CLI 调用失败，所以这里和切号写回用的是同一份。
pub(crate) const SCOPES: &str = concat!(
    "openid profile email offline_access ",
    "grok-cli:access conversations:read conversations:write api:access ",
    "workspaces:read workspaces:write"
);

/// 本机回调兜底的端口与路径（Grok CLI 注册值）。
const CALLBACK_PORT: u16 = 56121;
const CALLBACK_PATH: &str = "/callback";

const DEFAULT_INTERVAL_SECS: u64 = 5;
const DEFAULT_EXPIRES_SECS: u64 = 1_800;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

static CLIENT_ID: LazyLock<String> = LazyLock::new(|| {
    std::env::var(CLIENT_ID_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| BUILTIN_CLIENT_ID.to_string())
});

/// 这个进程解析出的 Grok client id。
pub(crate) fn client_id() -> &'static str {
    CLIENT_ID.as_str()
}

/// `auth.json` 里一条登录的键：OIDC issuer + client id。切号写回与导入都读它，
/// 所以只在这里拼一次。
pub(crate) fn scope_key() -> String {
    format!("https://auth.x.ai::{}", client_id())
}

/// 启动一次 Grok 登录：先试设备码，端点不存在（404）才退回本机回调。
pub(crate) fn start(login_id: &str, opts: &StartOptions) -> OAuthResult<Started> {
    match begin_device_flow(client_id())? {
        DeviceBegin::Ready(session) => Ok(start_device_login(login_id, session)),
        DeviceBegin::Unavailable => start_loopback_login(login_id, opts),
    }
}

// ---------------------------------------------------------------------------
// 设备码
// ---------------------------------------------------------------------------

/// 一次设备授权：UI 只要展示验证页与配对码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    open_url: String,
    interval_secs: u64,
    expires_in: u64,
}

/// 设备端点存在与否。404 是"这个部署还没上设备码"，不是失败。
#[derive(Debug)]
pub(crate) enum DeviceBegin {
    Ready(DeviceAuthorization),
    Unavailable,
}

/// 一次轮询的回答：继续等，或拿到令牌。
#[derive(Debug)]
pub(crate) enum DevicePoll {
    Pending { interval: u64 },
    Tokens(TokenSet),
}

fn begin_device_flow(client_id: &str) -> OAuthResult<DeviceBegin> {
    let (status, body) = post_form(DEVICE_CODE_URL, &[
        ("client_id", client_id),
        ("scope", SCOPES),
    ])?;
    classify_device_http(status, &body)
}

/// 224 → Ready，404 → Unavailable，其它非 2xx 走共享分类表。
pub(crate) fn classify_device_http(status: u16, body: &str) -> OAuthResult<DeviceBegin> {
    if status == 404 {
        return Ok(DeviceBegin::Unavailable);
    }
    if !(200..300).contains(&status) {
        return Err(token::classify_status(status, body));
    }
    Ok(DeviceBegin::Ready(parse_device_authorization(body)?))
}

/// 设备授权响应。验证地址必须是 https：http 的验证页等于把配对码交给中间人。
pub(crate) fn parse_device_authorization(body: &str) -> OAuthResult<DeviceAuthorization> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| OAuthError::Rejected(format!("Grok 设备授权响应解析失败: {error}")))?;
    let device_code = text_field(&value, "device_code")
        .ok_or_else(|| OAuthError::Rejected("Grok 设备授权响应缺少 device_code".to_string()))?;
    let user_code = text_field(&value, "user_code")
        .ok_or_else(|| OAuthError::Rejected("Grok 设备授权响应缺少 user_code".to_string()))?;
    if !valid_user_code(&user_code) {
        return Err(OAuthError::Rejected(
            "Grok 设备授权返回了无法核对的配对码".to_string(),
        ));
    }
    let open_url = https_uri(text_field(&value, "verification_uri_complete").as_deref())
        .or_else(|| https_uri(text_field(&value, "verification_uri").as_deref()))
        .ok_or_else(|| {
            OAuthError::Rejected("Grok 设备授权响应缺少 https 验证地址".to_string())
        })?;
    Ok(DeviceAuthorization {
        device_code,
        user_code,
        open_url,
        interval_secs: json_u64(&value, "interval")
            .filter(|seconds| *seconds > 0)
            .unwrap_or(DEFAULT_INTERVAL_SECS),
        expires_in: json_u64(&value, "expires_in")
            .filter(|seconds| *seconds > 0)
            .unwrap_or(DEFAULT_EXPIRES_SECS),
    })
}

fn start_device_login(login_id: &str, session: DeviceAuthorization) -> Started {
    let start = LoginStart {
        login_id: login_id.to_string(),
        flow: LoginFlow::RemotePoll,
        auth_url: session.open_url.clone(),
        user_code: Some(session.user_code.clone()),
        verification_uri: Some(session.open_url.clone()),
        interval_secs: Some(session.interval_secs),
        expires_at: Some(crate::oauth::now_secs() + session.expires_in as i64),
    };
    let (pending, channel) = PendingLogin::pair(None);
    std::thread::spawn(move || {
        let outcome = poll_device_tokens(&session, &channel.cancel)
            .and_then(|tokens| outcome_from_tokens(tokens));
        let _ = channel.results.send(outcome);
    });
    Started { start, pending }
}

/// 按 interval 轮询到拿票、取消或超时。
fn poll_device_tokens(
    session: &DeviceAuthorization,
    cancel: &CancelSignal,
) -> OAuthResult<TokenSet> {
    let deadline = Instant::now() + Duration::from_secs(session.expires_in);
    let mut interval = session.interval_secs;
    loop {
        if wait_interval(interval, cancel) {
            return Err(OAuthError::Rejected("登录已取消".to_string()));
        }
        if Instant::now() >= deadline {
            return Err(OAuthError::Rejected(
                "Grok 设备授权已过期，请重新发起登录".to_string(),
            ));
        }
        let (status, body) = post_form(TOKEN_URL, &[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", session.device_code.as_str()),
            ("client_id", client_id()),
        ])?;
        match interpret_device_poll(status, &body, interval)? {
            DevicePoll::Pending { interval: next } => interval = next,
            DevicePoll::Tokens(tokens) => return Ok(tokens),
        }
    }
}

/// 轮询回答的解读：等待/放慢继续，过期/拒绝/未知设备码直接结束。
pub(crate) fn interpret_device_poll(
    status: u16,
    body: &str,
    interval: u64,
) -> OAuthResult<DevicePoll> {
    if (200..300).contains(&status) {
        return Ok(DevicePoll::Tokens(token::tokens_from_json(
            &parse_json(body)?,
            token::TokenFlavor::Snake,
        )?));
    }
    match oauth_error(body).as_deref() {
        Some("authorization_pending") => Ok(DevicePoll::Pending { interval }),
        Some("slow_down") => Ok(DevicePoll::Pending {
            interval: interval.saturating_add(5).clamp(1, 60),
        }),
        Some("expired_token") => Err(OAuthError::Rejected(
            "Grok 设备授权已过期，请重新发起登录".to_string(),
        )),
        Some("access_denied") => Err(OAuthError::Rejected("Grok 登录被拒绝".to_string())),
        Some("invalid_grant") => Err(OAuthError::Rejected(
            "Grok 登录失败，请重新发起登录".to_string(),
        )),
        _ => Err(token::classify_status(status, body)),
    }
}

/// 睡一个轮询间隔，期间每 200ms 检查一次取消。
fn wait_interval(seconds: u64, cancel: &CancelSignal) -> bool {
    for _ in 0..seconds.max(1).saturating_mul(5) {
        if cancel.is_cancelled() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    cancel.is_cancelled()
}

// ---------------------------------------------------------------------------
// 本机回调（设备端点不存在的部署）
// ---------------------------------------------------------------------------

fn start_loopback_login(login_id: &str, opts: &StartOptions) -> OAuthResult<Started> {
    let state = uuid::Uuid::new_v4().to_string();
    let pkce = pkce::generate();
    let server = LoopbackServer::bind_with(&ports(opts), CALLBACK_PATH)?;
    let redirect_uri = server.redirect_uri();
    let auth_url = authorize_url(&redirect_uri, &pkce.challenge, &state);
    let (callback_tx, callback_rx) = mpsc::channel();
    let (pending, channel) = PendingLogin::pair(Some(callback_tx));
    let verifier = pkce.verifier.clone();
    std::thread::spawn(move || {
        let outcome = receive_and_exchange(
            server,
            callback_rx,
            &state,
            &redirect_uri,
            &verifier,
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

/// 授权 URL（兜底腿）。纯函数，测试据此核对参数。
pub(crate) fn authorize_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    let mut url = Url::parse(AUTHORIZE_URL).expect("static authorize endpoint parses");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id())
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", SCOPES)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("plan", "generic");
    url.to_string()
}

fn receive_and_exchange(
    server: LoopbackServer,
    callbacks: mpsc::Receiver<QueryMap>,
    state: &str,
    redirect_uri: &str,
    code_verifier: &str,
    cancel: &CancelSignal,
) -> OAuthResult<LoginOutcome> {
    let query = server.wait_for_query(&callbacks, CALLBACK_TIMEOUT, || cancel.is_cancelled())?;
    let code = code_from_query(&query, state)?;
    let grant = FormGrant {
        url: TOKEN_URL.to_string(),
        client_id: client_id().to_string(),
        code: Some(code),
        redirect_uri: Some(redirect_uri.to_string()),
        code_verifier: Some(code_verifier.to_string()),
        ..FormGrant::default()
    };
    let tokens = token::post_token(&grant, GrantKind::AuthorizationCode)?;
    outcome_from_tokens(tokens)
}

// ---------------------------------------------------------------------------
// 刷新
// ---------------------------------------------------------------------------

/// 刷新一个 Grok 凭据：标准 refresh_token form grant。
pub(crate) fn refresh(tokens: &TokenSet) -> OAuthResult<TokenSet> {
    let refresh_token = crate::oauth::agents::refresh_token_of(AGENT_ID, tokens)?;
    let grant = FormGrant {
        url: TOKEN_URL.to_string(),
        client_id: client_id().to_string(),
        refresh_token: Some(refresh_token),
        ..FormGrant::default()
    };
    let refreshed = token::post_token(&grant, GrantKind::RefreshToken)?;
    Ok(token::carry_over(tokens, refreshed))
}

// ---------------------------------------------------------------------------
// 身份与协助
// ---------------------------------------------------------------------------

fn outcome_from_tokens(tokens: TokenSet) -> OAuthResult<LoginOutcome> {
    let identity = identity_from_tokens(&tokens);
    Ok(LoginOutcome {
        agent: AGENT_ID.to_string(),
        tokens,
        identity,
        provider_state: None,
    })
}

/// xAI 没有 userinfo：身份只来自令牌里的 JWT claim（没验证签名，见
/// [crate::identity]）。email 是账号身份，sub 是 provider 侧的账号 id。
pub(crate) fn identity_from_tokens(tokens: &TokenSet) -> Identity {
    let id_token = tokens.id_token.as_deref();
    let access = tokens.access_token.as_str();
    let email = id_token
        .and_then(crate::identity::jwt_email)
        .or_else(|| crate::identity::jwt_email(access));
    let subject = id_token
        .and_then(|token| crate::identity::jwt_claim(token, "sub"))
        .or_else(|| crate::identity::jwt_claim(access, "sub"));
    Identity {
        display_name: email.clone(),
        email,
        plan: None,
        account_id: subject,
    }
}

/// POST 一个 form 并返回 (status, body)。
fn post_form(url: &str, pairs: &[(&str, &str)]) -> OAuthResult<(u16, String)> {
    let body: Vec<(String, String)> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    let url = url.to_string();
    rt::block_on(async move {
        let client = ss_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT)
            .map_err(|error| OAuthError::Other(format!("无法构建 HTTP 客户端: {error}")))?;
        let response = client
            .post(&url)
            .header("Accept", "application/json")
            .form(&body)
            .send()
            .await
            .map_err(|error| token::transport_error("Grok 请求失败", error))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|error| token::transport_error("Grok 响应读取失败", error))?;
        Ok::<_, OAuthError>((status, text))
    })?
}

fn parse_json(body: &str) -> OAuthResult<Value> {
    serde_json::from_str(body)
        .map_err(|error| OAuthError::Rejected(format!("Grok 令牌响应不是合法 JSON: {error}")))
}

fn oauth_error(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("error")?
        .as_str()
        .map(|code| code.trim().to_ascii_lowercase())
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

/// 配对码只由大写字母、数字和连字符构成。
fn valid_user_code(code: &str) -> bool {
    !code.is_empty()
        && code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'-')
}

fn https_uri(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    value.starts_with("https://").then(|| value.to_string())
}

/// 候选回调端口：显式请求优先，然后是 CLI 注册的 56121。
fn ports(opts: &StartOptions) -> Vec<u16> {
    match opts.prefer_port {
        Some(port) if port != CALLBACK_PORT => vec![port, CALLBACK_PORT],
        _ => vec![CALLBACK_PORT],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DEVICE_JSON: &str = r#"{
        "device_code": "dev-secret",
        "user_code": "ABCD-1234",
        "verification_uri": "https://accounts.x.ai/oauth2/device",
        "verification_uri_complete": "https://accounts.x.ai/oauth2/device?user_code=ABCD-1234",
        "expires_in": 1800,
        "interval": 5
    }"#;

    #[test]
    fn the_device_authorization_opens_the_complete_verification_url() {
        let session = parse_device_authorization(DEVICE_JSON).expect("device");
        assert_eq!(session.user_code, "ABCD-1234");
        assert_eq!(
            session.open_url,
            "https://accounts.x.ai/oauth2/device?user_code=ABCD-1234"
        );
        assert_eq!(session.interval_secs, 5);
        assert_eq!(session.expires_in, 1800);
    }

    #[test]
    fn a_bad_code_or_an_insecure_uri_is_rejected() {
        let bad_code = DEVICE_JSON.replace("ABCD-1234", "not a code");
        assert!(parse_device_authorization(&bad_code).is_err());
        let insecure = DEVICE_JSON.replace("https://", "http://");
        assert!(parse_device_authorization(&insecure).is_err());
        assert!(parse_device_authorization("not json").is_err());
    }

    #[test]
    fn a_missing_device_endpoint_falls_back_and_a_real_error_does_not() {
        assert!(matches!(
            classify_device_http(404, "missing").expect("unavailable"),
            DeviceBegin::Unavailable
        ));
        assert!(matches!(
            classify_device_http(200, DEVICE_JSON).expect("ready"),
            DeviceBegin::Ready(_)
        ));
        let error = classify_device_http(400, r#"{"error":"invalid_client"}"#)
            .err()
            .expect("a rejected client is an error");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
    }

    #[test]
    fn the_poll_waits_slows_down_and_stops_on_a_terminal_error() {
        assert!(matches!(
            interpret_device_poll(400, r#"{"error":"authorization_pending"}"#, 5).expect("pending"),
            DevicePoll::Pending { interval: 5 }
        ));
        assert!(matches!(
            interpret_device_poll(400, r#"{"error":"slow_down"}"#, 5).expect("slowed"),
            DevicePoll::Pending { interval: 10 }
        ));
        for (body, needle) in [
            (r#"{"error":"expired_token"}"#, "已过期"),
            (r#"{"error":"access_denied"}"#, "被拒绝"),
            (r#"{"error":"invalid_grant"}"#, "重新发起登录"),
        ] {
            let error = interpret_device_poll(400, body, 5).err().expect("terminal");
            assert!(error.to_string().contains(needle), "{body}: got {error}");
        }

        let tokens =
            interpret_device_poll(200, r#"{"access_token":"at","refresh_token":"rt"}"#, 5)
                .expect("tokens");
        match tokens {
            DevicePoll::Tokens(tokens) => {
                assert_eq!(tokens.access_token, "at");
                assert_eq!(tokens.refresh_token.as_deref(), Some("rt"));
            }
            other => panic!("expected tokens, got {other:?}"),
        }
    }

    #[test]
    fn the_scope_key_names_the_oidc_issuer_and_the_resolved_client() {
        let key = scope_key();
        assert!(key.starts_with("https://auth.x.ai::"), "{key}");
        assert!(key.ends_with(client_id()), "{key}");
    }

    #[test]
    fn the_cli_scopes_are_the_seven_the_grok_cli_asks_for() {
        let scopes: Vec<&str> = SCOPES.split(' ').collect();
        for required in [
            "openid",
            "offline_access",
            "grok-cli:access",
            "conversations:read",
            "conversations:write",
            "api:access",
            "workspaces:write",
        ] {
            assert!(scopes.contains(&required), "{required} missing");
        }
    }

    #[test]
    fn identity_comes_from_the_tokens_own_claims() {
        let access = crate::test_support::jwt_with_claims(json!({
            "email": "dev@example.com",
            "sub": "user-1"
        }));
        let tokens = TokenSet {
            access_token: access,
            ..TokenSet::default()
        };
        let identity = identity_from_tokens(&tokens);
        assert_eq!(identity.email.as_deref(), Some("dev@example.com"));
        assert_eq!(identity.account_id.as_deref(), Some("user-1"));
        assert_eq!(identity.display_name.as_deref(), Some("dev@example.com"));

        // 不透明令牌没有可读 claim，身份留空而不是编一个。
        let opaque = identity_from_tokens(&TokenSet {
            access_token: "opaque".to_string(),
            ..TokenSet::default()
        });
        assert_eq!(opaque.email, None);
        assert_eq!(opaque.account_id, None);
    }

    #[test]
    fn refresh_without_a_refresh_token_requires_a_login() {
        let error = refresh(&TokenSet::default()).expect_err("no refresh token");
        assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");
    }

    #[test]
    fn the_fallback_authorize_url_carries_pkce_and_the_cli_scopes() {
        let url = authorize_url("http://localhost:56121/callback", "challenge-1", "state-1");
        let parsed = Url::parse(&url).expect("url");
        assert_eq!(parsed.host_str(), Some("auth.x.ai"));
        let query: std::collections::BTreeMap<String, String> = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        assert_eq!(query.get("code_challenge_method").map(String::as_str), Some("S256"));
        assert_eq!(query.get("state").map(String::as_str), Some("state-1"));
        assert_eq!(query.get("client_id").map(String::as_str), Some(client_id()));
        assert!(query.get("scope").is_some_and(|scope| scope.contains("grok-cli:access")));
    }

    #[test]
    fn callback_ports_start_with_the_registered_one() {
        assert_eq!(ports(&StartOptions::default()), vec![56121]);
        let preferred = StartOptions {
            prefer_port: Some(43001),
            ..StartOptions::default()
        };
        assert_eq!(ports(&preferred), vec![43001, 56121]);
        let registered = StartOptions {
            prefer_port: Some(56121),
            ..StartOptions::default()
        };
        assert_eq!(ports(&registered), vec![56121]);
    }
}

//! Device-code login for Grok.
//!
//! Grok CLI 1.0.40 asks a remote login-config which transport to use. When
//! that config sets `device_flow`, the CLI posts here instead of opening the
//! loopback authorize page. A 404 means the deployment still only has the
//! loopback flow, and the caller falls back.

use std::time::{Duration, Instant};

use serde_json::Value;
use url::Url;

use super::TOKEN_URL;
use crate::oauth::local_server;
use crate::oauth::pkce::PkcePair;
use crate::oauth::token_endpoint::{self, TokenResponse};
use crate::subscription::Subscription;
use crate::{UsageError, UsageResult};

const AUTHORIZE_URL: &str = "https://auth.x.ai/oauth2/authorize";
const DEVICE_CODE_URL: &str = "https://auth.x.ai/oauth2/device/code";
const CALLBACK_PORT: u16 = 56121;
const CALLBACK_PATH: &str = "/callback";
const DEFAULT_INTERVAL_SECS: u64 = 5;
const DEFAULT_EXPIRES_SECS: u64 = 1_800;

pub(super) enum DeviceBegin {
    Ready(DeviceAuthorization),
    /// The device endpoint is not on this deployment. Use loopback login.
    Unavailable,
}

pub(super) struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub open_url: String,
    pub interval_secs: u64,
    pub expires_in: u64,
}

#[derive(Debug)]
pub(super) enum DevicePoll {
    Pending { interval: u64 },
    Tokens(TokenResponse),
}

pub(super) fn start_device_login(
    session: DeviceAuthorization,
    target_subscription_id: Option<&str>,
) -> super::super::OAuthStartInfo {
    let open_url = session.open_url.clone();
    let user_code = session.user_code.clone();
    let interval = u32::try_from(session.interval_secs).ok();
    let pending_id = register_pending(
        open_url.clone(),
        target_subscription_id,
        super::super::OAuthFlow::RemotePoll,
    );
    let pid = pending_id.clone();
    tokio::spawn(async move {
        let target_subscription_id = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_device_login(pid.clone(), session, target_subscription_id).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });
    super::super::OAuthStartInfo::device(pending_id, open_url, user_code, interval)
}

pub(super) async fn start_loopback_login(
    target_subscription_id: Option<&str>,
) -> UsageResult<super::super::OAuthStartInfo> {
    let pkce = PkcePair::generate();
    let state = crate::oauth::pkce::random_state();
    let nonce = crate::oauth::pkce::random_state();
    let redirect_uri = format!("http://127.0.0.1:{CALLBACK_PORT}{CALLBACK_PATH}");
    let auth_url = build_authorize_url(
        AUTHORIZE_URL,
        &redirect_uri,
        &pkce.challenge,
        &state,
        &nonce,
    )?;
    let pending_id = register_pending(
        auth_url.clone(),
        target_subscription_id,
        super::super::OAuthFlow::LocalCallback,
    );
    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let target_subscription_id = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(
            state_for_task,
            verifier,
            redirect_uri,
            target_subscription_id,
        )
        .await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });
    Ok(super::super::OAuthStartInfo::browser(auth_url, pending_id))
}

pub(super) fn build_authorize_url(
    endpoint: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    nonce: &str,
) -> UsageResult<String> {
    let mut url = Url::parse(endpoint)
        .map_err(|error| UsageError::Fetcher(format!("Grok authorize URL 无效: {error}")))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("response_type", "code")
            .append_pair("client_id", super::client_id())
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", super::SCOPES)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("plan", "generic")
            .append_pair("referrer", "skillstar");
    }
    Ok(url.to_string())
}

pub(super) fn register_pending(
    auth_url: String,
    target_subscription_id: Option<&str>,
    flow: super::super::OAuthFlow,
) -> String {
    let pending_id = match flow {
        super::super::OAuthFlow::LocalCallback => {
            crate::oauth::pending_state::register("xai", None, auth_url)
        }
        other => crate::oauth::pending_state::register_with_flow("xai", None, auth_url, other),
    };
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );
    pending_id
}

async fn drive_device_login(
    pending_id: String,
    session: DeviceAuthorization,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let tokens = poll_device_token(&pending_id, super::client_id(), &session).await?;
    if crate::oauth::pending_state::flow(&pending_id).is_none() {
        return Err(UsageError::Other("用户取消登录".into()));
    }
    crate::refresh_guard::with_catalog_lock("xai", || async {
        super::finalize(tokens, target_subscription_id.as_deref()).await
    })
    .await?
}

async fn drive_login(
    state: String,
    verifier: String,
    redirect_uri: String,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let code = local_server::wait_for_callback(CALLBACK_PORT, state, None).await?;
    let tokens = exchange_code(&code, &verifier, &redirect_uri).await?;
    crate::refresh_guard::with_catalog_lock("xai", || async {
        super::finalize(tokens, target_subscription_id.as_deref()).await
    })
    .await?
}

async fn exchange_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> UsageResult<TokenResponse> {
    token_endpoint::post_token(
        TOKEN_URL,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", super::client_id()),
            ("code_verifier", verifier),
        ],
        "Grok token",
    )
    .await
}

pub(super) async fn begin_device_flow(client_id: &str, scopes: &str) -> UsageResult<DeviceBegin> {
    let client = crate::http_client::usage_http_client()?;
    let response = client
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id), ("scope", scopes)])
        .send()
        .await
        .map_err(|error| UsageError::transport("Grok device", error))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    classify_device_http(status, &body)
}

pub(super) fn classify_device_http(status: u16, body: &str) -> UsageResult<DeviceBegin> {
    if status == 404 {
        return Ok(DeviceBegin::Unavailable);
    }
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status("Grok device", status, body));
    }
    Ok(DeviceBegin::Ready(parse_device_authorization(body)?))
}

pub(super) fn parse_device_authorization(body: &str) -> UsageResult<DeviceAuthorization> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| UsageError::Fetcher(format!("Grok 设备授权响应解析失败: {error}")))?;
    let device_code = string_field(&value, "device_code")
        .ok_or_else(|| UsageError::Fetcher("Grok 设备授权响应缺少 device_code".into()))?;
    let user_code = string_field(&value, "user_code")
        .ok_or_else(|| UsageError::Fetcher("Grok 设备授权响应缺少 user_code".into()))?;
    if !valid_user_code(&user_code) {
        return Err(UsageError::Fetcher(
            "Grok 设备授权返回了无法核对的配对码".into(),
        ));
    }
    let open_url = https_uri(string_field(&value, "verification_uri_complete").as_deref())
        .or_else(|| https_uri(string_field(&value, "verification_uri").as_deref()))
        .ok_or_else(|| UsageError::Fetcher("Grok 设备授权响应缺少 https 验证地址".into()))?;
    let interval_secs = json_u64(&value, "interval")
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_INTERVAL_SECS);
    let expires_in = json_u64(&value, "expires_in")
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_EXPIRES_SECS);
    Ok(DeviceAuthorization {
        device_code,
        user_code,
        open_url,
        interval_secs,
        expires_in,
    })
}

pub(super) async fn poll_device_token(
    pending_id: &str,
    client_id: &str,
    session: &DeviceAuthorization,
) -> UsageResult<TokenResponse> {
    let client = crate::http_client::usage_http_client()?;
    let deadline = Instant::now() + Duration::from_secs(session.expires_in);
    let mut interval = session.interval_secs;
    loop {
        wait_interval(pending_id, interval).await?;
        if Instant::now() >= deadline {
            return Err(UsageError::Other(
                "Grok 设备授权已过期，请重新发起登录".into(),
            ));
        }
        let response = client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", session.device_code.as_str()),
                ("client_id", client_id),
            ])
            .send()
            .await
            .map_err(|error| UsageError::transport("Grok device", error))?;
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        match interpret_device_poll(status, &body, interval)? {
            DevicePoll::Pending { interval: next } => interval = next,
            DevicePoll::Tokens(tokens) => return Ok(tokens),
        }
    }
}

pub(super) fn interpret_device_poll(
    status: u16,
    body: &str,
    interval: u64,
) -> UsageResult<DevicePoll> {
    if (200..300).contains(&status) {
        return Ok(DevicePoll::Tokens(token_endpoint::parse_token_body(
            status,
            body,
            "Grok device",
        )?));
    }
    match oauth_error(body).as_deref() {
        Some("authorization_pending") => Ok(DevicePoll::Pending { interval }),
        Some("slow_down") => Ok(DevicePoll::Pending {
            interval: interval.saturating_add(5).clamp(1, 60),
        }),
        Some("expired_token") => Err(UsageError::Other(
            "Grok 设备授权已过期，请重新发起登录".into(),
        )),
        Some("access_denied") => Err(UsageError::Other("Grok 登录被拒绝".into())),
        Some("invalid_grant") => Err(UsageError::Other("Grok 登录失败，请重新发起登录".into())),
        _ => match token_endpoint::parse_token_body(status, body, "Grok device") {
            Ok(tokens) => Ok(DevicePoll::Tokens(tokens)),
            Err(error) => Err(error),
        },
    }
}

async fn wait_interval(pending_id: &str, seconds: u64) -> UsageResult<()> {
    let steps = seconds.max(1).saturating_mul(5);
    for _ in 0..steps {
        if crate::oauth::pending_state::flow(pending_id).is_none() {
            return Err(UsageError::Other("用户取消登录".into()));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    if crate::oauth::pending_state::flow(pending_id).is_none() {
        return Err(UsageError::Other("用户取消登录".into()));
    }
    Ok(())
}

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

fn string_field(value: &Value, key: &str) -> Option<String> {
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

fn oauth_error(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("error")?
        .as_str()
        .map(|code| code.trim().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE_JSON: &str = r#"{
        "device_code": "dev-secret",
        "user_code": "23VE-QHYK",
        "verification_uri": "https://accounts.x.ai/oauth2/device",
        "verification_uri_complete": "https://accounts.x.ai/oauth2/device?user_code=23VE-QHYK",
        "expires_in": 1800,
        "interval": 5
    }"#;

    #[test]
    fn device_authorization_opens_the_complete_verification_url() {
        let session = parse_device_authorization(DEVICE_JSON).unwrap();
        assert_eq!(session.user_code, "23VE-QHYK");
        assert_eq!(
            session.open_url,
            "https://accounts.x.ai/oauth2/device?user_code=23VE-QHYK"
        );
        assert_eq!(session.interval_secs, 5);
        assert_eq!(session.expires_in, 1800);
        assert_eq!(session.device_code, "dev-secret");
    }

    #[test]
    fn device_authorization_rejects_a_bad_code_or_insecure_uri() {
        let bad_code = DEVICE_JSON.replace("23VE-QHYK", "not a code");
        assert!(parse_device_authorization(&bad_code).is_err());

        let insecure = DEVICE_JSON.replace("https://", "http://");
        assert!(parse_device_authorization(&insecure).is_err());
    }

    #[test]
    fn device_start_404_falls_back_and_other_errors_do_not() {
        assert!(matches!(
            classify_device_http(404, "missing").unwrap(),
            DeviceBegin::Unavailable
        ));
        assert!(classify_device_http(400, r#"{"error":"invalid_client"}"#).is_err());
        assert!(matches!(
            classify_device_http(200, DEVICE_JSON).unwrap(),
            DeviceBegin::Ready(_)
        ));
    }

    #[test]
    fn device_poll_waits_slows_down_and_stops_on_a_terminal_error() {
        let pending =
            interpret_device_poll(400, r#"{"error":"authorization_pending"}"#, 5).unwrap();
        assert!(matches!(pending, DevicePoll::Pending { interval: 5 }));

        let slowed = interpret_device_poll(400, r#"{"error":"slow_down"}"#, 5).unwrap();
        assert!(matches!(slowed, DevicePoll::Pending { interval: 10 }));

        let expired = interpret_device_poll(400, r#"{"error":"expired_token"}"#, 5).unwrap_err();
        assert!(
            expired.to_string().contains("已过期"),
            "expired device code should ask for a new login, got {expired}"
        );

        let denied = interpret_device_poll(400, r#"{"error":"access_denied"}"#, 5).unwrap_err();
        assert!(
            denied.to_string().contains("被拒绝"),
            "a denied login should stop polling, got {denied}"
        );

        let unknown = interpret_device_poll(
            400,
            r#"{"error":"invalid_grant","error_description":"Unknown device code"}"#,
            5,
        )
        .unwrap_err();
        assert!(
            unknown.to_string().contains("重新发起登录"),
            "an unknown device code should stop the poll, got {unknown}"
        );
    }
}

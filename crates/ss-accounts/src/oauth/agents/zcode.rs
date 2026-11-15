//! ZCode 登录：zcode:// 深链回调 + JSON token 交换。
//!
//! 与 Cockpit Tools 一致：
//!
//! * 两个上游，靠 StartOptions.region 选（"zai" 默认 / "bigmodel"）：
//!   - zai：授权页 https://chat.z.ai/api/oauth/authorize，回调
//!     zcode://zai-auth/callback；code 换出的是上游令牌，还要再打一次
//!     https://api.z.ai/api/auth/z/login 换成业务访问令牌。
//!   - bigmodel：授权页 https://bigmodel.cn/login，回调
//!     zcode://oauth/callback，换出的就是访问令牌 + refresh token。
//! * 换票端点是 POST https://zcode.z.ai/api/v1/oauth/token。
//!
//! 回调是自定义协议，host 与 path 都是语义的一部分（两个上游各用一条），所以
//! 这类流程拿到的是粘贴原文，由本模块自己校验协议、白名单与 state。
//!
//! 刷新只有 bigmodel 腿有：zai 的交换不给 refresh token，重登是唯一办法。

use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use url::Url;

use crate::oauth::agents::Started;
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::session::{CancelSignal, LoginFlow, LoginStart, PendingLogin, StartOptions};
use crate::oauth::token;
use crate::oauth::{Identity, LoginOutcome, TokenSet, rt};

pub(crate) const AGENT_ID: &str = "zcode";

/// 官方客户端的版本号：billing 端点要它做 app_version。
pub(crate) const APP_VERSION: &str = "3.14.3";

const ZAI_AUTHORIZE: &str = "https://chat.z.ai/api/oauth/authorize";
const BIGMODEL_AUTHORIZE: &str = "https://bigmodel.cn/login";
/// zai 腿注册的回调（host 与 path 都要对）。
pub(crate) const ZAI_REDIRECT: &str = "zcode://zai-auth/callback";
/// bigmodel 腿注册的回调。
pub(crate) const BIGMODEL_REDIRECT: &str = "zcode://oauth/callback";
const ZAI_CLIENT_ID: &str = "client_P8X5CMWmlaRO9gyO-KSqtg";

pub(crate) const TOKEN_URL: &str = "https://zcode.z.ai/api/v1/oauth/token";
const ZAI_BUSINESS_URL: &str = "https://api.z.ai/api/auth/z/login";

/// 深链粘贴的等待上限（用户要把浏览器里的链接贴回来）。
const PASTE_TIMEOUT: Duration = Duration::from_secs(600);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// 上游：zai（默认）或 bigmodel。
pub(crate) fn normalize_provider(region: Option<&str>) -> OAuthResult<&'static str> {
    match region.unwrap_or("zai").trim().to_ascii_lowercase().as_str() {
        "" | "zai" => Ok("zai"),
        "bigmodel" => Ok("bigmodel"),
        _ => Err(OAuthError::Rejected(
            "不支持的 ZCode 上游，请选择 zai 或 bigmodel".to_string(),
        )),
    }
}

/// 账号上记的上游（provider_state）。
pub(crate) fn provider_of(provider_state: Option<&Value>) -> OAuthResult<&'static str> {
    normalize_provider(
        provider_state
            .and_then(|state| state.get("provider"))
            .and_then(Value::as_str),
    )
}

/// provider_state：只记上游，别的一概不存。
pub(crate) fn provider_state(provider: &str) -> Value {
    json!({ "provider": provider })
}

/// 启动一次 ZCode 登录：把授权页交给 UI，等用户把 zcode:// 链接贴回来。
pub(crate) fn start(login_id: &str, opts: &StartOptions) -> OAuthResult<Started> {
    let provider = normalize_provider(opts.region.as_deref())?;
    let state = new_state();
    let auth_url = authorize_url(provider, &state)?;
    let (pending, channel, texts) = PendingLogin::pair_scheme();
    std::thread::spawn(move || {
        let outcome = wait_for_paste(&texts, &channel.cancel)
            .and_then(|text| parse_callback(&text, provider, &state))
            .and_then(|code| exchange_code(provider, &code, &state));
        let _ = channel.results.send(outcome);
    });
    Ok(Started {
        start: LoginStart {
            login_id: login_id.to_string(),
            flow: LoginFlow::SchemePaste,
            auth_url,
            user_code: None,
            verification_uri: None,
            interval_secs: None,
            expires_at: Some(crate::oauth::now_secs() + PASTE_TIMEOUT.as_secs() as i64),
        },
        pending,
    })
}

/// 32 字节随机十六进制（state）。
fn new_state() -> String {
    let mut bytes = [0_u8; 32];
    for byte in &mut bytes {
        *byte = rand::random::<u8>();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 授权页。纯函数，测试据此核对参数。
pub(crate) fn authorize_url(provider: &str, state: &str) -> OAuthResult<String> {
    if provider == "zai" {
        let mut url = Url::parse(ZAI_AUTHORIZE)
            .map_err(|error| OAuthError::Other(format!("Z.ai 授权地址无效: {error}")))?;
        url.query_pairs_mut()
            .append_pair("redirect_uri", ZAI_REDIRECT)
            .append_pair("response_type", "code")
            .append_pair("client_id", ZAI_CLIENT_ID)
            .append_pair("state", state);
        return Ok(url.to_string());
    }
    let mut url = Url::parse(BIGMODEL_AUTHORIZE)
        .map_err(|error| OAuthError::Other(format!("BigModel 授权地址无效: {error}")))?;
    url.query_pairs_mut()
        .append_pair("redirect", BIGMODEL_REDIRECT)
        .append_pair("appId", "zcode")
        .append_pair("state", state);
    Ok(url.to_string())
}

/// 等用户粘贴，期间检查取消。
fn wait_for_paste(
    texts: &mpsc::Receiver<String>,
    cancel: &CancelSignal,
) -> OAuthResult<String> {
    let deadline = Instant::now() + PASTE_TIMEOUT;
    loop {
        if cancel.is_cancelled() {
            return Err(OAuthError::Rejected("登录已取消".to_string()));
        }
        if Instant::now() >= deadline {
            return Err(OAuthError::Rejected(
                "等待粘贴 ZCode 回调超时，请重新发起登录".to_string(),
            ));
        }
        match texts.recv_timeout(Duration::from_millis(200)) {
            Ok(text) => return Ok(text),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(OAuthError::Rejected("登录已取消".to_string()));
            }
        }
    }
}

/// 校验粘贴的深链并取出授权码：协议、白名单、上游、state 一个都不能松。
pub(crate) fn parse_callback(callback: &str, provider: &str, expected_state: &str) -> OAuthResult<String> {
    let url = Url::parse(callback.trim())
        .map_err(|error| OAuthError::Rejected(format!("ZCode 回调链接无效: {error}")))?;
    if url.scheme() != "zcode" {
        return Err(OAuthError::Rejected(
            "ZCode 回调协议必须是 zcode://".to_string(),
        ));
    }
    let registered = [(ZAI_REDIRECT, "zai-auth"), (BIGMODEL_REDIRECT, "oauth")]
        .iter()
        .any(|(_, host)| url.host_str() == Some(*host) && url.path() == "/callback");
    if !registered {
        return Err(OAuthError::Rejected(
            "ZCode 回调地址不在白名单（只接受 zcode://oauth/callback 与 zcode://zai-auth/callback）"
                .to_string(),
        ));
    }
    let expected_host = if provider == "zai" { "zai-auth" } else { "oauth" };
    if url.host_str() != Some(expected_host) {
        return Err(OAuthError::Rejected(
            "ZCode 回调地址与登录上游不匹配".to_string(),
        ));
    }
    let query: std::collections::BTreeMap<String, String> = url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if let Some(error) = query.get("error").filter(|error| !error.is_empty()) {
        return Err(OAuthError::Rejected(format!("ZCode 授权失败: {error}")));
    }
    let state = query
        .get("state")
        .filter(|state| !state.is_empty())
        .ok_or_else(|| OAuthError::Rejected("ZCode 回调缺少 state".to_string()))?;
    if state != expected_state {
        return Err(OAuthError::Rejected(
            "ZCode OAuth state 不匹配或已过期".to_string(),
        ));
    }
    ["code", "authCode"]
        .into_iter()
        .find_map(|key| query.get(key))
        .filter(|code| !code.is_empty())
        .cloned()
        .ok_or_else(|| OAuthError::Rejected("ZCode 回调缺少 code".to_string()))
}

/// 授权码 → 令牌。zai 还要多换一次业务令牌。
pub(crate) fn exchange_code(provider: &str, code: &str, state: &str) -> OAuthResult<LoginOutcome> {
    let redirect = if provider == "zai" {
        ZAI_REDIRECT
    } else {
        BIGMODEL_REDIRECT
    };
    let envelope = post_json(
        TOKEN_URL,
        &json!({
            "provider": provider,
            "code": code,
            "redirect_uri": redirect,
            "state": state,
        }),
        "ZCode OAuth Token",
    )?;
    let provider_token = pick_string(
        &envelope,
        &[
            &["providerAccessToken"],
            &["provider_access_token"],
            &["accessToken"],
            &["access_token"],
        ],
    )
    .ok_or_else(|| OAuthError::Rejected("ZCode 令牌响应缺少 provider_access_token".to_string()))?;
    let jwt = pick_string(&envelope, &[&["jwt"], &["token"]]);
    let expires_at = json_u64(&envelope, &["expiresIn", "expires_in"])
        .filter(|seconds| *seconds > 0)
        .map(|seconds| crate::oauth::now_secs() + seconds as i64);
    let (access_token, refresh_token) = if provider == "zai" {
        let business = post_json(
            ZAI_BUSINESS_URL,
            &json!({ "token": provider_token }),
            "Z.ai 业务 Token",
        )?;
        (zai_business_token(&business)?, None)
    } else {
        (
            provider_token,
            pick_string(&envelope, &[&["refreshToken"], &["refresh_token"]]),
        )
    };
    let tokens = TokenSet {
        access_token,
        refresh_token,
        id_token: jwt,
        expires_at,
        token_type: "Bearer".to_string(),
    };
    let identity = identity_of(&tokens, envelope.get("userInfo"));
    Ok(LoginOutcome {
        agent: AGENT_ID.to_string(),
        tokens,
        identity,
        provider_state: Some(provider_state(provider)),
    })
}

/// zai 的业务令牌：返回码必须是 0 或 200，令牌在 data.token 或根上。
pub(crate) fn zai_business_token(body: &Value) -> OAuthResult<String> {
    let code = body.get("code");
    let ok = match code {
        None => true,
        Some(value) => {
            value.as_i64() == Some(0)
                || value.as_i64() == Some(200)
                || value.as_str() == Some("0")
                || value.as_str() == Some("200")
        }
    };
    if !ok {
        return Err(OAuthError::Rejected(format!(
            "Z.ai 业务 Token 被拒绝: {}",
            code.map(|value| value.to_string()).unwrap_or_default()
        )));
    }
    pick_string(
        body,
        &[&["data", "token"], &["token"], &["data", "accessToken"]],
    )
    .ok_or_else(|| OAuthError::Rejected("Z.ai 业务 Token 响应缺少令牌".to_string()))
}

/// 刷新：只有 bigmodel 腿换得出新令牌。
pub(crate) fn refresh(
    tokens: &TokenSet,
    provider_state: Option<&Value>,
) -> OAuthResult<TokenSet> {
    let provider = provider_of(provider_state)?;
    let refresh_token = crate::oauth::agents::refresh_token_of(AGENT_ID, tokens)?;
    if provider == "zai" {
        return Err(OAuthError::AuthRequired(
            "ZCode 的 zai 上游不发放 refresh token，需要重新登录".to_string(),
        ));
    }
    let envelope = post_json(
        TOKEN_URL,
        &json!({
            "provider": provider,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }),
        "ZCode refresh",
    )?;
    let access_token = pick_string(
        &envelope,
        &[
            &["providerAccessToken"],
            &["provider_access_token"],
            &["accessToken"],
            &["access_token"],
        ],
    )
    .ok_or_else(|| OAuthError::AuthRequired("ZCode 刷新响应缺少访问令牌".to_string()))?;
    let refreshed = TokenSet {
        access_token,
        refresh_token: pick_string(&envelope, &[&["refreshToken"], &["refresh_token"]]),
        id_token: pick_string(&envelope, &[&["jwt"], &["token"]]),
        expires_at: json_u64(&envelope, &["expiresIn", "expires_in"])
            .filter(|seconds| *seconds > 0)
            .map(|seconds| crate::oauth::now_secs() + seconds as i64),
        token_type: "Bearer".to_string(),
    };
    Ok(token::carry_over(tokens, refreshed))
}

/// 身份：先看令牌 claim，其次响应里的 userInfo。
pub(crate) fn identity_of(tokens: &TokenSet, user_info: Option<&Value>) -> Identity {
    let jwt = tokens.id_token.as_deref();
    let email = jwt
        .and_then(crate::identity::jwt_email)
        .or_else(|| crate::identity::jwt_email(&tokens.access_token))
        .or_else(|| user_info.and_then(|info| pick_string(info, &[&["email"]])));
    let account_id = jwt
        .and_then(|token| crate::identity::jwt_claim(token, "sub"))
        .or_else(|| crate::identity::jwt_claim(&tokens.access_token, "sub"))
        .or_else(|| user_info.and_then(|info| pick_string(info, &[&["userId"], &["id"]])));
    Identity {
        display_name: email.clone(),
        email,
        plan: None,
        account_id,
    }
}

fn pick_string(value: &Value, paths: &[&[&str]]) -> Option<String> {
    for path in paths {
        let mut current = value;
        let mut found = true;
        for key in *path {
            match current.get(*key) {
                Some(next) => current = next,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if found
            && let Some(text) = current
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

fn json_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        let value = value.get(*key)?;
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
    })
}

fn post_json(url: &str, body: &Value, label: &str) -> OAuthResult<Value> {
    let url = url.to_string();
    let body = body.clone();
    let label_in_request = label.to_string();
    let (status, text) = rt::block_on(async move {
        let client = ss_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT)
            .map_err(|error| OAuthError::Other(format!("无法构建 HTTP 客户端: {error}")))?;
        let response = client
            .post(&url)
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                OAuthError::Transient(format!("{label_in_request} 请求失败: {error}"))
            })?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|error| {
                OAuthError::Transient(format!("{label_in_request} 响应读取失败: {error}"))
            })?;
        Ok::<_, OAuthError>((status, text))
    })??;
    if !(200..300).contains(&status) {
        return Err(token::classify_status(status, &text));
    }
    serde_json::from_str(&text)
        .map_err(|error| OAuthError::Rejected(format!("{label} 响应不是合法 JSON: {error}")))
}

#[cfg(test)]
#[path = "zcode_tests.rs"]
mod zcode_tests;

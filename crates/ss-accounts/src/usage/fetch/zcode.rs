//! ZCode 额度：上游用户信息 + zcode.z.ai 的计费余额 + 上游配额上限。
//!
//! 三个端点（与 Cockpit Tools 一致）：
//!
//! * 用户信息：zai → GET https://chat.z.ai/api/oauth/userinfo（Bearer 访问令牌）；
//!   bigmodel → GET https://open.bigmodel.cn/api/biz/customer/getCustomerInfo
//!   （Authorization 直接是访问令牌）。
//! * 余额：GET https://zcode.z.ai/api/v1/zcode-plan/billing/balance?app_version=…
//!   用的是 **JWT**（令牌交换里那条），并且必须带 X-Device-Mid——少了它服务端
//!   回 400 / 业务码 3001。
//! * 配额上限：GET {上游}/api/monitor/usage/quota/limit（Bearer 访问令牌）。
//!
//! 没搬的：MCP 用量窗口（/api/v1/mcp/usage）与重置卡（reset.rs）。前者只影响
//! 一条附加窗口，后者是"花掉一次重置"，都不属于账号域的五个动作。

use std::sync::OnceLock;

use serde_json::Value;

use crate::oauth::agents::zcode::{APP_VERSION, provider_of};
use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};

use super::parse;
use super::zcode_parse;

const ZAI_USERINFO: &str = "https://chat.z.ai/api/oauth/userinfo";
const BIGMODEL_CUSTOMER: &str = "https://open.bigmodel.cn/api/biz/customer/getCustomerInfo";
const BILLING: &str = "https://zcode.z.ai/api/v1/zcode-plan/billing/balance";
const ZAI_QUOTA: &str = "https://api.z.ai/api/monitor/usage/quota/limit";
const BIGMODEL_QUOTA: &str = "https://open.bigmodel.cn/api/monitor/usage/quota/limit";

const PROFILE_LABEL: &str = "zcode profile";
const BILLING_LABEL: &str = "zcode billing";
const LIMITS_LABEL: &str = "zcode quota limit";

static FALLBACK_DEVICE_MID: OnceLock<String> = OnceLock::new();

/// Read the ZCode quota for one account.
pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome> {
    let reading = super::tokens_for_read("zcode", creds)?;
    let now = super::unix_now();
    let access_token = reading.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err(UsageError::AuthRequired(
            "ZCode 没有可用的 access token，请重新登录".to_string(),
        ));
    }
    let provider = provider_of(
        reading
            .provider_state
            .as_ref()
            .or(creds.provider_state.as_ref()),
    )
    .map_err(|error| UsageError::Unreadable(error.to_string()))?;
    let jwt = reading.tokens.id_token.as_deref().map(str::trim).filter(|token| !token.is_empty());

    let profile = fetch_profile(provider, access_token)?;
    let identity = crate::oauth::agents::zcode::identity_of(&reading.tokens, Some(&profile));
    // 余额是加分项（仅 Start Plan 有内容）：JWT 缺失或端点抖动时，照样读 quota/limit。
    let billing = jwt.and_then(|jwt| fetch_billing(jwt).ok());
    let has_billing_windows = billing
        .as_ref()
        .is_some_and(|value| !zcode_parse::billing_windows(value).is_empty());
    let (limits, limits_err) = match fetch_limits(provider, access_token) {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(error)),
    };

    let snapshot = zcode_parse::snapshot_from(&profile, billing.as_ref(), limits.as_ref(), now);
    if let Some(error) = limits_err
        && snapshot.windows.is_empty()
        && snapshot.plan.is_none()
    {
        if !has_billing_windows
            && matches!(error, UsageError::AuthRequired(_) | UsageError::Transient(_))
        {
            return Err(error);
        }
        if billing.is_none() {
            return Err(error);
        }
    }
    Ok(UsageOutcome {
        snapshot,
        tokens: reading.tokens,
        provider_state: reading.provider_state,
        identity: Some(identity),
    })
}

fn fetch_profile(provider: &str, access_token: &str) -> UsageResult<Value> {
    let (url, authorization) = if provider == "zai" {
        (ZAI_USERINFO, format!("Bearer {access_token}"))
    } else {
        (BIGMODEL_CUSTOMER, access_token.to_string())
    };
    let headers = [
        ("Authorization", authorization),
        ("Accept", "application/json".to_string()),
    ];
    let answer = super::http_get(PROFILE_LABEL, url, &headers)?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(PROFILE_LABEL, answer.status, &answer.body));
    }
    parse_body(PROFILE_LABEL, &answer.body)
}

fn fetch_billing(jwt: &str) -> UsageResult<Value> {
    let headers = [
        ("Authorization", format!("Bearer {jwt}")),
        ("Accept", "application/json".to_string()),
        ("User-Agent", format!("ZCode/{APP_VERSION}")),
        ("Referer", "https://zcode.z.ai".to_string()),
        ("X-Device-Mid", device_mid()),
    ];
    let url = format!("{BILLING}?app_version={APP_VERSION}");
    let answer = super::http_get(BILLING_LABEL, &url, &headers)?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(BILLING_LABEL, answer.status, &answer.body));
    }
    parse_body(BILLING_LABEL, &answer.body)
}

fn fetch_limits(provider: &str, access_token: &str) -> UsageResult<Value> {
    let url = if provider == "zai" {
        ZAI_QUOTA
    } else {
        BIGMODEL_QUOTA
    };
    let headers = [
        ("Authorization", format!("Bearer {access_token}")),
        ("Accept", "application/json".to_string()),
    ];
    let answer = super::http_get(LIMITS_LABEL, url, &headers)?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(LIMITS_LABEL, answer.status, &answer.body));
    }
    parse_body(LIMITS_LABEL, &answer.body)
}

fn parse_body(label: &str, body: &str) -> UsageResult<Value> {
    let value: Value = serde_json::from_str(body).map_err(|error| {
        UsageError::Unreadable(format!(
            "{label}: cannot parse the response: {error}: {}",
            parse::snippet(body)
        ))
    })?;
    check_envelope(label, &value)?;
    Ok(value)
}

fn check_envelope(label: &str, value: &Value) -> UsageResult<()> {
    let code = value.get("code");
    let is_auth = code.is_some_and(|c| c.as_i64() == Some(401) || c.as_str() == Some("401"));
    let code_ok = match code {
        None => true,
        Some(c) => {
            c.as_i64() == Some(0)
                || c.as_i64() == Some(200)
                || c.as_str() == Some("0")
                || c.as_str() == Some("200")
        }
    };
    let success_false = value.get("success").and_then(Value::as_bool) == Some(false);
    if !is_auth && code_ok && !success_false {
        return Ok(());
    }
    let msg = parse::text_in(value, &["msg", "message"])
        .or_else(|| code.map(|c| c.to_string()))
        .unwrap_or_else(|| "success=false".to_string());
    let detail = format!("{label}: {msg}");
    if is_auth {
        Err(UsageError::AuthRequired(detail))
    } else {
        Err(UsageError::Unreadable(detail))
    }
}

/// X-Device-Mid：先读官方客户端自己写在 telemetry-state.json 里的 deviceMid，
/// 读不到就用本进程自己铸一个 UUID。
///
/// 与 Cockpit Tools 的差别：它把自铸的 UUID 落到磁盘复用；这里只在进程内复用
/// （省一次写文件的副作用，服务端只检查"有没有带"）。
fn device_mid() -> String {
    read_official_device_mid()
        .unwrap_or_else(|| FALLBACK_DEVICE_MID.get_or_init(|| uuid::Uuid::new_v4().to_string()).clone())
}

fn read_official_device_mid() -> Option<String> {
    let path = crate::switch::paths::zcode_telemetry_path(&crate::switch::paths::PathRoots::from_env());
    let body = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&body).ok()?;
    let text = value.get("deviceMid")?.as_str()?.trim();
    uuid::Uuid::parse_str(text).ok().map(|id| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::TokenSet;

    #[test]
    fn a_read_without_a_token_asks_for_a_new_login() {
        let creds = Credentials {
            tokens: TokenSet::default(),
            account_id: None,
            provider_state: None,
            needs_identity: false,
        };
        let error = fetch(&creds).expect_err("no token");
        assert!(matches!(error, UsageError::AuthRequired(_)), "got {error:?}");
    }

    #[test]
    fn the_device_mid_is_always_a_uuid() {
        let _sandbox = crate::test_support::Sandbox::new();
        let mid = device_mid();
        assert!(uuid::Uuid::parse_str(&mid).is_ok(), "{mid}");
        // 同一进程内稳定复用。
        assert_eq!(mid, device_mid());
    }

    #[test]
    fn parse_body_checks_business_envelope_codes() {
        assert!(parse_body("test", r#"{"code":200,"success":true,"data":{}}"#).is_ok());
        assert!(parse_body("test", r#"{"sub":"u1","name":"alice"}"#).is_ok());
        let auth = parse_body("test", r#"{"code":401,"msg":"expired","success":false}"#)
            .expect_err("401");
        assert!(matches!(auth, UsageError::AuthRequired(_)), "got {auth:?}");
        let fail = parse_body("test", r#"{"code":200,"success":false,"msg":"denied"}"#)
            .expect_err("success=false");
        assert!(matches!(fail, UsageError::Unreadable(_)), "got {fail:?}");
    }
}

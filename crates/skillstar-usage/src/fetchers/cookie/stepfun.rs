//! StepFun (阶跃星辰) — Cookie-based account-balance fetcher.
//!
//! 阶跃 does NOT expose a public balance endpoint on `api.stepfun.com`; the
//! only source is the developer console's internal Connect-RPC service. We
//! drive it the same way the web console does:
//!
//! `POST https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/QueryAccountBalance`
//! with the Connect protocol (`Connect-Protocol-Version: 1`,
//! `Content-Type: application/json`) and the body `{"biz_type":0}`.
//!
//! Auth is the `Oasis-Token` value, sent both as a cookie and as the
//! `Oasis-Token` header (the console reads the cookie and re-attaches it as a
//! header via a Connect interceptor). The console also sends `Oasis-appID` /
//! `Oasis-Platform`; we replay them for parity.
//!
//! Response (Connect JSON, protobuf camelCase) — all monetary fields are int64
//! in the platform's minor unit (see [`MINOR_UNIT_PER_YUAN`]):
//! `voucher`, `payment`, `balance`, `credit`, `costYesterday`, `costMonth`,
//! `costTotal`, `voucherApi`, `voucherPlan`, `voucherExpireTime`.

use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::cookie_jar::CookieEntry;
use crate::subscription::{MonetaryBalance, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const BALANCE_ENDPOINT: &str =
    "https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/QueryAccountBalance";

/// Web console app id (overseas console uses `20700`).
const OASIS_APP_ID: &str = "10300";

/// Console-internal monetary fields are int64 in a minor unit. The console
/// renders amounts in 元 (CNY); the platform's storage unit is the "厘"
/// (1 元 = 1000), which is the divisor the console's number formatting uses.
/// Isolated as a constant so a single edit corrects every amount if the unit
/// turns out to differ for a given account.
const MINOR_UNIT_PER_YUAN: f64 = 1000.0;

/// Connect-RPC business error code returned for auth failures
/// (`token is missing` / `token is illegal`).
const UNAUTHENTICATED_CODE: i64 = 120_000;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BalanceRes {
    #[serde(default)]
    voucher: i64,
    #[serde(default)]
    payment: i64,
    #[serde(default)]
    balance: i64,
    #[serde(default)]
    credit: i64,
    #[serde(default)]
    cost_yesterday: i64,
    #[serde(default)]
    cost_month: i64,
}

/// Connect-RPC error envelope (HTTP 4xx/5xx carry a JSON body like
/// `{"code":"unauthenticated","message":"...","details":[{"value":"...",
/// "debug":{"code":120000}}]}`).
#[derive(Debug, Default, Deserialize)]
struct ErrorEnvelope {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    details: Vec<ErrorDetail>,
}

#[derive(Debug, Default, Deserialize)]
struct ErrorDetail {
    #[serde(default)]
    debug: Option<ErrorDebug>,
}

#[derive(Debug, Default, Deserialize)]
struct ErrorDebug {
    #[serde(default)]
    code: Option<i64>,
}

fn extract_oasis_token(cookies: &[CookieEntry], cookie_header: &str) -> Option<String> {
    if let Some(c) = cookies
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case("Oasis-Token"))
    {
        let v = c.value.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    // Fall back to scanning the raw header (case-insensitive cookie name).
    cookie_header.split(';').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        if name.trim().eq_ignore_ascii_case("Oasis-Token") {
            let v = value.trim();
            (!v.is_empty()).then(|| v.to_string())
        } else {
            None
        }
    })
}

pub async fn fetch(
    subscription_id: &str,
    cookies: &[CookieEntry],
    cookie_header: &str,
) -> UsageResult<SubscriptionUsage> {
    let token = extract_oasis_token(cookies, cookie_header).ok_or_else(|| {
        UsageError::Fetcher(
            "未找到 Oasis-Token：请登录 platform.stepfun.com 后重新粘贴包含 Oasis-Token 的 Cookie。"
                .into(),
        )
    })?;

    let client = crate::http_client::usage_http_client()?;

    // biz_type 0 = unspecified (the console default for the overview page).
    let resp = client
        .post(BALANCE_ENDPOINT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("Connect-Protocol-Version", "1")
        .header(reqwest::header::COOKIE, cookie_header)
        .header("Oasis-Token", &token)
        .header("Oasis-appID", OASIS_APP_ID)
        .header("Oasis-Platform", "web")
        .json(&json!({ "biz_type": 0 }))
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("阶跃余额请求失败：{e}")))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(map_error_body(status.as_u16(), &body));
    }

    let data: BalanceRes = serde_json::from_str(&body)
        .map_err(|e| UsageError::Fetcher(format!("阶跃余额响应解析失败：{e}")))?;

    Ok(build_usage(subscription_id, &data))
}

/// Map a non-2xx Connect-RPC body to a `UsageError`, surfacing auth failures as
/// [`UsageError::AuthRequired`] so the UI prompts for a fresh Cookie.
fn map_error_body(http_status: u16, body: &str) -> UsageError {
    let env: ErrorEnvelope = serde_json::from_str(body).unwrap_or_default();

    let is_auth = http_status == 401
        || http_status == 403
        || env.code.as_deref() == Some("unauthenticated")
        || env
            .details
            .iter()
            .filter_map(|d| d.debug.as_ref().and_then(|dbg| dbg.code))
            .any(|c| c == UNAUTHENTICATED_CODE);

    if is_auth {
        return UsageError::AuthRequired;
    }

    let msg = env
        .message
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| body.chars().take(200).collect());
    UsageError::Fetcher(format!("阶跃余额返回 {http_status}：{msg}"))
}

fn to_yuan(minor: i64) -> f64 {
    minor as f64 / MINOR_UNIT_PER_YUAN
}

fn build_usage(subscription_id: &str, data: &BalanceRes) -> SubscriptionUsage {
    // `balance` is the headline total; `voucher` (赠送代金券) maps to `granted`,
    // `payment` (充值余额) maps to `topped_up`. `credit` is a separate credit
    // line — folded into the granted bucket so it is not lost in display.
    let balance = MonetaryBalance {
        currency: "CNY".to_string(),
        total: to_yuan(data.balance),
        granted: to_yuan(data.voucher + data.credit),
        topped_up: to_yuan(data.payment),
        is_available: Some(data.balance > 0),
    };

    // Surface spend as usage windows (used-only — there is no quota cap here).
    let yesterday = (data.cost_yesterday > 0).then(|| spend_window("昨日", data.cost_yesterday));
    let month = (data.cost_month > 0).then(|| spend_window("本月", data.cost_month));

    SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some("Step".to_string()),
        hourly: yesterday,
        weekly: None,
        monthly: month,
        balance: Some(balance),
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    }
}

/// A spend amount expressed as a used-only [`UsageWindow`] in 分 (cents), to
/// match how other monetary usage windows in this crate report `used`.
fn spend_window(label: &str, minor: i64) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used: (to_yuan(minor) * 100.0).round() as i64,
        total: None,
        percent: None,
        reset_at: None,
        breakdown: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(name: &str, value: &str) -> CookieEntry {
        CookieEntry {
            name: name.to_string(),
            value: value.to_string(),
            domain: Some(".stepfun.com".to_string()),
            path: Some("/".to_string()),
            expires: None,
            http_only: false,
            secure: true,
            source_url: None,
        }
    }

    #[test]
    fn extracts_token_from_cookie_entries() {
        let jar = vec![cookie("foo", "bar"), cookie("Oasis-Token", "tok123")];
        assert_eq!(
            extract_oasis_token(&jar, ""),
            Some("tok123".to_string())
        );
    }

    #[test]
    fn extracts_token_from_raw_header_case_insensitive() {
        let header = "foo=bar; oasis-token=hdr456; baz=qux";
        assert_eq!(
            extract_oasis_token(&[], header),
            Some("hdr456".to_string())
        );
    }

    #[test]
    fn missing_token_yields_none() {
        assert!(extract_oasis_token(&[cookie("foo", "bar")], "foo=bar").is_none());
        assert!(extract_oasis_token(&[cookie("Oasis-Token", "")], "").is_none());
    }

    #[test]
    fn parses_balance_response_and_converts_to_yuan() {
        // 12345 厘 = 12.345 元; voucher 2000 + credit 500 = 2.5 元 granted.
        let body = r#"{"voucher":2000,"payment":10345,"balance":12345,"credit":500,
            "costYesterday":1500,"costMonth":30000,"costTotal":99999}"#;
        let data: BalanceRes = serde_json::from_str(body).unwrap();
        let usage = build_usage("sub-1", &data);

        let bal = usage.balance.expect("balance present");
        assert_eq!(bal.currency, "CNY");
        assert!((bal.total - 12.345).abs() < 1e-9);
        assert!((bal.granted - 2.5).abs() < 1e-9);
        assert!((bal.topped_up - 10.345).abs() < 1e-9);
        assert_eq!(bal.is_available, Some(true));

        // costYesterday 1500 厘 = 1.5 元 = 150 分.
        assert_eq!(usage.hourly.as_ref().unwrap().used, 150);
        assert_eq!(usage.monthly.as_ref().unwrap().used, 3000);
        assert_eq!(usage.plan_name.as_deref(), Some("Step"));
    }

    #[test]
    fn zero_spend_windows_are_omitted() {
        let data: BalanceRes = serde_json::from_str(r#"{"balance":1000}"#).unwrap();
        let usage = build_usage("sub-1", &data);
        assert!(usage.hourly.is_none());
        assert!(usage.monthly.is_none());
    }

    #[test]
    fn auth_error_body_maps_to_auth_required() {
        let body = r#"{"code":"unauthenticated","message":"auth failed: token is illegal",
            "details":[{"debug":{"code":120000}}]}"#;
        assert!(matches!(map_error_body(401, body), UsageError::AuthRequired));
        // Even a 200-ish status with the business code should be treated as auth.
        assert!(matches!(map_error_body(500, body), UsageError::AuthRequired));
    }

    #[test]
    fn non_auth_error_body_maps_to_fetcher() {
        let body = r#"{"code":"internal","message":"boom"}"#;
        match map_error_body(500, body) {
            UsageError::Fetcher(m) => assert!(m.contains("boom")),
            other => panic!("expected Fetcher, got {other:?}"),
        }
    }
}

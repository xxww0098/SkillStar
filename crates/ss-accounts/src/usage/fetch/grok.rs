//! Grok（xAI）额度：计费代理的两个视图 + 套餐名。
//!
//! 端点与 Cockpit Tools 一致，三个 GET 互相独立，一个抖动不该抹掉另一个的收获：
//!
//! * GET https://cli-chat-proxy.grok.com/v1/billing            月度数字额度
//! * GET https://cli-chat-proxy.grok.com/v1/billing?format=credits  周进度与周期
//! * GET https://cli-chat-proxy.grok.com/v1/settings           套餐显示名
//!
//! CLI 会话令牌必须带 x-xai-token-auth: xai-grok-cli，否则代理直接拒绝；官方客户端
//! 每次代理调用都发它。解析全在 [super::grok_parse]（纯函数），这里只管网络与
//! 三态分类。
//!
//! 没搬过来的两件：Cockpit Tools 的 gRPC-Web 重置额度兑换（reset.rs）与
//! auth.json.lock 的刷新托管——前者是"花掉一次重置"，不属于账号域的五个动作；
//! 后者的目的（同一次刷新不被花两次）在这里由 store 的按 agent 刷新锁承担。

use serde_json::Value;

use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};

use super::grok_parse::{self as parse_grok, CurrentPeriod};
use super::parse;

const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing";
/// 同一个端点的另一个投影：给周进度与周期，不给月度数字。
const BILLING_CREDITS_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const SETTINGS_URL: &str = "https://cli-chat-proxy.grok.com/v1/settings";

/// 代理只认带这个声明的 CLI 会话令牌。
const TOKEN_AUTH_HEADER: &str = "x-xai-token-auth";
const TOKEN_AUTH_VALUE: &str = "xai-grok-cli";

const BILLING_LABEL: &str = "grok billing";
const CREDITS_LABEL: &str = "grok billing (credits)";
const SETTINGS_LABEL: &str = "grok settings";

/// Read the Grok quota for one account.
pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome> {
    // Grok 的刷新是普通的 refresh_token grant，不和 IDE 共享单一 grant，
    // 所以读之前可以按统一规则先刷新（与 codex/cursor 一致）。
    let reading = super::tokens_for_read("grok", creds)?;
    let now = super::unix_now();
    let access_token = reading.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err(UsageError::AuthRequired(
            "Grok 没有可用的 access token，请重新登录".to_string(),
        ));
    }
    let headers = headers(access_token);

    // 周进度先读：它决定"月度视图挂了还能不能出卡"。
    let period = credits_period(&headers);
    let billing = match billing_payload(&headers) {
        Ok(payload) => payload,
        Err(UsageError::AuthRequired(message)) => return Err(UsageError::AuthRequired(message)),
        // 周计划的账号靠周进度就能出卡：一次月度抖动不该让卡片变红。
        Err(error) if period.as_ref().and_then(|item| item.weekly) == Some(true) => {
            tracing::debug!(
                target: "accounts",
                error = %error,
                "Grok billing failed; the weekly view still describes the account"
            );
            Value::Null
        }
        Err(error) => return Err(error),
    };
    if !parse_grok::has_numbers(&billing, period.as_ref()) {
        return Err(UsageError::Unreadable(format!(
            "{BILLING_LABEL}: the reply carries no quota field: {}",
            parse::snippet(&billing.to_string())
        )));
    }

    let plan = settings_plan(&headers).or_else(|| parse_grok::plan_from_access_token(access_token));
    Ok(UsageOutcome {
        snapshot: parse_grok::snapshot_from(&billing, period.as_ref(), plan, now),
        tokens: reading.tokens,
        provider_state: reading.provider_state,
    })
}

fn headers(access_token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Authorization", format!("Bearer {access_token}")),
        ("Accept", "application/json".to_string()),
        (TOKEN_AUTH_HEADER, TOKEN_AUTH_VALUE.to_string()),
    ]
}

/// 默认计费视图；传输失败重试一次（代理偶尔抖），401 直接结束。
fn billing_payload(headers: &[(&str, String)]) -> UsageResult<Value> {
    let mut last: Option<UsageError> = None;
    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        match get_json(BILLING_LABEL, BILLING_URL, headers) {
            Ok(payload) => return Ok(payload),
            Err(UsageError::AuthRequired(message)) => {
                return Err(UsageError::AuthRequired(message));
            }
            Err(error @ UsageError::Transient(_)) => last = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last.unwrap_or_else(|| UsageError::Transient(format!("{BILLING_LABEL}: 请求失败"))))
}

/// credits 视图：只有失败与不可读两种结局，任何一种都退化成"没有周窗口"。
fn credits_period(headers: &[(&str, String)]) -> Option<CurrentPeriod> {
    let payload = get_json(CREDITS_LABEL, BILLING_CREDITS_URL, headers).ok()?;
    parse_grok::current_period(&payload)
}

/// 套餐显示名：读不到就是读不到，卡片退回令牌里的 tier。
fn settings_plan(headers: &[(&str, String)]) -> Option<String> {
    let payload = get_json(SETTINGS_LABEL, SETTINGS_URL, headers).ok()?;
    parse_grok_plan(&payload)
}

fn parse_grok_plan(payload: &Value) -> Option<String> {
    parse_grok::plan_from_settings(payload)
}

fn get_json(label: &str, url: &str, headers: &[(&str, String)]) -> UsageResult<Value> {
    let answer = super::http_get(label, url, headers)?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(label, answer.status, &answer.body));
    }
    serde_json::from_str(&answer.body).map_err(|error| {
        UsageError::Unreadable(format!(
            "{label}: cannot parse the reply: {error}: {}",
            parse::snippet(&answer.body)
        ))
    })
}

#[cfg(test)]
#[path = "grok_tests.rs"]
mod grok_tests;

//! Kiro 额度：CodeWhisperer getUsageLimits。
//!
//! GET {runtime}/getUsageLimits?origin=AI_EDITOR&profileArn=..&resourceType=AGENTIC_REQUEST&isEmailRequired=true
//!
//! runtime 跟区域走（[crate::oauth::agents::kiro::runtime_endpoint]），区域来自
//! provider_state，缺时看 profileArn 自己的区域，再缺就是 us-east-1。
//!
//! 两件与别家不同的事：
//!
//! * profileArn 是**必填**：没有它这个端点不返回任何额度。它由上一次读取（或从
//!   本机导入）写进 provider_state；这一次读回来就补上，下次直接用。
//! * 读响应顺带补 identity（email / userId）：Kiro 的 JWT 里不一定带 email，
//!   而 usage 响应里有，对账就靠它。

use serde_json::Value;

use crate::oauth::agents::kiro::{self, KiroState};
use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};

use super::kiro_parse;
use super::parse;

const USAGE_LABEL: &str = "kiro getUsageLimits";
const ORIGIN: &str = "AI_EDITOR";
const RESOURCE_TYPE: &str = "AGENTIC_REQUEST";

/// Read the Kiro quota for one account.
pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome> {
    // Kiro 的刷新是普通的 refresh grant（门户 refreshToken 或 IdC 注册），
    // 所以读之前可以按统一规则先刷新。
    let reading = super::tokens_for_read("kiro", creds)?;
    let now = super::unix_now();
    let access_token = reading.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err(UsageError::AuthRequired(
            "Kiro 没有可用的 access token，请重新登录".to_string(),
        ));
    }
    let state = KiroState::parse(
        reading
            .provider_state
            .as_ref()
            .or(creds.provider_state.as_ref()),
    );
    let region = state.region_or_default();
    let endpoint = kiro::runtime_endpoint(&region);
    let url = usage_url(&endpoint, state.profile_arn.as_deref())?;
    let headers = [
        ("Authorization", format!("Bearer {access_token}")),
        ("Accept", "application/json".to_string()),
    ];
    let answer = super::http_get(USAGE_LABEL, &url, &headers)?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(USAGE_LABEL, answer.status, &answer.body));
    }
    let body: Value = serde_json::from_str(&answer.body).map_err(|error| {
        UsageError::Unreadable(format!(
            "{USAGE_LABEL}: cannot parse the response: {error}: {}",
            parse::snippet(&answer.body)
        ))
    })?;
    let snapshot = kiro_parse::snapshot_from(&body, now);
    if snapshot.windows.is_empty() && snapshot.plan.is_none() {
        return Err(UsageError::Unreadable(format!(
            "{USAGE_LABEL}: the reply carries no quota field: {}",
            parse::snippet(&answer.body)
        )));
    }
    // 这一轮学到的 profileArn 与区域与身份一起回写，供对账与下次读取使用。
    let learned = learned_state(&state, &body, &region);
    let identity = kiro_parse::identity_of(&body);
    Ok(UsageOutcome {
        snapshot,
        tokens: reading.tokens,
        provider_state: learned.to_value(),
        identity,
    })
}

/// 用量地址。profileArn 缺失时**仍然发**（不带该参数）：有的账号端点能自己解析，
/// 带上空值反而被判无效。
pub(crate) fn usage_url(endpoint: &str, profile_arn: Option<&str>) -> UsageResult<String> {
    let mut url = url::Url::parse(&format!(
        "{}/getUsageLimits",
        endpoint.trim_end_matches('/')
    ))
    .map_err(|error| UsageError::Unreadable(format!("{USAGE_LABEL}: bad endpoint: {error}")))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("origin", ORIGIN)
            .append_pair("resourceType", RESOURCE_TYPE)
            .append_pair("isEmailRequired", "true");
        if let Some(profile_arn) = profile_arn.map(str::trim).filter(|value| !value.is_empty()) {
            pairs.append_pair("profileArn", profile_arn);
        }
    }
    Ok(url.to_string())
}

/// 这一轮之后该记下来的 provider_state：区域 + 学到的 profileArn。
fn learned_state(state: &KiroState, body: &Value, region: &str) -> KiroState {
    let mut next = state.clone();
    if next.region.as_deref().is_none_or(|current| !kiro::is_aws_region(current)) {
        next.region = Some(region.to_string());
    }
    if next.profile_arn.is_none() {
        next.profile_arn = kiro_parse::profile_arn(body);
    }
    next
}

#[cfg(test)]
#[path = "kiro_tests.rs"]
mod kiro_tests;

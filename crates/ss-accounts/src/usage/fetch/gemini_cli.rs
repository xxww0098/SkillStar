//! Gemini CLI 额度：Google Cloud Code Assist。
//!
//! 与 Antigravity 同一族端点，但**不是同一份客户端契约**：Gemini CLI 只认生产
//! 端点，metadata 里自报的是 IDE_UNSPECIFIED / PLATFORM_UNSPECIFIED，额度读的是
//! `retrieveUserQuota`（每模型的剩余份额桶）。
//!
//! 一次读取的次序：
//!
//! 1. `loadCodeAssist`（套餐、tier、项目 id、积分）
//! 2. 没有项目时 `onboardUser` 并轮询 operation
//! 3. `retrieveUserQuota`（每模型桶）
//! 4. 桶为空或端点不存在时退回 `fetchAvailableModels`
//!
//! 两条与别家不同的约束，理由和 [super::antigravity] 相同：
//!
//! * 这里**不兑换 refresh token**：同一个 Google grant 的 refresh 会让 Gemini CLI
//!   手上的登录态失效，刷新只走 accounts 的显式 refresh 路径。
//! * 项目 id 缓存进 provider_state（加密随记录落盘），带缓存被 400 拒绝时清掉
//!   缓存重试一次。

use serde_json::{Value, json};

use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};
use crate::usage::snapshot::UsageSnapshot;

use super::antigravity_parse::{self as ag_parse, BucketWindow};
use super::gemini_cli_parse;
use super::parse;

const LOAD_CODE_ASSIST_PATH: &str = "v1internal:loadCodeAssist";
const ONBOARD_USER_PATH: &str = "v1internal:onboardUser";
const RETRIEVE_USER_QUOTA_PATH: &str = "v1internal:retrieveUserQuota";
const AVAILABLE_MODELS_PATH: &str = "v1internal:fetchAvailableModels";

/// Gemini CLI 只走生产端点（它自己的 `CODE_ASSIST_ENDPOINT`）；daily 是
/// Antigravity 的构建期选择，不适用于这个产品。
const BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
const BASE_URL_ENV: &str = "SKILLSTAR_GEMINI_CLI_CLOUD_CODE_URL";

/// provider_state 里的项目 id 键（与 Antigravity 同名，但按账号各自存储）。
const PROJECT_STATE_KEY: &str = "project_id";

const ONBOARD_POLL_DELAY: std::time::Duration = std::time::Duration::from_millis(500);
/// onboardUser 的 operation 通常几轮内就完成；再多就是服务端卡住了。
const ONBOARD_MAX_POLLS: u32 = 20;

const LOAD_LABEL: &str = "gemini-cli loadCodeAssist";
const ONBOARD_LABEL: &str = "gemini-cli onboardUser";
const QUOTA_LABEL: &str = "gemini-cli retrieveUserQuota";
const MODELS_LABEL: &str = "gemini-cli fetchAvailableModels";

/// Read the Gemini CLI quota for one account.
pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome> {
    let now = super::unix_now();
    let access_token = creds.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err(UsageError::AuthRequired(
            "Gemini CLI 没有可用的 access token，请重新登录".to_string(),
        ));
    }
    // 过期不是"凭据没了"：这里拒绝兑换 refresh token，所以只能要求先走一次显式
    // 刷新。按 Unreadable 报，导入路径才会把这个账号原样存下来（而不是当成失效）。
    if creds.tokens.needs_refresh(now, 0) {
        return Err(UsageError::Unreadable(
            "Gemini CLI 的 access token 已过期：凭据会照原样保存，请刷新后再读额度"
                .to_string(),
        ));
    }
    let state = creds.provider_state.clone().unwrap_or(Value::Null);
    let mut project = cached_project(&state);
    let base = base_url();

    let (load, cleared) = match load_code_assist(access_token, &base, project.as_deref()) {
        Ok(load) => (load, false),
        Err(error) if project.is_some() && is_bad_request(&error) => {
            // 缓存的项目可能已经被回收：清掉缓存重试一次，而不是让卡片直接红。
            tracing::debug!(
                target: "accounts",
                "the cached Gemini CLI project was refused; retrying without it"
            );
            (load_code_assist(access_token, &base, None)?, true)
        }
        Err(error) => return Err(error),
    };
    if cleared {
        project = None;
    }
    if let Some(found) = load.project.clone() {
        project = Some(found);
    }
    if project.is_none()
        && let Some(tier) = load.onboard_tier.clone()
    {
        match onboard_project(access_token, &base, &tier) {
            Ok(found) => project = found,
            // onboardUser 失败不该让卡片报错：没有项目时桶通常照样能读。
            Err(error) => tracing::warn!(
                target: "accounts",
                error = %error,
                "Gemini CLI onboardUser failed; reading the quota without a project"
            ),
        }
    }
    let buckets = quota_buckets(access_token, &base, project.as_deref())?;
    Ok(UsageOutcome {
        snapshot: UsageSnapshot {
            fetched_at: now,
            windows: ag_parse::usage_windows(&buckets),
            plan: Some(load.plan),
            credits: load.credits,
            error: None,
            gcp_tos: None,
        },
        tokens: creds.tokens.clone(),
        provider_state: Some(provider_state(project.as_deref())),
    })
}

/// 覆盖变量：只给调试/自建代理用，空值等于没设。
fn base_url() -> String {
    std::env::var(BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string())
        .unwrap_or_else(|| BASE_URL.to_string())
}

// ---------------------------------------------------------------------------
// loadCodeAssist
// ---------------------------------------------------------------------------

fn load_code_assist(
    access_token: &str,
    base: &str,
    project: Option<&str>,
) -> UsageResult<ag_parse::PlanInfo> {
    let mut headers = headers(access_token);
    headers.push(("Accept", "*/*".to_string()));
    let answer = super::http_post_json(
        LOAD_LABEL,
        &format!("{base}/{LOAD_CODE_ASSIST_PATH}"),
        &headers,
        &load_code_assist_body(project),
    )?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(LOAD_LABEL, answer.status, &answer.body));
    }
    Ok(ag_parse::plan_info(&parse_body(LOAD_LABEL, &answer.body)?))
}

/// loadCodeAssist 的请求体。
pub(crate) fn load_code_assist_body(project: Option<&str>) -> Value {
    let mut body = json!({
        "metadata": metadata(project),
        "mode": "FULL_ELIGIBILITY_CHECK",
    });
    if let Some(project) = project.map(str::trim).filter(|id| !id.is_empty()) {
        body["cloudaicompanionProject"] = json!(project);
    }
    body
}

/// Cloud Code metadata。Gemini CLI 自报的是"通用 IDE"：它没有专属 ideType，
/// 用 Antigravity 的 ANTIGRAVITY 会被服务端按另一个产品的配额规则记账。
fn metadata(duet_project: Option<&str>) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("ideType".to_string(), json!("IDE_UNSPECIFIED"));
    metadata.insert("platform".to_string(), json!("PLATFORM_UNSPECIFIED"));
    metadata.insert("pluginType".to_string(), json!("GEMINI"));
    metadata.insert("pluginVersion".to_string(), json!(env!("CARGO_PKG_VERSION")));
    metadata.insert("updateChannel".to_string(), json!("stable"));
    if let Some(project) = duet_project.map(str::trim).filter(|id| !id.is_empty()) {
        metadata.insert("duetProject".to_string(), json!(project));
    }
    Value::Object(metadata)
}

// ---------------------------------------------------------------------------
// onboardUser
// ---------------------------------------------------------------------------

/// 这个账号没有项目时让服务端建一个，并轮询到 operation 完成。
fn onboard_project(access_token: &str, base: &str, tier: &str) -> UsageResult<Option<String>> {
    let body = json!({ "tierId": tier, "metadata": metadata(None) });
    let answer = super::http_post_json(
        ONBOARD_LABEL,
        &format!("{base}/{ONBOARD_USER_PATH}"),
        &headers(access_token),
        &body,
    )?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(ONBOARD_LABEL, answer.status, &answer.body));
    }
    let mut value = parse_body(ONBOARD_LABEL, &answer.body)?;
    for _ in 0..ONBOARD_MAX_POLLS {
        if value.get("done").and_then(Value::as_bool).unwrap_or(false) {
            return Ok(value
                .get("response")
                .and_then(ag_parse::response_project));
        }
        let name = parse::text(value.get("name")).ok_or_else(|| {
            UsageError::Unreadable(format!("{ONBOARD_LABEL}: unfinished operation without a name"))
        })?;
        std::thread::sleep(ONBOARD_POLL_DELAY);
        let answer = super::http_get(
            ONBOARD_LABEL,
            &format!("{base}/v1internal/{name}"),
            &headers(access_token),
        )?;
        if !(200..300).contains(&answer.status) {
            return Err(super::classify_status(ONBOARD_LABEL, answer.status, &answer.body));
        }
        value = parse_body(ONBOARD_LABEL, &answer.body)?;
    }
    Err(UsageError::Unreadable(format!(
        "{ONBOARD_LABEL}: the operation did not finish within {ONBOARD_MAX_POLLS} polls"
    )))
}

/// onboard 的一步（测试用：把"要轮询"和"已完成"两种响应分开驱动）。
#[cfg(test)]
pub(crate) fn onboard_step(value: &Value) -> UsageResult<ag_parse::OnboardStep> {
    if value.get("done").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(ag_parse::OnboardStep::Done(
            value.get("response").and_then(ag_parse::response_project),
        ));
    }
    let name = parse::text(value.get("name")).ok_or_else(|| {
        UsageError::Unreadable(format!("{ONBOARD_LABEL}: unfinished operation without a name"))
    })?;
    Ok(ag_parse::OnboardStep::Pending(name))
}

// ---------------------------------------------------------------------------
// 桶
// ---------------------------------------------------------------------------

/// 先读每模型桶；桶为空或端点不支持时退回模型明细。
fn quota_buckets(
    access_token: &str,
    base: &str,
    project: Option<&str>,
) -> UsageResult<Vec<BucketWindow>> {
    match user_quota(access_token, base, project) {
        Ok(buckets) if !buckets.is_empty() => Ok(buckets),
        Ok(_) => available_models(access_token, base, project),
        Err(error) if is_endpoint_missing(&error) => available_models(access_token, base, project),
        Err(error) => Err(error),
    }
}

fn user_quota(
    access_token: &str,
    base: &str,
    project: Option<&str>,
) -> UsageResult<Vec<BucketWindow>> {
    let answer = super::http_post_json(
        QUOTA_LABEL,
        &format!("{base}/{RETRIEVE_USER_QUOTA_PATH}"),
        &headers(access_token),
        &project_body(project),
    )?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(QUOTA_LABEL, answer.status, &answer.body));
    }
    Ok(gemini_cli_parse::quota_windows(&parse_body(
        QUOTA_LABEL,
        &answer.body,
    )?))
}

fn available_models(
    access_token: &str,
    base: &str,
    project: Option<&str>,
) -> UsageResult<Vec<BucketWindow>> {
    let answer = super::http_post_json(
        MODELS_LABEL,
        &format!("{base}/{AVAILABLE_MODELS_PATH}"),
        &headers(access_token),
        &project_body(project),
    )?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(MODELS_LABEL, answer.status, &answer.body));
    }
    // 模型明细与 Antigravity 是同一份 JSON（models[].quotaInfo.remainingFraction）。
    Ok(ag_parse::model_windows(&parse_body(MODELS_LABEL, &answer.body)?))
}

fn project_body(project: Option<&str>) -> Value {
    match project.map(str::trim).filter(|id| !id.is_empty()) {
        Some(project) => json!({ "project": project }),
        None => json!({}),
    }
}

fn parse_body(label: &str, body: &str) -> UsageResult<Value> {
    serde_json::from_str(body).map_err(|error| {
        UsageError::Unreadable(format!(
            "{label}: cannot parse the response: {error}: {}",
            parse::snippet(body)
        ))
    })
}

// ---------------------------------------------------------------------------
// provider_state
// ---------------------------------------------------------------------------

fn cached_project(state: &Value) -> Option<String> {
    parse::text(state.get(PROJECT_STATE_KEY))
}

/// 项目 id 进 provider_state（加密随记录落盘），下次读额度直接用。
pub(crate) fn provider_state(project: Option<&str>) -> Value {
    let mut state = serde_json::Map::new();
    if let Some(project) = project.map(str::trim).filter(|id| !id.is_empty()) {
        state.insert(PROJECT_STATE_KEY.to_string(), json!(project));
    }
    Value::Object(state)
}

// ---------------------------------------------------------------------------
// 协助
// ---------------------------------------------------------------------------

fn headers(access_token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Authorization", format!("Bearer {access_token}")),
        ("User-Agent", user_agent()),
        ("x-goog-api-client", "gl-node/22.21.1".to_string()),
        ("Content-Type", "application/json".to_string()),
    ]
}

pub(crate) fn user_agent() -> String {
    format!(
        "GeminiCLI/1.0.0 ({}; {})",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// 带缓存项目时的 400：缓存的项目可能已经不归这个账号了。
fn is_bad_request(error: &UsageError) -> bool {
    match error {
        UsageError::Unreadable(message) => {
            message.contains("HTTP 400") || message.contains("Bad Request")
        }
        _ => false,
    }
}

/// 桶端点在这个 base 上不存在（404/405）：模型明细是兼容兜底。
fn is_endpoint_missing(error: &UsageError) -> bool {
    match error {
        UsageError::Unreadable(message) => {
            message.contains("HTTP 404") || message.contains("HTTP 405")
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "gemini_cli_tests.rs"]
mod gemini_cli_tests;

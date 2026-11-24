//! Antigravity 额度：Google Cloud Code Assist。
//!
//! 一次读取的次序与 Cockpit Tools 一致：`loadCodeAssist`（套餐、ToS、积分、
//! 项目 id）→ 需要时 `onboardUser` 并轮询 operation → `retrieveUserQuotaSummary`
//! （组/桶的 5h、weekly 窗口）→ 汇总为空或端点不支持时退回
//! `fetchAvailableModels`（per-model 剩余份额）。
//!
//! 两个跟别家不同的约束：
//!
//! * 这里**不兑换 refresh token**。同一个 Google grant 的 refresh 会让 IDE 手上
//!   的那份登录态失效，刷新只走 accounts 的显式 refresh 路径；额度读取只用
//!   手上的 access token。
//! * 项目 id 会被缓存进 provider_state（加密随记录落盘），带缓存请求被 400
//!   拒绝时清掉缓存重试一次。

use std::time::Duration;

use serde_json::{Value, json};

use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};
use crate::usage::snapshot::UsageSnapshot;

use super::antigravity_parse::{self as parse_ag, BucketWindow, OnboardStep};
use super::parse;

const LOAD_CODE_ASSIST_PATH: &str = "v1internal:loadCodeAssist";
const ONBOARD_USER_PATH: &str = "v1internal:onboardUser";
const QUOTA_SUMMARY_PATH: &str = "v1internal:retrieveUserQuotaSummary";
const AVAILABLE_MODELS_PATH: &str = "v1internal:fetchAvailableModels";

/// daily 是默认端点；只有 GCP ToS 账号才去生产端点。
const DAILY_BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
const PROD_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
const BASE_URL_ENV: &str = "SKILLSTAR_ANTIGRAVITY_CLOUD_CODE_URL";

/// Cloud Code 请求里自报的 IDE 版本与 node client 版本。不去探测用户装的是
/// 哪个版本：探测带来的收益远小于在授权路径上多读一个文件的风险。
const IDE_VERSION: &str = "2.25.0";
const X_GOOG_API_CLIENT: &str = "gl-node/22.21.1";

/// provider_state 里的两个键。
const PROJECT_STATE_KEY: &str = "project_id";
const GCP_TOS_STATE_KEY: &str = "is_gcp_tos";

const ONBOARD_POLL_DELAY: Duration = Duration::from_millis(500);
/// onboardUser 的 operation 通常几轮内就完成；再多就是服务端卡住了。
const ONBOARD_MAX_POLLS: u32 = 20;

const LOAD_LABEL: &str = "antigravity loadCodeAssist";
const ONBOARD_LABEL: &str = "antigravity onboardUser";
const SUMMARY_LABEL: &str = "antigravity retrieveUserQuotaSummary";
const MODELS_LABEL: &str = "antigravity fetchAvailableModels";

/// Read the Antigravity quota for one account.
pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome> {
    fetch_with_identity(creds, |token| {
        crate::oauth::agents::antigravity::read_identity_blocking(token).ok()
    })
}

fn fetch_with_identity(
    creds: &Credentials,
    read_identity: impl FnOnce(&str) -> Option<crate::oauth::Identity>,
) -> UsageResult<UsageOutcome> {
    let now = super::unix_now();
    let access_token = creds.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err(UsageError::AuthRequired(
            "Antigravity 没有可用的 access token，请重新登录".to_string(),
        ));
    }
    // 过期不是"凭据没了"：这里拒绝兑换 refresh token，所以只能要求先走一次显式
    // 刷新。按 Unreadable 报，导入路径才会把这个账号原样存下来（而不是当成失效）。
    if creds.tokens.needs_refresh(now, 0) {
        return Err(UsageError::Unreadable(
            "Antigravity 的 access token 已过期：凭据会照原样保存，请刷新后再读额度"
                .to_string(),
        ));
    }
    let state = creds.provider_state.clone().unwrap_or(Value::Null);
    let mut project = cached_project(&state);
    let cached_tos = cached_gcp_tos(&state);
    let mut base = select_base_url(base_url_override().as_deref(), cached_tos.unwrap_or(false), project.as_deref());

    let (load, cleared) = match load_code_assist(access_token, &base, project.as_deref()) {
        Ok(load) => (load, false),
        Err(error) if project.is_some() && is_bad_request(&error) => {
            // 缓存的项目可能已经被回收：清掉缓存重试一次，而不是让卡片直接红。
            tracing::debug!(
                target: "accounts",
                "the cached Antigravity project was refused; retrying without it"
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
    // 端点跟着项目走：清掉缓存后重新选一次。
    base = select_base_url(base_url_override().as_deref(), load.gcp_tos.or(cached_tos).unwrap_or(false), project.as_deref());
    if project.is_none()
        && let Some(tier) = load.onboard_tier.clone()
    {
        match onboard_project(access_token, &base, &tier, None) {
            Ok(found) => project = found,
            // onboardUser 失败不该让卡片报错：没有项目时组/桶通常照样能读。
            Err(error) => tracing::warn!(
                target: "accounts",
                error = %error,
                "Antigravity onboardUser failed; reading the quota without a project"
            ),
        }
    }
    let buckets = quota_buckets(access_token, &base, project.as_deref())?;
    let gcp_tos = load.gcp_tos.or(cached_tos);
    Ok(UsageOutcome {
        snapshot: UsageSnapshot {
            fetched_at: now,
            windows: parse_ag::usage_windows(&buckets),
            plan: Some(load.plan),
            credits: load.credits,
            error: None,
            gcp_tos,
            reset_credits: None,
        },
        tokens: creds.tokens.clone(),
        provider_state: Some(provider_state(project.as_deref(), gcp_tos)),
        identity: if creds.needs_identity {
            read_identity(access_token)
        } else {
            None
        },
    })
}

// ---------------------------------------------------------------------------
// 端点选择
// ---------------------------------------------------------------------------

/// 覆盖变量：只给调试/自建代理用，空值等于没设。
fn base_url_override() -> Option<String> {
    std::env::var(BASE_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 路由顺序与 Cockpit Tools 一致：override → GCP ToS（且已识别项目）→ daily。
/// 真机运维更关心的"Google 内部构建走 sandbox"分支没有搬过来：它只对
/// `ANTIGRAVITY_*_INSIDER` 环境成立，而这个应用不是那个构建。
pub(crate) fn select_base_url(
    override_url: Option<&str>,
    is_gcp_tos: bool,
    project: Option<&str>,
) -> String {
    if let Some(url) = override_url.map(str::trim).filter(|url| !url.is_empty()) {
        return url.trim_end_matches('/').to_string();
    }
    if is_gcp_tos && project.map(str::trim).is_some_and(|id| !id.is_empty()) {
        return PROD_BASE_URL.to_string();
    }
    DAILY_BASE_URL.to_string()
}

// ---------------------------------------------------------------------------
// loadCodeAssist
// ---------------------------------------------------------------------------

fn load_code_assist(
    access_token: &str,
    base: &str,
    project: Option<&str>,
) -> UsageResult<parse_ag::PlanInfo> {
    let mut headers = headers(access_token);
    // loadCodeAssist 额外声明接受任意响应类型（Cockpit Tools 就是这么发的）。
    headers.push(("Accept", "*/*".to_string()));
    let body = load_code_assist_body(project);
    let answer = super::http_post_json(
        LOAD_LABEL,
        &format!("{base}/{LOAD_CODE_ASSIST_PATH}"),
        &headers,
        &body,
    )?;
    interpret_load(answer.status, &answer.body)
}

/// loadCodeAssist 的回答：非 2xx 交给共用分类表，2xx 交给纯解析。
pub(crate) fn interpret_load(status: u16, body: &str) -> UsageResult<parse_ag::PlanInfo> {
    if !(200..300).contains(&status) {
        return Err(super::classify_status(LOAD_LABEL, status, body));
    }
    let value: Value = serde_json::from_str(body).map_err(|error| {
        UsageError::Unreadable(format!(
            "{LOAD_LABEL}: cannot parse the response: {error}: {}",
            parse::snippet(body)
        ))
    })?;
    Ok(parse_ag::plan_info(&value))
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

/// Cloud Code metadata：IDE 身份 + 插件版本 + 平台。`duetProject` 只在已经
/// 有项目时才带，否则服务端会把它当成一个不存在的项目。
fn metadata(duet_project: Option<&str>) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("ideName".to_string(), json!("antigravity"));
    metadata.insert("ideType".to_string(), json!("ANTIGRAVITY"));
    metadata.insert("ideVersion".to_string(), json!(IDE_VERSION));
    metadata.insert("pluginVersion".to_string(), json!(env!("CARGO_PKG_VERSION")));
    metadata.insert("platform".to_string(), json!(platform_name()));
    metadata.insert("updateChannel".to_string(), json!("stable"));
    metadata.insert("pluginType".to_string(), json!("GEMINI"));
    if let Some(project) = duet_project.map(str::trim).filter(|id| !id.is_empty()) {
        metadata.insert("duetProject".to_string(), json!(project));
    }
    Value::Object(metadata)
}

fn platform_name() -> &'static str {
    match (os_name(), arch_name()) {
        ("darwin", "amd64") => "DARWIN_AMD64",
        ("darwin", "arm64") => "DARWIN_ARM64",
        ("linux", "amd64") => "LINUX_AMD64",
        ("linux", "arm64") => "LINUX_ARM64",
        ("windows", "amd64") => "WINDOWS_AMD64",
        _ => "PLATFORM_UNSPECIFIED",
    }
}

fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "windows",
        "linux" => "linux",
        other => other,
    }
}

fn arch_name() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// onboardUser
// ---------------------------------------------------------------------------

/// 这个账号没有项目时让服务端建一个，并轮询到 operation 完成。
fn onboard_project(
    access_token: &str,
    base: &str,
    tier: &str,
    project: Option<&str>,
) -> UsageResult<Option<String>> {
    let mut body = json!({ "tierId": tier, "metadata": metadata(project) });
    if let Some(project) = project.map(str::trim).filter(|id| !id.is_empty()) {
        body["cloudaicompanionProject"] = json!(project);
    }
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
        match onboard_step(&value)? {
            OnboardStep::Done(project) => return Ok(project),
            OnboardStep::Pending(name) => {
                std::thread::sleep(ONBOARD_POLL_DELAY);
                let answer = super::http_get(
                    ONBOARD_LABEL,
                    &format!("{base}/v1internal/{name}"),
                    &headers(access_token),
                )?;
                if !(200..300).contains(&answer.status) {
                    return Err(super::classify_status(
                        ONBOARD_LABEL,
                        answer.status,
                        &answer.body,
                    ));
                }
                value = parse_body(ONBOARD_LABEL, &answer.body)?;
            }
        }
    }
    Err(UsageError::Unreadable(format!(
        "{ONBOARD_LABEL}: the operation did not finish within {ONBOARD_MAX_POLLS} polls"
    )))
}

/// 一次 onboardUser 响应：还要轮询，或者已经给出项目 id。
pub(crate) fn onboard_step(value: &Value) -> UsageResult<OnboardStep> {
    if value.get("done").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(OnboardStep::Done(
            value
                .get("response")
                .and_then(parse_ag::response_project),
        ));
    }
    let name = parse::text(value.get("name")).ok_or_else(|| {
        UsageError::Unreadable(format!("{ONBOARD_LABEL}: unfinished operation without a name"))
    })?;
    Ok(OnboardStep::Pending(name))
}

// ---------------------------------------------------------------------------
// 组/桶与模型明细
// ---------------------------------------------------------------------------

/// 先读组/桶窗口；汇总为空或端点不支持时退回模型明细。
fn quota_buckets(
    access_token: &str,
    base: &str,
    project: Option<&str>,
) -> UsageResult<Vec<BucketWindow>> {
    match quota_summary(access_token, base, project) {
        Ok(buckets) if !buckets.is_empty() => Ok(buckets),
        Ok(_) => available_models(access_token, base, project),
        Err(error) if is_endpoint_missing(&error) => available_models(access_token, base, project),
        Err(error) => Err(error),
    }
}

fn quota_summary(
    access_token: &str,
    base: &str,
    project: Option<&str>,
) -> UsageResult<Vec<BucketWindow>> {
    let answer = super::http_post_json(
        SUMMARY_LABEL,
        &format!("{base}/{QUOTA_SUMMARY_PATH}"),
        &headers(access_token),
        &project_body(project),
    )?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(SUMMARY_LABEL, answer.status, &answer.body));
    }
    Ok(parse_ag::summary_windows(&parse_body(SUMMARY_LABEL, &answer.body)?))
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
    Ok(parse_ag::model_windows(&parse_body(MODELS_LABEL, &answer.body)?))
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

fn cached_gcp_tos(state: &Value) -> Option<bool> {
    state.get(GCP_TOS_STATE_KEY).and_then(Value::as_bool)
}

/// 项目 id 与 ToS 标记都进 provider_state（加密随记录落盘），下次读额度直接
/// 用，不再从头问一遍。
pub(crate) fn provider_state(project: Option<&str>, gcp_tos: Option<bool>) -> Value {
    let mut state = serde_json::Map::new();
    if let Some(project) = project.map(str::trim).filter(|id| !id.is_empty()) {
        state.insert(PROJECT_STATE_KEY.to_string(), json!(project));
    }
    if gcp_tos == Some(true) {
        state.insert(GCP_TOS_STATE_KEY.to_string(), json!(true));
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
        ("x-goog-api-client", X_GOOG_API_CLIENT.to_string()),
        ("Content-Type", "application/json".to_string()),
    ]
}

/// `antigravity/{ver} {os}/{arch}`。
pub(crate) fn user_agent() -> String {
    format!(
        "antigravity/{IDE_VERSION} {}/{}",
        os_name(),
        arch_name()
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

/// 汇总端点在这个 base 上不存在（404/405）：模型明细是兼容兜底。
fn is_endpoint_missing(error: &UsageError) -> bool {
    match error {
        UsageError::Unreadable(message) => message.contains("HTTP 404") || message.contains("HTTP 405"),
        _ => false,
    }
}

#[cfg(test)]
#[path = "antigravity_tests.rs"]
mod antigravity_tests;

//! Kiro OAuth fetcher (PKCE + local callback, derived from cockpit-tools `kiro_oauth.rs`).
//!
//! 1. Open `https://app.kiro.dev/signin` with PKCE + `redirect_uri=http://127.0.0.1:{port}`.
//! 2. Local server catches `/oauth/callback` or `/signin/callback` with `code` + `state`.
//! 3. POST `https://prod.us-east-1.auth.desktop.kiro.dev/oauth/token` to swap tokens.
//! 4. GET `https://q.{region}.amazonaws.com/getUsageLimits` for credits usage.
//!
//! v1 supports Google / GitHub browser login only (Builder ID / IdC need the desktop client).

use base64::Engine;
use chrono::Utc;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Cursor;
use std::time::Duration;
use tiny_http::{Header, Response, Server};
use tokio::sync::oneshot;
use tokio::task;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::pkce::PkcePair;
use crate::oauth::token_refresh;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const KIRO_AUTH_PORTAL_URL: &str = "https://app.kiro.dev/signin";
const KIRO_TOKEN_ENDPOINT: &str = "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/token";
const KIRO_REFRESH_ENDPOINT: &str = "https://prod.us-east-1.auth.desktop.kiro.dev/refreshToken";
const KIRO_RUNTIME_DEFAULT_ENDPOINT: &str = "https://q.us-east-1.amazonaws.com";
const OAUTH_TIMEOUT: Duration = Duration::from_secs(600);
const CALLBACK_PORTS: [u16; 10] = [
    3128, 4649, 6588, 8008, 9091, 49153, 50153, 51153, 52153, 53153,
];

#[derive(Debug, Clone)]
struct OAuthCallbackData {
    login_option: String,
    code: Option<String>,
    path: String,
}

pub async fn start_login(_region: Option<&str>) -> UsageResult<super::OAuthStartInfo> {
    let callback_port = find_callback_port()?;
    let callback_url = format!("http://127.0.0.1:{}", callback_port);
    let state_token = random_token();
    let pkce = PkcePair::generate();
    let auth_url = format!(
        "{}?state={}&code_challenge={}&code_challenge_method=S256&redirect_uri={}&redirect_from=KiroIDE",
        KIRO_AUTH_PORTAL_URL,
        urlencoding(&state_token),
        urlencoding(&pkce.challenge),
        urlencoding(&callback_url),
    );

    let pending_id = crate::oauth::pending_state::register("kiro", None, auth_url.clone());
    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    tokio::spawn(async move {
        let result = drive_login(callback_port, state_token, verifier, callback_url).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::OAuthStartInfo::browser(auth_url, pending_id))
}

async fn drive_login(
    callback_port: u16,
    state_token: String,
    code_verifier: String,
    callback_url: String,
) -> UsageResult<Subscription> {
    let callback = wait_for_kiro_callback(callback_port, state_token, OAUTH_TIMEOUT).await?;

    if callback.code.is_none() {
        let reason = match callback.login_option.as_str() {
            "builderid" | "awsidc" | "internal" => {
                "当前登录方式需要 Kiro 客户端后续认证，请改用 Google 或 GitHub 登录。"
            }
            "external_idp" => "External IdP 登录未返回授权 code，暂不支持自动导入。",
            _ => "回调缺少授权 code，无法完成登录。",
        };
        return Err(UsageError::Other(reason.into()));
    }

    let redirect_uri = build_token_exchange_redirect_uri(&callback_url, &callback);
    let auth_token = exchange_code_for_token(&callback, &code_verifier, &redirect_uri).await?;
    finalize_subscription(auth_token).await
}

async fn wait_for_kiro_callback(
    port: u16,
    expected_state: String,
    timeout: Duration,
) -> UsageResult<OAuthCallbackData> {
    let (tx, rx) = oneshot::channel();
    let state = expected_state.clone();
    let join = task::spawn_blocking(move || {
        let server = match Server::http(format!("127.0.0.1:{}", port)) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(Err(UsageError::Other(format!(
                    "无法监听本地回调端口 {}: {}",
                    port, e
                ))));
                return;
            }
        };
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if std::time::Instant::now() >= deadline {
                let _ = tx.send(Err(UsageError::Other(
                    "等待 Kiro 登录超时，请重新发起授权".into(),
                )));
                break;
            }
            match server.recv_timeout(Duration::from_millis(200)) {
                Ok(Some(request)) => {
                    let outcome = parse_kiro_callback_request(request.url(), &state);
                    let body = r#"<!doctype html><html><head><meta charset="utf-8"><title>Kiro</title></head>
<body style="font-family:system-ui;text-align:center;padding:3rem;background:#0b1020;color:#e7e9ee">
<p>登录完成，可关闭此窗口返回 SkillStar。</p></body></html>"#;
                    let resp = Response::new(
                        200.into(),
                        vec![
                            Header::from_bytes(
                                &b"Content-Type"[..],
                                &b"text/html; charset=utf-8"[..],
                            )
                            .unwrap(),
                        ],
                        Cursor::new(body.as_bytes().to_vec()),
                        Some(body.len()),
                        None,
                    );
                    let _ = request.respond(resp);
                    let _ = tx.send(outcome);
                    break;
                }
                Ok(None) => continue,
                Err(e) => {
                    let _ = tx.send(Err(UsageError::Other(format!(
                        "Kiro OAuth 回调服务异常: {}",
                        e
                    ))));
                    break;
                }
            }
        }
    });

    let result = rx
        .await
        .map_err(|_| UsageError::Other("Kiro OAuth 回调通道已关闭".into()))?;
    let _ = join.await;
    result
}

fn parse_kiro_callback_request(url: &str, expected_state: &str) -> UsageResult<OAuthCallbackData> {
    let (path, query) = match url.split_once('?') {
        Some((p, q)) => (p, q),
        None => (url, ""),
    };
    if path != "/oauth/callback" && path != "/signin/callback" {
        return Err(UsageError::Other(format!("未知回调路径: {}", path)));
    }

    let params = parse_query_params(query);
    if let Some(err) = params.get("error") {
        let desc = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or("");
        let msg = if desc.is_empty() {
            format!("授权失败: {}", err)
        } else {
            format!("授权失败: {} ({})", err, desc)
        };
        return Err(UsageError::Other(msg));
    }

    let callback_state = params.get("state").map(String::as_str).unwrap_or("");
    if callback_state.is_empty() || callback_state != expected_state {
        return Err(UsageError::Other(
            "授权 state 校验失败，请重新发起登录".into(),
        ));
    }

    let login_option = params
        .get("login_option")
        .or_else(|| params.get("loginOption"))
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    Ok(OAuthCallbackData {
        login_option,
        code: params.get("code").cloned().filter(|s| !s.trim().is_empty()),
        path: path.to_string(),
    })
}

async fn exchange_code_for_token(
    callback: &OAuthCallbackData,
    code_verifier: &str,
    redirect_uri: &str,
) -> UsageResult<Value> {
    let code = callback
        .code
        .as_deref()
        .ok_or_else(|| UsageError::Other("Kiro 回调缺少 code".into()))?;

    let client = http_client()?;
    let resp = client
        .post(KIRO_TOKEN_ENDPOINT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&json!({
            "code": code,
            "code_verifier": code_verifier,
            "redirect_uri": redirect_uri,
        }))
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Kiro oauth/token 请求失败: {}", e)))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_else(|_| String::new());
    if !status.is_success() {
        return Err(UsageError::Fetcher(format!(
            "Kiro oauth/token 状态码 {} (body_len={})",
            status,
            body.len()
        )));
    }

    let mut token: Value = serde_json::from_str(&body)
        .map_err(|e| UsageError::Fetcher(format!("解析 Kiro token 响应失败: {}", e)))?;
    if let Some(data) = token.get("data").filter(|v| v.is_object()).cloned() {
        token = data;
    }
    if !callback.login_option.is_empty()
        && let Some(obj) = token.as_object_mut()
    {
        obj.entry("login_option")
            .or_insert_with(|| Value::String(callback.login_option.clone()));
    }
    ensure_expires_at_from_expires_in(&mut token);
    Ok(token)
}

async fn finalize_subscription(auth_token: Value) -> UsageResult<Subscription> {
    let access_token = pick_string(
        Some(&auth_token),
        &[
            &["accessToken"],
            &["access_token"],
            &["token"],
            &["idToken"],
            &["id_token"],
        ],
    )
    .ok_or_else(|| UsageError::Other("Kiro 缺少 access token".into()))?;

    let refresh_token = pick_string(Some(&auth_token), &[&["refreshToken"], &["refresh_token"]]);
    let profile_arn = extract_profile_arn(Some(&auth_token), None);
    let email = normalize_email(pick_string(
        Some(&auth_token),
        &[&["email"], &["login_hint"], &["loginHint"]],
    ));
    let display = email.unwrap_or_else(|| "Kiro".to_string());
    let expires_at = parse_timestamp(
        get_path_value(&auth_token, &["expiresAt"])
            .or_else(|| get_path_value(&auth_token, &["expires_at"])),
    )
    .or_else(|| {
        pick_number(Some(&auth_token), &[&["expiresIn"], &["expires_in"]])
            .map(|s| Utc::now().timestamp() + s.round() as i64)
    })
    .or_else(|| token_refresh::jwt_exp(&access_token));

    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: "kiro".to_string(),
        display_name: display,
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: "USD".to_string(),
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(&access_token)),
        refresh_token_encrypted: refresh_token.as_deref().map(crypto::encrypt),
        access_token_expires_at: expires_at,
        oauth_account_id: profile_arn,
        oauth_region: None,
        requires_reauth: false,
        fingerprint_id: None,
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };

    if let Ok(usage) = fetch_usage_for_subscription(&sub).await {
        storage::save_usage_snapshot(usage).ok();
    }

    storage::upsert_subscription(sub.clone())
        .map_err(|e| UsageError::Other(format!("Kiro 订阅保存失败: {}", e)))
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let fp_id = subscription.fingerprint_id.clone();
    crate::http_client::with_fingerprint(fp_id, fetch_inner(subscription)).await
}

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let mut access_token = decrypt_required(&subscription.access_token_encrypted)?;

    if token_refresh::needs_refresh(subscription.access_token_expires_at)
        && let Some(rt_cipher) = subscription.refresh_token_encrypted.as_deref()
    {
        let refresh_token = crypto::decrypt(rt_cipher);
        if !refresh_token.is_empty()
            && let Ok((at, rt_new, profile_arn, expires_at)) =
                refresh_access_token(&refresh_token).await
        {
            subscription.access_token_encrypted = Some(crypto::encrypt(&at));
            subscription.access_token_expires_at = expires_at;
            if let Some(rt) = rt_new {
                subscription.refresh_token_encrypted = Some(crypto::encrypt(&rt));
            }
            if let Some(arn) = profile_arn {
                subscription.oauth_account_id = Some(arn);
            }
            access_token = at;
        }
    }

    fetch_usage_with_token(subscription, &access_token).await
}

async fn fetch_usage_for_subscription(sub: &Subscription) -> UsageResult<SubscriptionUsage> {
    let access_token = decrypt_required(&sub.access_token_encrypted)?;
    fetch_usage_with_token(sub, &access_token).await
}

async fn fetch_usage_with_token(
    sub: &Subscription,
    access_token: &str,
) -> UsageResult<SubscriptionUsage> {
    let profile_arn = sub
        .oauth_account_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| UsageError::Other("缺少 profile ARN，请重新 OAuth 登录".into()))?;

    let usage_json = fetch_usage_limits(access_token, profile_arn).await?;
    Ok(build_subscription_usage(&sub.id, &usage_json))
}

async fn refresh_access_token(
    refresh_token: &str,
) -> UsageResult<(String, Option<String>, Option<String>, Option<i64>)> {
    let client = http_client()?;
    let resp = client
        .post(KIRO_REFRESH_ENDPOINT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&json!({ "refreshToken": refresh_token }))
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Kiro refreshToken 失败: {}", e)))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(UsageError::AuthRequired);
        }
        return Err(UsageError::Fetcher(format!(
            "Kiro refreshToken 状态码 {}",
            status
        )));
    }

    let mut token: Value = serde_json::from_str(&body)
        .map_err(|e| UsageError::Fetcher(format!("解析 refresh 响应: {}", e)))?;
    if let Some(data) = token.get("data").filter(|v| v.is_object()).cloned() {
        token = data;
    }
    ensure_expires_at_from_expires_in(&mut token);

    let access_token = pick_string(
        Some(&token),
        &[
            &["accessToken"],
            &["access_token"],
            &["token"],
            &["idToken"],
            &["id_token"],
        ],
    )
    .ok_or(UsageError::AuthRequired)?;

    let refresh_new = pick_string(Some(&token), &[&["refreshToken"], &["refresh_token"]]);
    let profile_arn = extract_profile_arn(Some(&token), None);
    let expires_at = parse_timestamp(
        get_path_value(&token, &["expiresAt"]).or_else(|| get_path_value(&token, &["expires_at"])),
    )
    .or_else(|| token_refresh::jwt_exp(&access_token));

    Ok((access_token, refresh_new, profile_arn, expires_at))
}

async fn fetch_usage_limits(access_token: &str, profile_arn: &str) -> UsageResult<Value> {
    let region = parse_profile_arn_region(profile_arn);
    let endpoint = runtime_endpoint_for_region(region.as_deref());
    let url = format!(
        "{}/getUsageLimits?origin=AI_EDITOR&profileArn={}&resourceType=AGENTIC_REQUEST&isEmailRequired=true",
        endpoint.trim_end_matches('/'),
        urlencoding(profile_arn),
    );

    let client = http_client()?;
    let resp = client
        .get(&url)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", access_token.trim()),
        )
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Kiro usage 请求失败: {}", e)))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        return Err(UsageError::Fetcher(format!(
            "Kiro 账号受限: {}",
            parse_runtime_error_reason(&body).unwrap_or(body)
        )));
    }
    if !status.is_success() {
        return Err(UsageError::Fetcher(format!(
            "Kiro usage 状态码 {} (body_len={})",
            status,
            body.len()
        )));
    }

    serde_json::from_str(&body)
        .map_err(|e| UsageError::Fetcher(format!("解析 Kiro usage JSON: {}", e)))
}

fn build_subscription_usage(subscription_id: &str, usage: &Value) -> SubscriptionUsage {
    let (
        plan_name,
        _plan_tier,
        credits_total,
        credits_used,
        bonus_total,
        bonus_used,
        usage_reset_at,
        _bonus_expire_days,
    ) = extract_usage_fields(Some(usage));

    let monthly = credits_total.map(|total| {
        let used = credits_used.unwrap_or(0.0).round() as i64;
        let total_i = total.round() as i64;
        let percent =
            (total_i > 0).then(|| ((used as f64 / total_i as f64) * 100.0).round() as i32);
        UsageWindow {
            label: "Credits".to_string(),
            used,
            total: Some(total_i),
            percent,
            reset_at: usage_reset_at,
            breakdown: Vec::new(),
        }
    });

    let mut monthly = monthly;
    if let (Some(ref mut m), Some(bonus_total)) = (monthly.as_mut(), bonus_total)
        && let Some(bonus_used) = bonus_used
    {
        let total_i = bonus_total.round() as i64;
        let used = bonus_used.round() as i64;
        if total_i > 0 {
            let percent = ((used as f64 / total_i as f64) * 100.0).round() as i32;
            m.breakdown.push(UsageWindow {
                label: "赠送额度".to_string(),
                used,
                total: Some(total_i),
                percent: Some(percent),
                reset_at: None,
                breakdown: Vec::new(),
            });
        }
    }

    SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name,
        hourly: None,
        weekly: None,
        monthly,
        balance: None,
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
    }
}

/// `(plan_name, reset_at, used, total, premium_used, premium_total, count_used, count_total)`
/// extracted from a Kiro usage payload.
type KiroUsageFields = (
    Option<String>,
    Option<String>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<i64>,
    Option<i64>,
);

fn extract_usage_fields(usage: Option<&Value>) -> KiroUsageFields {
    let usage = resolve_usage_root(usage);
    let plan_name = pick_string(
        usage,
        &[
            &["planName"],
            &["currentPlanName"],
            &["subscriptionInfo", "subscriptionTitle"],
            &["subscriptionInfo", "subscriptionName"],
        ],
    );
    let plan_tier = pick_string(
        usage,
        &[&["planTier"], &["tier"], &["subscriptionInfo", "type"]],
    );

    let mut credits_total = pick_number(
        usage,
        &[
            &["estimatedUsage", "total"],
            &["estimatedUsage", "creditsTotal"],
            &["credits", "total"],
            &["totalCredits"],
        ],
    );
    let mut credits_used = pick_number(
        usage,
        &[
            &["estimatedUsage", "used"],
            &["estimatedUsage", "creditsUsed"],
            &["credits", "used"],
            &["usedCredits"],
        ],
    );
    let mut bonus_total = pick_number(usage, &[&["bonusCredits", "total"], &["bonus", "total"]]);
    let mut bonus_used = pick_number(usage, &[&["bonusCredits", "used"], &["bonus", "used"]]);

    let breakdown = pick_usage_breakdown(usage);
    let free_trial = breakdown.and_then(|v| {
        get_path_value(v, &["freeTrialUsage"]).or_else(|| get_path_value(v, &["freeTrialInfo"]))
    });

    if credits_total.is_none() {
        credits_total = pick_number(
            breakdown,
            &[
                &["usageLimitWithPrecision"],
                &["usageLimit"],
                &["totalCredits"],
            ],
        );
    }
    if credits_used.is_none() {
        credits_used = pick_number(
            breakdown,
            &[
                &["currentUsageWithPrecision"],
                &["currentUsage"],
                &["usedCredits"],
            ],
        );
    }
    if bonus_total.is_none() {
        bonus_total = pick_number(free_trial, &[&["usageLimitWithPrecision"], &["usageLimit"]]);
    }
    if bonus_used.is_none() {
        bonus_used = pick_number(
            free_trial,
            &[&["currentUsageWithPrecision"], &["currentUsage"]],
        );
    }

    let usage_reset_at = parse_timestamp(
        usage
            .and_then(|v| get_path_value(v, &["resetAt"]))
            .or_else(|| usage.and_then(|v| get_path_value(v, &["resetTime"])))
            .or_else(|| breakdown.and_then(|v| get_path_value(v, &["resetDate"]))),
    );

    let bonus_expire_days =
        pick_number(free_trial, &[&["daysRemaining"], &["expiryDays"]]).map(|v| v.round() as i64);

    (
        plan_name,
        plan_tier,
        credits_total,
        credits_used,
        bonus_total,
        bonus_used,
        usage_reset_at,
        bonus_expire_days,
    )
}

// ── JSON path helpers ───────────────────────────────────────────────

fn get_path_value<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = root;
    for key in path {
        current = if let Ok(idx) = key.parse::<usize>() {
            current.get(idx)?
        } else {
            current.get(*key)?
        };
    }
    Some(current)
}

fn pick_string(root: Option<&Value>, paths: &[&[&str]]) -> Option<String> {
    let root = root?;
    for path in paths {
        if let Some(value) = get_path_value(root, path) {
            if let Some(text) = value.as_str() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            if let Some(num) = value.as_i64() {
                return Some(num.to_string());
            }
        }
    }
    None
}

fn pick_number(root: Option<&Value>, paths: &[&[&str]]) -> Option<f64> {
    let root = root?;
    for path in paths {
        if let Some(value) = get_path_value(root, path) {
            if let Some(num) = value.as_f64()
                && num.is_finite()
            {
                return Some(num);
            }
            if let Some(text) = value.as_str()
                && let Ok(num) = text.trim().parse::<f64>()
                && num.is_finite()
            {
                return Some(num);
            }
        }
    }
    None
}

fn resolve_usage_root(usage: Option<&Value>) -> Option<&Value> {
    let usage = usage?;
    usage
        .get("kiro.resourceNotifications.usageState")
        .or_else(|| get_path_value(usage, &["kiro", "resourceNotifications", "usageState"]))
        .or_else(|| get_path_value(usage, &["usageState"]))
        .or(Some(usage))
}

fn pick_usage_breakdown(usage: Option<&Value>) -> Option<&Value> {
    let usage = usage?;
    let list = get_path_value(usage, &["usageBreakdownList"])
        .and_then(|v| v.as_array())
        .or_else(|| get_path_value(usage, &["usageBreakdowns"]).and_then(|v| v.as_array()))?;
    list.iter()
        .find(|item| {
            item.get("type")
                .and_then(|v| v.as_str())
                .map(|t| t.eq_ignore_ascii_case("credit"))
                .unwrap_or(false)
        })
        .or_else(|| list.first())
}

fn extract_profile_arn(auth_token: Option<&Value>, profile: Option<&Value>) -> Option<String> {
    pick_string(profile, &[&["arn"], &["profileArn"], &["profile", "arn"]])
        .or_else(|| pick_string(auth_token, &[&["profileArn"], &["profile_arn"], &["arn"]]))
}

fn parse_profile_arn_region(profile_arn: &str) -> Option<String> {
    let mut segments = profile_arn.split(':');
    if !segments.next()?.eq_ignore_ascii_case("arn") {
        return None;
    }
    let _ = segments.next()?;
    let _ = segments.next()?;
    let region = segments.next()?.trim();
    if region.is_empty() {
        None
    } else {
        Some(region.to_string())
    }
}

fn runtime_endpoint_for_region(region: Option<&str>) -> String {
    match region.unwrap_or("us-east-1").to_ascii_lowercase().as_str() {
        "us-east-1" => "https://q.us-east-1.amazonaws.com".to_string(),
        "eu-central-1" => "https://q.eu-central-1.amazonaws.com".to_string(),
        _ => KIRO_RUNTIME_DEFAULT_ENDPOINT.to_string(),
    }
}

fn parse_timestamp(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(seconds) = value.as_i64() {
        return normalize_timestamp(seconds);
    }
    if let Some(seconds) = value.as_u64() {
        return normalize_timestamp(seconds as i64);
    }
    if let Some(text) = value.as_str() {
        if let Ok(num) = text.trim().parse::<i64>() {
            return normalize_timestamp(num);
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(text.trim()) {
            return Some(dt.timestamp());
        }
    }
    None
}

fn normalize_timestamp(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }
    if raw > 10_000_000_000 {
        Some(raw / 1000)
    } else {
        Some(raw)
    }
}

fn ensure_expires_at_from_expires_in(token: &mut Value) {
    let Some(obj) = token.as_object_mut() else {
        return;
    };
    if obj.contains_key("expiresAt") || obj.contains_key("expires_at") {
        return;
    }
    let expires_in = obj
        .get("expiresIn")
        .or_else(|| obj.get("expires_in"))
        .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|n| n as i64)))
        .unwrap_or(0);
    if expires_in <= 0 {
        return;
    }
    let expires_at = Utc::now() + chrono::Duration::seconds(expires_in);
    obj.insert(
        "expiresAt".to_string(),
        Value::String(expires_at.to_rfc3339()),
    );
}

fn parse_runtime_error_reason(body: &str) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(body).ok()?;
    pick_string(
        Some(&parsed),
        &[
            &["reason"],
            &["message"],
            &["errorMessage"],
            &["error", "message"],
        ],
    )
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.trim();
            if key.is_empty() {
                return None;
            }
            let raw = parts.next().unwrap_or("");
            Some((key.to_string(), percent_decode(raw)))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
                    && let Ok(val) = u8::from_str_radix(hex, 16)
                {
                    out.push(val);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn build_token_exchange_redirect_uri(base: &str, callback: &OAuthCallbackData) -> String {
    let path = if callback.path.starts_with('/') {
        callback.path.clone()
    } else {
        format!("/{}", callback.path)
    };
    format!(
        "{}{}?login_option={}",
        base.trim_end_matches('/'),
        path,
        urlencoding(&callback.login_option)
    )
}

fn find_callback_port() -> UsageResult<u16> {
    for port in CALLBACK_PORTS {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err(UsageError::Other(
        "本地回调端口均被占用，请关闭占用进程后重试".into(),
    ))
}

fn random_token() -> String {
    let bytes: Vec<u8> = (0..24).map(|_| rand::random::<u8>()).collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn normalize_email(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.contains('@') {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

fn http_client() -> UsageResult<reqwest::Client> {
    crate::http_client::usage_reqwest_with_active_fingerprint()
}

fn decrypt_required(cipher: &Option<String>) -> UsageResult<String> {
    let cipher = cipher
        .as_deref()
        .ok_or_else(|| UsageError::Other("缺少 access_token".into()))?;
    let pt = crypto::decrypt(cipher);
    if pt.is_empty() {
        return Err(UsageError::AuthRequired);
    }
    Ok(pt)
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

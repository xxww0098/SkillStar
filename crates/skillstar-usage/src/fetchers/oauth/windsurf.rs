//! Windsurf OAuth fetcher (implicit grant local callback + Codeium seat APIs).

use chrono::Utc;
use serde_json::{Value, json};
use std::time::Duration;
use tiny_http::{Header, Request, Response, Server};

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::pending_state;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const AUTH_BASE: &str = "https://www.windsurf.com";
const REGISTER_BASE: &str = "https://register.windsurf.com";
const DEFAULT_API_SERVER: &str = "https://server.codeium.com";
const CLIENT_ID: &str = "3GUryQ7ldAeKEuD2obYnppsnmj58eP5u";
const USER_AGENT: &str = "SkillStar";
const CALLBACK_PATH: &str = "/windsurf-auth-callback";
const OAUTH_TIMEOUT_SECS: i64 = 600;

pub async fn start_login(_region: Option<&str>) -> UsageResult<super::OAuthStartInfo> {
    let port = find_port()?;
    let state = crate::oauth::pkce::random_state();
    let redirect = format!("http://127.0.0.1:{}{}", port, CALLBACK_PATH);
    let auth_url = format!(
        "{}/windsurf/signin?response_type=token&client_id={}&redirect_uri={}&state={}&prompt=login&redirect_parameters_type=query&workflow=onboarding",
        AUTH_BASE,
        CLIENT_ID,
        urlencoding(&redirect),
        state
    );

    let pending_id = pending_state::register("windsurf", None, auth_url.clone());
    let pid = pending_id.clone();
    tokio::spawn(async move {
        let result = async {
            let firebase_token =
                tokio::task::spawn_blocking(move || wait_firebase_token(port, state))
                    .await
                    .map_err(|e| UsageError::Other(format!("Windsurf 回调：{}", e)))??;
            finalize_from_firebase(&firebase_token).await
        }
        .await;
        if let Some(tx) = pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::OAuthStartInfo::browser(auth_url, pending_id))
}

fn wait_firebase_token(port: u16, expected_state: String) -> UsageResult<String> {
    let server = Server::http(format!("127.0.0.1:{}", port))
        .map_err(|e| UsageError::Other(format!("Windsurf 回调服务：{}", e)))?;
    let deadline = Utc::now().timestamp() + OAUTH_TIMEOUT_SECS;
    loop {
        if Utc::now().timestamp() > deadline {
            return Err(UsageError::Other("Windsurf OAuth 超时".into()));
        }
        for request in server.incoming_requests() {
            if let Ok(token) = parse_callback(request, &expected_state) {
                return Ok(token);
            }
        }
        std::thread::sleep(Duration::from_millis(120));
    }
}

fn parse_callback(request: Request, expected_state: &str) -> UsageResult<String> {
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("");
    if !path.ends_with(CALLBACK_PATH) {
        let _ = request.respond(Response::from_string("not found").with_status_code(404));
        return Err(UsageError::Other("invalid path".into()));
    }
    let query = url.split('?').nth(1).unwrap_or("");
    let mut params = std::collections::HashMap::new();
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
            params.insert(k.to_string(), v.to_string());
        }
    }
    if params.get("state").map(|s| s.as_str()) != Some(expected_state) {
        let _ = request.respond(Response::from_string("state mismatch").with_status_code(400));
        return Err(UsageError::Other("Windsurf state 校验失败".into()));
    }
    if let Some(err) = params.get("error") {
        let _ = request.respond(Response::from_string(err).with_status_code(400));
        return Err(UsageError::Other(format!("Windsurf 授权失败：{}", err)));
    }
    let token = params
        .get("access_token")
        .cloned()
        .ok_or_else(|| UsageError::Other("缺少 access_token".into()))?;
    let _ = request.respond(
        Response::from_string("OK — return to SkillStar")
            .with_status_code(200)
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..])
                    .unwrap(),
            ),
    );
    Ok(token)
}

async fn finalize_from_firebase(firebase_token: &str) -> UsageResult<Subscription> {
    let registered = register_user(firebase_token).await?;
    let auth_token = get_one_time_auth_token(&registered.api_server_url, firebase_token).await?;
    let plan_status = get_plan_status(&registered.api_server_url, &auth_token)
        .await
        .ok();
    let display = registered.name.unwrap_or_else(|| "Windsurf".to_string());

    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: "windsurf".to_string(),
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
        access_token_encrypted: Some(crypto::encrypt(&registered.api_key)),
        refresh_token_encrypted: Some(crypto::encrypt(firebase_token)),
        access_token_expires_at: None,
        oauth_account_id: Some(registered.api_server_url),
        oauth_region: None,
        requires_reauth: false,
        fingerprint_id: None,
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: Some(crypto::encrypt(&auth_token)),
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };

    if let Some(status) = plan_status.as_ref()
        && let Ok(usage) = usage_from_plan(&sub.id, status)
    {
        storage::save_usage_snapshot(usage).ok();
    }

    storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Windsurf 订阅保存失败：{}", e)))
}

struct RegisterResult {
    api_key: String,
    api_server_url: String,
    name: Option<String>,
}

async fn register_user(firebase_id_token: &str) -> UsageResult<RegisterResult> {
    let value = post_json(
        REGISTER_BASE,
        "RegisterUser",
        json!({ "firebase_id_token": firebase_id_token }),
    )
    .await?;
    let api_key = pick_str(&value, &["apiKey", "api_key"])
        .ok_or_else(|| UsageError::Fetcher("RegisterUser 缺少 apiKey".into()))?;
    let api_server_url = pick_str(&value, &["apiServerUrl", "api_server_url"])
        .unwrap_or_else(|| DEFAULT_API_SERVER.to_string());
    let name = pick_str(&value, &["name"]);
    Ok(RegisterResult {
        api_key,
        api_server_url,
        name,
    })
}

async fn get_one_time_auth_token(
    api_server_url: &str,
    firebase_id_token: &str,
) -> UsageResult<String> {
    let value = post_json(
        api_server_url,
        "GetOneTimeAuthToken",
        json!({ "firebaseIdToken": firebase_id_token }),
    )
    .await?;
    pick_str(&value, &["authToken", "auth_token"])
        .ok_or_else(|| UsageError::Fetcher("缺少 authToken".into()))
}

async fn get_plan_status(api_server_url: &str, auth_token: &str) -> UsageResult<Value> {
    post_json(
        api_server_url,
        "GetPlanStatus",
        json!({ "authToken": auth_token, "includeTopUpStatus": true }),
    )
    .await
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let fp_id = subscription.fingerprint_id.clone();
    crate::http_client::with_fingerprint(fp_id, fetch_inner(subscription)).await
}

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let _api_key = decrypt_required(&subscription.access_token_encrypted)?;
    let firebase = subscription
        .refresh_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .unwrap_or_default();
    let api_server = subscription
        .oauth_account_id
        .clone()
        .unwrap_or_else(|| DEFAULT_API_SERVER.to_string());
    let auth_token = subscription
        .note
        .as_deref()
        .map(crypto::decrypt)
        .filter(|s| !s.is_empty());

    let auth_token = match auth_token {
        Some(t) if !firebase.is_empty() => {
            if get_plan_status(&api_server, &t).await.is_err() {
                get_one_time_auth_token(&api_server, &firebase).await?
            } else {
                t
            }
        }
        _ if !firebase.is_empty() => get_one_time_auth_token(&api_server, &firebase).await?,
        _ => return Err(UsageError::AuthRequired),
    };

    subscription.note = Some(crypto::encrypt(&auth_token));
    let status = get_plan_status(&api_server, &auth_token).await?;
    usage_from_plan(&subscription.id, &status)
}

fn usage_from_plan(subscription_id: &str, plan_status: &Value) -> UsageResult<SubscriptionUsage> {
    let plan_info = plan_status
        .get("planInfo")
        .cloned()
        .or_else(|| plan_status.get("plan_info").cloned());
    let plan_name = plan_info
        .as_ref()
        .and_then(|p| pick_str(p, &["planName", "plan_name", "teamsTier", "teams_tier"]))
        .unwrap_or_else(|| "Windsurf".to_string());

    let available_prompt = pick_i64(
        plan_status,
        &["availablePromptCredits", "available_prompt_credits"],
    );
    let used_prompt = pick_i64(plan_status, &["usedPromptCredits", "used_prompt_credits"]);
    let available_flow = pick_i64(
        plan_status,
        &["availableFlowCredits", "available_flow_credits"],
    );
    let used_flow = pick_i64(plan_status, &["usedFlowCredits", "used_flow_credits"]);

    let mut breakdown = Vec::new();
    if let (Some(av), Some(used)) = (available_prompt, used_prompt) {
        let total = av + used;
        if total > 0 {
            let pct = ((av as f64 / total as f64) * 100.0).round() as i32;
            breakdown.push(UsageWindow {
                label: "Prompt Credits".to_string(),
                used,
                total: Some(total),
                percent: Some(pct),
                reset_at: None,
                breakdown: Vec::new(),
            });
        }
    }
    if let (Some(av), Some(used)) = (available_flow, used_flow) {
        let total = av + used;
        if total > 0 {
            let pct = ((av as f64 / total as f64) * 100.0).round() as i32;
            breakdown.push(UsageWindow {
                label: "Flow Credits".to_string(),
                used,
                total: Some(total),
                percent: Some(pct),
                reset_at: None,
                breakdown: Vec::new(),
            });
        }
    }

    let monthly = if breakdown.is_empty() {
        None
    } else {
        let avg =
            breakdown.iter().filter_map(|w| w.percent).sum::<i32>() / breakdown.len().max(1) as i32;
        Some(UsageWindow {
            label: "Credits".to_string(),
            used: (100 - avg).max(0) as i64,
            total: Some(100),
            percent: Some(avg),
            reset_at: None,
            breakdown,
        })
    };

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some(plan_name),
        hourly: None,
        weekly: None,
        monthly,
        balance: None,
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
    })
}

async fn post_json(base: &str, method: &str, body: Value) -> UsageResult<Value> {
    let client = http_client()?;
    let url = format!(
        "{}/exa.seat_management_pb.SeatManagementService/{}",
        base.trim_end_matches('/'),
        method
    );
    let resp = client
        .post(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Windsurf {}：{}", method, e)))?;
    if !resp.status().is_success() {
        return Err(UsageError::Fetcher(format!(
            "Windsurf {} 状态 {}",
            method,
            resp.status()
        )));
    }
    resp.json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Windsurf {} 解析：{}", method, e)))
}

fn pick_str(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = value.get(*key).and_then(|v| v.as_str()).map(str::trim)
            && !s.is_empty()
        {
            return Some(s.to_string());
        }
    }
    None
}

fn pick_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(n) = value.get(*key).and_then(|v| v.as_i64()) {
            return Some(n);
        }
    }
    None
}

fn find_port() -> UsageResult<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| UsageError::Other(format!("绑定端口：{}", e)))?;
    Ok(listener
        .local_addr()
        .map_err(|e| UsageError::Other(e.to_string()))?
        .port())
}

fn http_client() -> UsageResult<reqwest::Client> {
    crate::http_client::usage_reqwest_with_active_fingerprint()
}

fn decrypt_required(cipher: &Option<String>) -> UsageResult<String> {
    let cipher = cipher
        .as_deref()
        .ok_or_else(|| UsageError::Other("缺少 api_key".into()))?;
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
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

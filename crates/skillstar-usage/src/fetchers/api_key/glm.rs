//! 智谱 GLM Coding Plan fetcher.
//!
//! `GET https://open.bigmodel.cn/api/monitor/usage/quota/limit`
//! NOTE: `Authorization: <token>` — no `Bearer ` prefix.
//!
//! Response shape (CN domestic; intl is `api.z.ai/...`):
//! ```json
//! { "success": true, "code": 200, "data": {
//!     "TIME_LIMIT":   { "used": ..., "total": ..., "resetTime": ... },
//!     "TOKENS_LIMIT": { "used": ..., "total": ..., "resetTime": ... },
//!     "level": "pro"
//! }}
//! ```

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;

use crate::subscription::{SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const ENDPOINT_CN: &str = "https://open.bigmodel.cn/api/monitor/usage/quota/limit";

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    code: i32,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    data: Value,
}

pub async fn fetch(subscription_id: &str, api_key: &str) -> UsageResult<SubscriptionUsage> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| UsageError::Fetcher(e.to_string()))?;

    let resp = client
        .get(ENDPOINT_CN)
        // GLM uses raw token, NOT Bearer.
        .header(reqwest::header::AUTHORIZATION, api_key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("GLM 请求失败：{}", e)))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UsageError::Fetcher(format!(
            "GLM 返回 {}: {}",
            status,
            body.chars().take(200).collect::<String>()
        )));
    }

    let env: Envelope = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("GLM 响应解析失败：{}", e)))?;
    if !env.success && env.code != 200 {
        return Err(UsageError::Fetcher(format!(
            "GLM 业务错误：{}",
            env.msg.unwrap_or_else(|| "未知".into())
        )));
    }

    let hourly = parse_window(&env.data, "TIME_LIMIT", "5h");
    let weekly = parse_window(&env.data, "TOKENS_LIMIT", "7d");
    let plan_name = env
        .data
        .get("level")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .or_else(|| Some("FREE".to_string()));

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name,
        hourly,
        weekly,
        monthly: None,
        balance: None,
        error: None,
    })
}

fn parse_window(data: &Value, key: &str, label: &str) -> Option<UsageWindow> {
    let node = data.get(key)?;
    let used = node.get("used").and_then(Value::as_i64).unwrap_or(0);
    let total = node.get("total").and_then(Value::as_i64);
    let reset_at = node
        .get("resetTime")
        .or_else(|| node.get("resetAt"))
        .and_then(Value::as_i64);
    let percent = match total {
        Some(t) if t > 0 => Some(((used as f64 / t as f64) * 100.0).round() as i32),
        _ => None,
    };
    Some(UsageWindow {
        label: label.to_string(),
        used,
        total,
        percent,
        reset_at,
    })
}

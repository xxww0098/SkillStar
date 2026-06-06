//! 智谱 GLM Coding Plan fetcher.
//!
//! `GET https://open.bigmodel.cn/api/monitor/usage/quota/limit`
//! NOTE: `Authorization: <token>` — no `Bearer ` prefix (see the spec's
//! [`AuthScheme::RawHeader`] in `skillstar-providers`).
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
use skillstar_fingerprint::DeviceFingerprint;
use skillstar_providers::balance;

use crate::subscription::{SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

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

pub async fn fetch(
    subscription_id: &str,
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<SubscriptionUsage> {
    let env: Envelope = super::fetch_spec(&balance::GLM, api_key, fingerprint).await?;

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
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
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
        breakdown: Vec::new(),
    })
}

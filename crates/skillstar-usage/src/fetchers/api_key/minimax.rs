//! MiniMax Token Plan fetcher.
//!
//! `GET https://www.minimax.io/v1/token_plan/remains` with the **Token Plan
//! Key** (different from a normal pay-as-you-go API key). The 401 hint lives on
//! the [`BalanceSpec`] in `skillstar-providers` and is applied by the shared
//! error mapper.

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
    code: i32,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Value,
}

pub async fn fetch(
    subscription_id: &str,
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<SubscriptionUsage> {
    let env: Envelope = super::fetch_spec(&balance::MINIMAX, api_key, fingerprint).await?;

    if env.code != 0 && env.code != 200 {
        return Err(UsageError::Fetcher(format!(
            "MiniMax 业务错误：{}",
            env.message.unwrap_or_else(|| format!("code={}", env.code))
        )));
    }

    // Field names from public docs (issue #88 referenced).
    let hourly = window_from(&env.data, &["five_hour_remains", "fiveHourRemains"], "5h");
    let daily = window_from(&env.data, &["day_remains", "dayRemains"], "1d");
    let plan_name = pick_string(
        &env.data,
        &[
            &["plan_name"],
            &["planName"],
            &["plan", "name"],
            &["package", "name"],
        ],
    )
    .or_else(|| Some("PRO".to_string()));

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name,
        hourly,
        weekly: None,
        monthly: daily,
        balance: None,
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
    })
}

fn window_from(data: &Value, keys: &[&str], label: &str) -> Option<UsageWindow> {
    let node = keys.iter().find_map(|k| data.get(*k))?;
    let remains = node
        .get("remains")
        .or_else(|| node.get("remaining"))
        .and_then(Value::as_i64);
    let total = node
        .get("total")
        .or_else(|| node.get("limit"))
        .and_then(Value::as_i64);
    let reset_at = node
        .get("reset_at")
        .or_else(|| node.get("resetAt"))
        .and_then(Value::as_i64);

    let (used, total_out) = match (remains, total) {
        (Some(r), Some(t)) => (t - r, Some(t)),
        (Some(r), None) => (0, Some(r)),
        _ => (0, None),
    };
    let percent = match total_out {
        Some(t) if t > 0 => Some(((used as f64 / t as f64) * 100.0).round() as i32),
        _ => None,
    };
    Some(UsageWindow {
        label: label.to_string(),
        used,
        total: total_out,
        percent,
        reset_at,
        breakdown: Vec::new(),
    })
}

fn pick_string(data: &Value, paths: &[&[&str]]) -> Option<String> {
    for path in paths {
        let mut cur = data;
        let mut ok = true;
        for key in *path {
            match cur.get(*key) {
                Some(v) => cur = v,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok
            && let Some(s) = cur.as_str()
            && !s.is_empty()
        {
            return Some(s.to_string());
        }
    }
    None
}

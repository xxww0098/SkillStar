//! 智谱 GLM Coding Plan fetcher.
//!
//! Endpoints (CN `open.bigmodel.cn`; intl uses `api.z.ai` with the same paths):
//! - `GET /api/monitor/usage/quota/limit` — 5h / 7d token windows + MCP monthly quota
//! - `GET /api/monitor/usage/model-usage?startTime=&endTime=` — rolling 24h model stats
//! - `GET /api/monitor/usage/tool-usage?startTime=&endTime=` — rolling 24h MCP tool stats
//!
//! Auth: raw token in `Authorization` (no `Bearer ` prefix).

use chrono::{Duration, Timelike, Utc};
use serde::Deserialize;
use serde_json::Value;
use skillstar_fingerprint::{DeviceFingerprint, Req, RequestError};
use skillstar_providers::balance;

use crate::http_client::usage_client_with_fingerprint;
use crate::subscription::{CreditInfo, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const GLM_MONITOR_BASE: &str = "https://open.bigmodel.cn/api/monitor/usage";

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
    let quota = fetch_quota(api_key, fingerprint).await?;
    let model_url = format!("{GLM_MONITOR_BASE}/model-usage?{}", usage_time_query());
    let tool_url = format!("{GLM_MONITOR_BASE}/tool-usage?{}", usage_time_query());

    let (model_usage, tool_usage) = tokio::join!(
        fetch_optional_json(api_key, fingerprint, &model_url),
        fetch_optional_json(api_key, fingerprint, &tool_url),
    );

    let plan_name = pick_plan_name(&quota.data);
    let (hourly, weekly, monthly) = parse_quota_windows(&quota.data);
    let mut credits = Vec::new();
    if let Some(model) = model_usage.as_ref() {
        credits.extend(parse_model_credits(&model.data));
    }
    if let Some(tool) = tool_usage.as_ref() {
        credits.extend(parse_tool_credits(&tool.data));
    }

    let has_windows = hourly.is_some() || weekly.is_some() || monthly.is_some();
    let error = if !has_windows && credits.is_empty() && plan_name.is_some() {
        Some("GLM 已返回套餐信息，但未解析到可展示的额度窗口。".to_string())
    } else {
        None
    };

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name,
        hourly,
        weekly,
        monthly,
        balance: None,
        credits,
        error,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    })
}

async fn fetch_quota(
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<Envelope> {
    super::fetch_spec(&balance::GLM, api_key, fingerprint).await
}

async fn fetch_optional_json(
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
    url: &str,
) -> Option<Envelope> {
    match fetch_monitor_json(api_key, fingerprint, url).await {
        Ok(env) if env.success || env.code == 200 => Some(env),
        _ => None,
    }
}

async fn fetch_monitor_json(
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
    url: &str,
) -> UsageResult<Envelope> {
    let client = usage_client_with_fingerprint(fingerprint)
        .map_err(|e| UsageError::Fetcher(format!("GLM client: {e}")))?;

    Req::get(&client, url)
        .header("Authorization", api_key)
        .header("Accept", "application/json")
        .send_json::<Envelope>()
        .await
        .map_err(map_monitor_err)
}

fn map_monitor_err(e: RequestError) -> UsageError {
    if e.is_auth_error() {
        return UsageError::AuthRequired;
    }
    match e {
        RequestError::HttpStatus { status, body } => UsageError::Fetcher(format!(
            "GLM 返回 {status}: {}",
            body.chars().take(200).collect::<String>()
        )),
        RequestError::JsonDecode { source, .. } => {
            UsageError::Fetcher(format!("GLM 响应解析失败：{source}"))
        }
        other => UsageError::Fetcher(format!("GLM 请求失败：{other}")),
    }
}

/// Rolling 24h window aligned with the official GLM monitor UI.
fn usage_time_query() -> String {
    let now = Utc::now();
    let start = now - Duration::hours(24);
    let start_floor = start
        .with_minute(0)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(start);
    let end = now
        .with_minute(59)
        .and_then(|t| t.with_second(59))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(now);

    format!(
        "startTime={}&endTime={}",
        urlencoding(&format_timestamp(start_floor)),
        urlencoding(&format_timestamp(end)),
    )
}

fn format_timestamp(dt: chrono::DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn urlencoding(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            ':' => "%3A".to_string(),
            other if other.is_ascii_alphanumeric() || other == '-' || other == '_' || other == '.' => {
                other.to_string()
            }
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}

fn parse_quota_windows(data: &Value) -> (Option<UsageWindow>, Option<UsageWindow>, Option<UsageWindow>) {
    if let Some(limits) = data.get("limits").and_then(Value::as_array)
        && !limits.is_empty()
    {
        return parse_limits_array(limits);
    }

    let hourly = parse_legacy_window(data, "TIME_LIMIT", "5h");
    let weekly = parse_legacy_window(data, "TOKENS_LIMIT", "7d");
    (hourly, weekly, None)
}

fn parse_limits_array(limits: &[Value]) -> (Option<UsageWindow>, Option<UsageWindow>, Option<UsageWindow>) {
    let mut hourly = None;
    let mut weekly = None;
    let mut monthly = None;

    for entry in limits {
        let Some(limit_type) = entry.get("type").and_then(Value::as_str) else {
            continue;
        };
        match limit_type {
            "TOKENS_LIMIT" => {
                let label = token_window_label(entry);
                let window = limit_entry_to_window(entry, label);
                match label {
                    "5h" => hourly = Some(window),
                    "7d" => weekly = Some(window),
                    _ => {}
                }
            }
            "TIME_LIMIT" => {
                let mut window = limit_entry_to_window(entry, "MCP");
                window.breakdown = parse_mcp_breakdown(entry);
                monthly = Some(window);
            }
            _ => {}
        }
    }

    (hourly, weekly, monthly)
}

fn parse_mcp_breakdown(entry: &Value) -> Vec<UsageWindow> {
    let Some(details) = entry.get("usageDetails").and_then(Value::as_array) else {
        return Vec::new();
    };
    details
        .iter()
        .filter_map(|detail| {
            let code = detail.get("modelCode").and_then(Value::as_str)?;
            let used = detail.get("usage").and_then(Value::as_i64).unwrap_or(0);
            Some(UsageWindow {
                label: mcp_tool_label(code),
                used,
                total: None,
                percent: None,
                reset_at: None,
                breakdown: Vec::new(),
            })
        })
        .collect()
}

fn mcp_tool_label(code: &str) -> String {
    match code {
        "search-prime" => "glm-mcp-search".to_string(),
        "web-reader" => "glm-mcp-web-read".to_string(),
        "zread" => "glm-mcp-zread".to_string(),
        other => format!("glm-mcp-{other}"),
    }
}

fn token_window_label(entry: &Value) -> &'static str {
    let unit = entry.get("unit").and_then(Value::as_i64).unwrap_or(0);
    let number = entry.get("number").and_then(Value::as_i64).unwrap_or(0);

    if unit == 3 && number == 5 {
        return "5h";
    }
    if unit == 6 && (number == 1 || number == 7) {
        return "7d";
    }
    if let Some(minutes) = window_minutes(entry) {
        if minutes <= 5 * 60 {
            return "5h";
        }
        return "7d";
    }
    "7d"
}

fn window_minutes(entry: &Value) -> Option<i64> {
    let unit = entry.get("unit").and_then(Value::as_i64)?;
    let number = entry.get("number").and_then(Value::as_i64)?;
    if number <= 0 {
        return None;
    }
    let minutes = match unit {
        5 => number,
        3 => number * 60,
        1 => number * 24 * 60,
        6 => number * 7 * 24 * 60,
        _ => return None,
    };
    Some(minutes)
}

fn limit_entry_to_window(entry: &Value, label: &str) -> UsageWindow {
    let total = entry.get("usage").and_then(Value::as_i64);
    let current = entry
        .get("currentValue")
        .or_else(|| entry.get("current_value"))
        .and_then(Value::as_i64);
    let remaining = entry.get("remaining").and_then(Value::as_i64);
    let used = compute_used(total, current, remaining);
    let percent = entry
        .get("percentage")
        .and_then(Value::as_i64)
        .map(|p| p.clamp(0, 100) as i32)
        .or_else(|| match total {
            Some(t) if t > 0 => Some(((used as f64 / t as f64) * 100.0).round() as i32),
            _ => None,
        });
    let reset_at = entry
        .get("nextResetTime")
        .or_else(|| entry.get("next_reset_time"))
        .or_else(|| entry.get("resetTime"))
        .or_else(|| entry.get("resetAt"))
        .and_then(Value::as_i64)
        .map(normalize_reset_epoch);

    UsageWindow {
        label: label.to_string(),
        used,
        total,
        percent,
        reset_at,
        breakdown: Vec::new(),
    }
}

fn parse_model_credits(data: &Value) -> Vec<CreditInfo> {
    let mut credits = Vec::new();
    let total = data.get("totalUsage").or_else(|| data.get("total_usage"));

    if let Some(tokens) = total
        .and_then(|v| v.get("totalTokensUsage").or_else(|| v.get("total_tokens_usage")))
        .and_then(Value::as_i64)
    {
        credits.push(credit_count("glm-24h-tokens", tokens));
    }
    if let Some(calls) = total
        .and_then(|v| v.get("totalModelCallCount").or_else(|| v.get("total_model_call_count")))
        .and_then(Value::as_i64)
    {
        credits.push(credit_count("glm-24h-calls", calls));
    }

    if let Some(models) = data
        .get("modelDataList")
        .or_else(|| data.get("model_data_list"))
        .and_then(Value::as_array)
    {
        for model in models {
            let Some(name) = model
                .get("modelName")
                .or_else(|| model.get("model_name"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let tokens: i64 = model
                .get("tokensUsage")
                .or_else(|| model.get("tokens_usage"))
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_i64).sum())
                .unwrap_or(0);
            if tokens > 0 {
                credits.push(credit_count(&format!("glm-model:{name}"), tokens));
            }
        }
    }

    credits
}

fn parse_tool_credits(data: &Value) -> Vec<CreditInfo> {
    let total = data.get("totalUsage").or_else(|| data.get("total_usage"));
    let Some(total) = total else {
        return Vec::new();
    };

    let pairs = [
        ("glm-24h-network-search", "totalNetworkSearchCount"),
        ("glm-24h-web-read", "totalWebReadMcpCount"),
        ("glm-24h-zread", "totalZreadMcpCount"),
    ];

    pairs
        .into_iter()
        .filter_map(|(kind, key)| {
            let count = total.get(key).and_then(Value::as_i64)?;
            (count > 0).then(|| credit_count(kind, count))
        })
        .collect()
}

fn credit_count(kind: &str, count: i64) -> CreditInfo {
    CreditInfo {
        credit_type: kind.to_string(),
        credit_amount: Some(format!("{count}")),
        minimum_credit_amount_for_usage: None,
    }
}

fn compute_used(total: Option<i64>, current: Option<i64>, remaining: Option<i64>) -> i64 {
    if let (Some(limit), Some(rem)) = (total, remaining) {
        let from_remaining = (limit - rem).max(0);
        return current.map(|c| from_remaining.max(c)).unwrap_or(from_remaining);
    }
    current.unwrap_or(0)
}

fn normalize_reset_epoch(raw: i64) -> i64 {
    if raw > 1_000_000_000_000 {
        raw / 1000
    } else {
        raw
    }
}

fn parse_legacy_window(data: &Value, key: &str, label: &str) -> Option<UsageWindow> {
    let node = data.get(key)?;
    let used = node
        .get("used")
        .or_else(|| node.get("currentValue"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let total = node
        .get("total")
        .or_else(|| node.get("usage"))
        .and_then(Value::as_i64);
    let reset_at = node
        .get("resetTime")
        .or_else(|| node.get("resetAt"))
        .or_else(|| node.get("nextResetTime"))
        .and_then(Value::as_i64)
        .map(normalize_reset_epoch);
    let percent = node
        .get("percentage")
        .and_then(Value::as_i64)
        .map(|p| p.clamp(0, 100) as i32)
        .or_else(|| match total {
            Some(t) if t > 0 => Some(((used as f64 / t as f64) * 100.0).round() as i32),
            _ => None,
        });
    Some(UsageWindow {
        label: label.to_string(),
        used,
        total,
        percent,
        reset_at,
        breakdown: Vec::new(),
    })
}

fn pick_plan_name(data: &Value) -> Option<String> {
    for key in ["level", "planName", "plan", "plan_type", "packageName"] {
        if let Some(name) = data.get(key).and_then(Value::as_str) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_limits_array_maps_token_windows_and_mcp_monthly() {
        let data = json!({
            "level": "lite",
            "limits": [
                {
                    "type": "TOKENS_LIMIT",
                    "unit": 3,
                    "number": 5,
                    "usage": 40_000_000,
                    "currentValue": 10_000_000,
                    "remaining": 30_000_000,
                    "percentage": 25,
                    "nextResetTime": 1_770_648_402_389_i64
                },
                {
                    "type": "TOKENS_LIMIT",
                    "unit": 6,
                    "number": 1,
                    "usage": 200_000_000,
                    "currentValue": 50_000_000,
                    "remaining": 150_000_000,
                    "percentage": 25,
                    "nextResetTime": 1_771_253_202_389_i64
                },
                {
                    "type": "TIME_LIMIT",
                    "unit": 5,
                    "number": 1,
                    "usage": 4000,
                    "currentValue": 100,
                    "remaining": 3900,
                    "percentage": 2,
                    "usageDetails": [
                        { "modelCode": "search-prime", "usage": 60 },
                        { "modelCode": "web-reader", "usage": 30 }
                    ]
                }
            ]
        });

        let (hourly, weekly, monthly) = parse_quota_windows(&data);
        let hourly = hourly.expect("5h");
        let weekly = weekly.expect("7d");
        let monthly = monthly.expect("mcp");

        assert_eq!(hourly.label, "5h");
        assert_eq!(hourly.used, 10_000_000);
        assert_eq!(weekly.label, "7d");
        assert_eq!(monthly.label, "MCP");
        assert_eq!(monthly.breakdown.len(), 2);
        assert_eq!(monthly.breakdown[0].label, "glm-mcp-search");
        assert_eq!(monthly.breakdown[0].used, 60);
    }

    #[test]
    fn parse_model_and_tool_credits() {
        let model = json!({
            "totalUsage": { "totalTokensUsage": 1_250_000, "totalModelCallCount": 42 },
            "modelDataList": [
                { "modelName": "glm-4.7", "tokensUsage": [100, 200, 300] }
            ]
        });
        let tool = json!({
            "totalUsage": { "totalNetworkSearchCount": 5, "totalWebReadMcpCount": 2 }
        });

        let model_credits = parse_model_credits(&model);
        assert!(model_credits.iter().any(|c| c.credit_type == "glm-24h-tokens"));
        assert!(model_credits.iter().any(|c| c.credit_type == "glm-model:glm-4.7"));

        let tool_credits = parse_tool_credits(&tool);
        assert!(tool_credits.iter().any(|c| c.credit_type == "glm-24h-network-search"));
    }

    #[test]
    fn parse_legacy_flat_shape_still_works() {
        let data = json!({
            "level": "pro",
            "TIME_LIMIT": { "used": 90, "total": 100, "resetTime": 1_700_000_000 },
            "TOKENS_LIMIT": { "used": 620, "total": 1000, "resetTime": 1_700_100_000 }
        });

        let (hourly, weekly, monthly) = parse_quota_windows(&data);
        assert!(monthly.is_none());
        assert_eq!(hourly.unwrap().label, "5h");
        assert_eq!(weekly.unwrap().used, 620);
    }
}
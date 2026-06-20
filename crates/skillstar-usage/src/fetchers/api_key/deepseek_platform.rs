//! DeepSeek platform usage analytics (non-official internal APIs).
//!
//! Requires a browser session token from `platform.deepseek.com` — distinct from
//! the API Key used for `/user/balance`.

use std::collections::HashMap;
use chrono::{Datelike, Utc};
use serde::Deserialize;
use skillstar_fingerprint::{DeviceFingerprint, Req};

use crate::http_client::usage_client_with_fingerprint;
use crate::subscription::{DeepSeekAnalytics, DeepSeekDailyUsage, DeepSeekModelUsage};
use crate::{UsageError, UsageResult};

const PLATFORM_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const AMOUNT_BASE: &str = "https://platform.deepseek.com/api/v0/usage/amount";
const COST_BASE: &str = "https://platform.deepseek.com/api/v0/usage/cost";

#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    amount: String,
}

#[derive(Debug, Deserialize)]
struct ModelUsage {
    model: String,
    usage: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct DayUsage {
    date: String,
    data: Vec<ModelUsage>,
}

#[derive(Debug, Deserialize)]
struct AmountBiz {
    total: Vec<ModelUsage>,
    days: Vec<DayUsage>,
}

#[derive(Debug, Deserialize)]
struct AmountData {
    biz_data: AmountBiz,
}

#[derive(Debug, Deserialize)]
struct AmountResp {
    data: AmountData,
}

#[derive(Debug, Deserialize)]
struct CostBiz {
    total: Vec<ModelUsage>,
    days: Vec<DayUsage>,
}

#[derive(Debug, Deserialize)]
struct CostData {
    biz_data: Vec<CostBiz>,
}

#[derive(Debug, Deserialize)]
struct CostResp {
    data: CostData,
}

pub async fn fetch_analytics(
    platform_token: &str,
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<DeepSeekAnalytics> {
    let now = Utc::now();
    let month = now.month();
    let year = now.year() as u32;

    let client = usage_client_with_fingerprint(fingerprint)
        .map_err(|e| UsageError::Fetcher(format!("DeepSeek platform client: {e}")))?;

    let amount_url = format!("{AMOUNT_BASE}?month={month}&year={year}");
    let cost_url = format!("{COST_BASE}?month={month}&year={year}");

    let (amount, cost) = tokio::join!(
        get_json::<AmountResp>(&client, &amount_url, platform_token),
        get_json::<CostResp>(&client, &cost_url, platform_token),
    );
    let amount = amount?;
    let cost = cost?;

    let mut daily = map_daily(&amount.data.biz_data.days, &cost);
    let month_cost = cost
        .data
        .biz_data
        .first()
        .map(|item| item.total.iter().map(|m| cost_sum(&m.usage)).sum::<f64>())
        .unwrap_or(0.0);

    if needs_previous_month_daily() {
        let prev_month = if month == 1 { 12 } else { month - 1 };
        let prev_year = if month == 1 { year - 1 } else { year };
        let prev_amount_url = format!("{AMOUNT_BASE}?month={prev_month}&year={prev_year}");
        let prev_cost_url = format!("{COST_BASE}?month={prev_month}&year={prev_year}");
        if let (Ok(prev_amount), Ok(prev_cost)) = tokio::join!(
            get_json::<AmountResp>(&client, &prev_amount_url, platform_token),
            get_json::<CostResp>(&client, &prev_cost_url, platform_token),
        ) {
            let mut merged = map_daily(&prev_amount.data.biz_data.days, &prev_cost);
            merged.extend(daily);
            daily = merged;
        }
    }

    let models = map_models(&amount, &cost);
    let today_cost = today_cost_from_daily(&daily);

    Ok(DeepSeekAnalytics {
        month_cost,
        today_cost,
        models,
        daily,
    })
}

async fn get_json<T: serde::de::DeserializeOwned>(
    client: &skillstar_fingerprint::FingerprintAwareClient,
    url: &str,
    token: &str,
) -> UsageResult<T> {
    let resp = Req::get(client, url)
        .header("Accept", "*/*")
        .header("x-app-version", "1.0.0")
        .header("User-Agent", PLATFORM_UA)
        .bearer(token)
        .send()
        .await
        .map_err(|e| map_platform_err(e))?;

    if resp.is_auth_error() {
        return Err(UsageError::Fetcher(
            "DeepSeek 平台用量 Token 无效或已过期，请在订阅设置中重新粘贴".into(),
        ));
    }
    if !resp.is_success() {
        return Err(UsageError::Fetcher(format!(
            "DeepSeek 平台用量接口 HTTP {}: {}",
            resp.status,
            resp.body.chars().take(200).collect::<String>()
        )));
    }
    serde_json::from_str(&resp.body)
        .map_err(|e| UsageError::Fetcher(format!("DeepSeek 平台用量解析失败: {e}")))
}

fn map_platform_err(e: skillstar_fingerprint::RequestError) -> UsageError {
    if e.is_auth_error() {
        return UsageError::Fetcher(
            "DeepSeek 平台用量 Token 无效或已过期，请在订阅设置中重新粘贴".into(),
        );
    }
    UsageError::Fetcher(format!("DeepSeek 平台用量请求失败: {e}"))
}

fn map_models(amount: &AmountResp, cost: &CostResp) -> Vec<DeepSeekModelUsage> {
    let cost_total = cost.data.biz_data.first();
    let cost_for_model = |model: &str| -> f64 {
        cost_total
            .and_then(|item| item.total.iter().find(|m| m.model == model))
            .map(|m| cost_sum(&m.usage))
            .unwrap_or(0.0)
    };

    let mut models = Vec::new();
    for model_usage in &amount.data.biz_data.total {
        let Some((key, name)) = model_label(&model_usage.model) else {
            continue;
        };
        let (total, request, hit, miss, response) = token_breakdown(&model_usage.usage);
        models.push(DeepSeekModelUsage {
            key: key.to_string(),
            name: name.to_string(),
            total_tokens: total,
            request_count: request,
            cache_hit_tokens: hit,
            cache_miss_tokens: miss,
            response_tokens: response,
            cost: cost_for_model(&model_usage.model),
        });
    }
    models
}

fn map_daily(days: &[DayUsage], cost: &CostResp) -> Vec<DeepSeekDailyUsage> {
    let mut cost_by_date: HashMap<String, f64> = HashMap::new();
    if let Some(item) = cost.data.biz_data.first() {
        for day in &item.days {
            let day_cost: f64 = day.data.iter().map(|m| cost_sum(&m.usage)).sum();
            cost_by_date.insert(day.date.clone(), day_cost);
        }
    }

    days.iter()
        .map(|day| {
            let mut flash = 0u64;
            let mut flash_hit = 0u64;
            let mut flash_miss = 0u64;
            let mut flash_resp = 0u64;
            let mut pro = 0u64;
            let mut pro_hit = 0u64;
            let mut pro_miss = 0u64;
            let mut pro_resp = 0u64;
            let mut total = 0u64;
            for model_usage in &day.data {
                let (tokens, _, hit, miss, response) = token_breakdown(&model_usage.usage);
                total += tokens;
                match model_usage.model.as_str() {
                    "deepseek-v4-flash" => {
                        flash += tokens;
                        flash_hit += hit;
                        flash_miss += miss;
                        flash_resp += response;
                    }
                    "deepseek-v4-pro" => {
                        pro += tokens;
                        pro_hit += hit;
                        pro_miss += miss;
                        pro_resp += response;
                    }
                    _ => {}
                }
            }
            DeepSeekDailyUsage {
                date: day.date.clone(),
                flash_tokens: flash,
                flash_cache_hit: flash_hit,
                flash_cache_miss: flash_miss,
                flash_response: flash_resp,
                pro_tokens: pro,
                pro_cache_hit: pro_hit,
                pro_cache_miss: pro_miss,
                pro_response: pro_resp,
                total_tokens: total,
                total_cost: cost_by_date.get(&day.date).copied().unwrap_or(0.0),
            }
        })
        .collect()
}

fn today_cost_from_daily(daily: &[DeepSeekDailyUsage]) -> f64 {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    daily
        .iter()
        .find(|day| day.date == today)
        .map(|day| day.total_cost)
        .unwrap_or(0.0)
}

fn needs_previous_month_daily() -> bool {
    let now = Utc::now();
    let week_ago = now - chrono::Duration::days(6);
    week_ago.month() != now.month() || week_ago.year() != now.year()
}

fn model_label(model: &str) -> Option<(&'static str, &'static str)> {
    match model {
        "deepseek-v4-flash" => Some(("flash", "V4 Flash")),
        "deepseek-v4-pro" => Some(("pro", "V4 Pro")),
        _ => None,
    }
}

fn token_breakdown(usage: &[Entry]) -> (u64, u64, u64, u64, u64) {
    let mut total = 0u64;
    let mut request = 0u64;
    let mut hit = 0u64;
    let mut miss = 0u64;
    let mut response = 0u64;
    for entry in usage {
        let Some(value) = parse_amount_u64(&entry.amount) else {
            // Skip malformed rows instead of silently counting them as 0,
            // which would skew the displayed totals. The warning leaves a
            // breadcrumb for when DeepSeek changes its response shape.
            tracing::warn!(
                "[deepseek-platform] skipping malformed usage row (kind={}, amount={:?})",
                entry.kind,
                entry.amount
            );
            continue;
        };
        match entry.kind.as_str() {
            "REQUEST" => request = value,
            "PROMPT_CACHE_HIT_TOKEN" => {
                hit = value;
                total = total.saturating_add(value);
            }
            "PROMPT_CACHE_MISS_TOKEN" => {
                miss = value;
                total = total.saturating_add(value);
            }
            "RESPONSE_TOKEN" => {
                response = value;
                total = total.saturating_add(value);
            }
            "PROMPT_TOKEN" => total = total.saturating_add(value),
            _ => {}
        }
    }
    (total, request, hit, miss, response)
}

fn cost_sum(usage: &[Entry]) -> f64 {
    usage
        .iter()
        .filter(|entry| entry.kind != "REQUEST")
        .map(|entry| match entry.amount.parse::<f64>() {
            Ok(v) if v.is_finite() && v >= 0.0 => v,
            Ok(_) => {
                tracing::warn!(
                    "[deepseek-platform] skipping non-positive cost row (kind={}, amount={:?})",
                    entry.kind,
                    entry.amount
                );
                0.0
            }
            Err(_) => {
                tracing::warn!(
                    "[deepseek-platform] skipping malformed cost row (kind={}, amount={:?})",
                    entry.kind,
                    entry.amount
                );
                0.0
            }
        })
        .sum()
}

/// Parse a DeepSeek `amount` string into a clamped `u64`. Returns `None` (and
/// leaves the caller to warn) when the value cannot be parsed or is negative,
/// so bad upstream rows never masquerade as a legitimate `0`.
fn parse_amount_u64(s: &str) -> Option<u64> {
    let v = s.trim().parse::<f64>().ok()?;
    if !v.is_finite() || v < 0.0 {
        return None;
    }
    // `f64 as u64` saturates at `u64::MAX`; clamp to a sane upper bound first
    // so one pathological row can't pin every subsequent `total +=` at MAX.
    Some(v.round().min(u64::MAX as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_breakdown_splits_usage_types() {
        let usage = vec![
            Entry {
                kind: "REQUEST".into(),
                amount: "12".into(),
            },
            Entry {
                kind: "PROMPT_CACHE_HIT_TOKEN".into(),
                amount: "100".into(),
            },
            Entry {
                kind: "PROMPT_CACHE_MISS_TOKEN".into(),
                amount: "50".into(),
            },
            Entry {
                kind: "RESPONSE_TOKEN".into(),
                amount: "25".into(),
            },
        ];
        assert_eq!(token_breakdown(&usage), (175, 12, 100, 50, 25));
    }

    #[test]
    fn cost_sum_ignores_request_entries() {
        let usage = vec![
            Entry {
                kind: "REQUEST".into(),
                amount: "99".into(),
            },
            Entry {
                kind: "PROMPT_CACHE_HIT_TOKEN".into(),
                amount: "1.25".into(),
            },
            Entry {
                kind: "RESPONSE_TOKEN".into(),
                amount: "0.75".into(),
            },
        ];
        assert!((cost_sum(&usage) - 2.0).abs() < f64::EPSILON);
    }
}
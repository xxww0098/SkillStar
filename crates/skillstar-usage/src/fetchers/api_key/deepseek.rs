//! DeepSeek balance fetcher.
//!
//! `GET https://api.deepseek.com/user/balance` with `Authorization: Bearer <key>`.
//! Returns `is_available` + `balance_infos[]` (per-currency totals).

use chrono::Utc;
use serde::Deserialize;

use crate::subscription::{MonetaryBalance, SubscriptionUsage};
use crate::{UsageError, UsageResult};

const ENDPOINT: &str = "https://api.deepseek.com/user/balance";

#[derive(Debug, Deserialize)]
struct BalanceResponse {
    #[serde(default)]
    is_available: bool,
    #[serde(default)]
    balance_infos: Vec<BalanceInfo>,
}

#[derive(Debug, Deserialize)]
struct BalanceInfo {
    #[serde(default)]
    currency: String,
    #[serde(default)]
    total_balance: String,
    #[serde(default)]
    granted_balance: String,
    #[serde(default)]
    topped_up_balance: String,
}

pub async fn fetch(subscription_id: &str, api_key: &str) -> UsageResult<SubscriptionUsage> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| UsageError::Fetcher(e.to_string()))?;

    let resp = client
        .get(ENDPOINT)
        .bearer_auth(api_key)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("DeepSeek 请求失败：{}", e)))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UsageError::Fetcher(format!(
            "DeepSeek 返回 {}: {}",
            status,
            body.chars().take(200).collect::<String>()
        )));
    }

    let body: BalanceResponse = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("DeepSeek 响应解析失败：{}", e)))?;

    // Pick the first balance entry; if user has both CNY and USD we surface CNY first.
    let primary = body
        .balance_infos
        .iter()
        .find(|b| b.currency.eq_ignore_ascii_case("CNY"))
        .or_else(|| body.balance_infos.first());

    let balance = primary.map(|b| MonetaryBalance {
        currency: if b.currency.is_empty() {
            "CNY".to_string()
        } else {
            b.currency.clone()
        },
        total: parse_decimal(&b.total_balance),
        granted: parse_decimal(&b.granted_balance),
        topped_up: parse_decimal(&b.topped_up_balance),
    });

    let mut error = None;
    if !body.is_available {
        error = Some("DeepSeek 显示账户不可用（余额耗尽？）".to_string());
    }

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some("PAYG".to_string()),
        balance,
        hourly: None,
        weekly: None,
        monthly: None,
        error,
    })
}

fn parse_decimal(s: &str) -> f64 {
    s.parse().unwrap_or(0.0)
}

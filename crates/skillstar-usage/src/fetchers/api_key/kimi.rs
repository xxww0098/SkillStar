//! Kimi (Moonshot) balance fetcher.
//!
//! `GET https://api.moonshot.cn/v1/users/me/balance` with `Bearer <key>`.
//! Returns CNY balance breakdown — no plan tier, no rate-limit windows.

use chrono::Utc;
use serde::Deserialize;

use crate::subscription::{MonetaryBalance, SubscriptionUsage};
use crate::{UsageError, UsageResult};

const ENDPOINT: &str = "https://api.moonshot.cn/v1/users/me/balance";

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    code: i32,
    #[serde(default)]
    status: bool,
    #[serde(default)]
    data: BalanceData,
}

#[derive(Debug, Default, Deserialize)]
struct BalanceData {
    #[serde(default)]
    available_balance: f64,
    #[serde(default)]
    voucher_balance: f64,
    #[serde(default)]
    cash_balance: f64,
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
        .map_err(|e| UsageError::Fetcher(format!("Kimi 请求失败：{}", e)))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UsageError::Fetcher(format!(
            "Kimi 返回 {}: {}",
            status,
            body.chars().take(200).collect::<String>()
        )));
    }

    let env: Envelope = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Kimi 响应解析失败：{}", e)))?;
    if !env.status && env.code != 0 {
        return Err(UsageError::Fetcher(format!("Kimi 业务错误 code={}", env.code)));
    }

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some("PAYG".to_string()),
        balance: Some(MonetaryBalance {
            currency: "CNY".to_string(),
            total: env.data.available_balance,
            granted: env.data.voucher_balance,
            topped_up: env.data.cash_balance,
        }),
        hourly: None,
        weekly: None,
        monthly: None,
        error: None,
    })
}

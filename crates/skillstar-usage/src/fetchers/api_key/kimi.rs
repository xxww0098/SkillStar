//! Kimi (Moonshot) balance fetcher.
//!
//! `GET https://api.moonshot.cn/v1/users/me/balance` with `Bearer <key>`.
//! Returns CNY balance breakdown — no plan tier, no rate-limit windows.
//!
//! The request path is shared (see [`super::fetch_spec`]); this module only
//! describes how to turn the Kimi response into a [`SubscriptionUsage`].

use chrono::Utc;
use serde::Deserialize;
use skillstar_fingerprint::DeviceFingerprint;
use skillstar_providers::balance;

use crate::subscription::{MonetaryBalance, SubscriptionUsage};
use crate::{UsageError, UsageResult};

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

pub async fn fetch(
    subscription_id: &str,
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<SubscriptionUsage> {
    let env: Envelope = super::fetch_spec(&balance::KIMI, api_key, fingerprint).await?;

    if !env.status && env.code != 0 {
        return Err(UsageError::Fetcher(format!(
            "Kimi 业务错误 code={}",
            env.code
        )));
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
            is_available: None,
        }),
        hourly: None,
        weekly: None,
        monthly: None,
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    })
}

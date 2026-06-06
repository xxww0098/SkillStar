//! DeepSeek balance fetcher.
//!
//! `GET https://api.deepseek.com/user/balance` with `Authorization: Bearer <key>`.
//! Returns `is_available` + `balance_infos[]` (per-currency totals).
//!
//! The request path is shared (see [`super::fetch_spec`]); this module only
//! describes how to turn the DeepSeek response into a [`SubscriptionUsage`].

use chrono::Utc;
use serde::Deserialize;
use skillstar_fingerprint::DeviceFingerprint;
use skillstar_providers::balance;

use crate::UsageResult;
use crate::subscription::{MonetaryBalance, SubscriptionUsage};

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

pub async fn fetch(
    subscription_id: &str,
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<SubscriptionUsage> {
    let body: BalanceResponse = super::fetch_spec(&balance::DEEPSEEK, api_key, fingerprint).await?;

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
        credits: Vec::new(),
        error,
        api_keys: Vec::new(),
    })
}

fn parse_decimal(s: &str) -> f64 {
    s.parse().unwrap_or(0.0)
}

//! DeepSeek balance fetcher.
//!
//! `GET https://api.deepseek.com/user/balance` with `Authorization: Bearer <key>`.
//! Returns `is_available` + `balance_infos[]` (per-currency totals).
//!
//! When a platform session token is configured, also fetches model usage analytics
//! from `platform.deepseek.com` internal APIs (see [`super::deepseek_platform`]).
//!
//! The request path is shared (see [`super::fetch_spec`]); this module only
//! describes how to turn the DeepSeek response into a [`SubscriptionUsage`].

use chrono::Utc;
use serde::Deserialize;
use skillstar_fingerprint::DeviceFingerprint;
use skillstar_providers::balance;

use crate::UsageResult;
use crate::subscription::{CreditInfo, MonetaryBalance, Subscription, SubscriptionUsage};

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
    subscription: &Subscription,
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<SubscriptionUsage> {
    let body: BalanceResponse = super::fetch_spec(&balance::DEEPSEEK, api_key, fingerprint).await?;
    let (balance, credits, error) = map_balance_response(&body);

    let mut usage = SubscriptionUsage {
        subscription_id: subscription.id.clone(),
        fetched_at: Utc::now().timestamp(),
        plan_name: None,
        balance,
        hourly: None,
        weekly: None,
        monthly: None,
        credits,
        error,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    };

    if let Some(cipher) = subscription.platform_token_encrypted.as_deref() {
        let token = crate::crypto::decrypt(cipher);
        if token.trim().is_empty() {
            append_platform_error(&mut usage, "平台用量 Token 解密失败");
        } else {
            match super::deepseek_platform::fetch_analytics(token.trim(), fingerprint).await {
                Ok(analytics) => usage.deepseek_analytics = Some(analytics),
                Err(err) => append_platform_error(&mut usage, &err.to_string()),
            }
        }
    }

    Ok(usage)
}

fn append_platform_error(usage: &mut SubscriptionUsage, message: &str) {
    let detail = format!("平台用量：{message}");
    usage.error = match usage.error.take() {
        Some(existing) => Some(format!("{existing}；{detail}")),
        None => Some(detail),
    };
}

fn map_balance_response(
    body: &BalanceResponse,
) -> (Option<MonetaryBalance>, Vec<CreditInfo>, Option<String>) {
    let primary_idx = pick_primary_index(&body.balance_infos);
    let primary = primary_idx.and_then(|idx| body.balance_infos.get(idx));

    let balance = primary.map(|b| MonetaryBalance {
        currency: normalize_currency(&b.currency),
        total: parse_decimal(&b.total_balance),
        granted: parse_decimal(&b.granted_balance),
        topped_up: parse_decimal(&b.topped_up_balance),
        is_available: Some(body.is_available),
    });

    let credits = body
        .balance_infos
        .iter()
        .enumerate()
        .filter_map(|(idx, info)| {
            if Some(idx) == primary_idx {
                return None;
            }
            let currency = normalize_currency(&info.currency);
            Some(CreditInfo {
                credit_type: format!("deepseek-balance:{currency}"),
                credit_amount: Some(format!("{:.2}", parse_decimal(&info.total_balance))),
                minimum_credit_amount_for_usage: None,
            })
        })
        .collect();

    let error = if !body.is_available {
        Some("DeepSeek 显示账户不可用（余额耗尽？）".to_string())
    } else {
        None
    };

    (balance, credits, error)
}

fn pick_primary_index(infos: &[BalanceInfo]) -> Option<usize> {
    if infos.is_empty() {
        return None;
    }
    infos
        .iter()
        .position(|b| b.currency.eq_ignore_ascii_case("CNY"))
        .or(Some(0))
}

fn normalize_currency(currency: &str) -> String {
    if currency.is_empty() {
        "CNY".to_string()
    } else {
        currency.to_uppercase()
    }
}

fn parse_decimal(s: &str) -> f64 {
    // DeepSeek's `/user/balance` returns numeric strings here. A parse failure
    // used to silently produce `0.0`, which would make the UI show an empty
    // balance and mislead users into thinking they had no credit. Keep the
    // `0.0` fallback (so one malformed field can't abort the whole balance
    // object) but surface a warning so upstream format changes are visible.
    match s.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => v,
        Ok(_) => {
            tracing::warn!("[deepseek] clamped non-finite balance field (raw={:?})", s);
            0.0
        }
        Err(_) => {
            tracing::warn!("[deepseek] failed to parse balance field as number (raw={:?}); falling back to 0", s);
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_body() -> BalanceResponse {
        BalanceResponse {
            is_available: true,
            balance_infos: vec![
                BalanceInfo {
                    currency: "CNY".to_string(),
                    total_balance: "110.00".to_string(),
                    granted_balance: "10.00".to_string(),
                    topped_up_balance: "100.00".to_string(),
                },
                BalanceInfo {
                    currency: "USD".to_string(),
                    total_balance: "5.50".to_string(),
                    granted_balance: "0.00".to_string(),
                    topped_up_balance: "5.50".to_string(),
                },
            ],
        }
    }

    #[test]
    fn maps_primary_cny_balance_and_secondary_currency_credit() {
        let (balance, credits, error) = map_balance_response(&sample_body());
        let balance = balance.expect("primary balance");
        assert_eq!(balance.currency, "CNY");
        assert!((balance.total - 110.0).abs() < f64::EPSILON);
        assert!((balance.granted - 10.0).abs() < f64::EPSILON);
        assert!((balance.topped_up - 100.0).abs() < f64::EPSILON);
        assert_eq!(balance.is_available, Some(true));
        assert_eq!(credits.len(), 1);
        assert_eq!(credits[0].credit_type, "deepseek-balance:USD");
        assert_eq!(credits[0].credit_amount.as_deref(), Some("5.50"));
        assert!(error.is_none());
    }

    #[test]
    fn unavailable_account_sets_error_and_flag() {
        let mut body = sample_body();
        body.is_available = false;
        let (balance, _, error) = map_balance_response(&body);
        assert_eq!(balance.and_then(|b| b.is_available), Some(false));
        assert!(error.is_some());
    }
}
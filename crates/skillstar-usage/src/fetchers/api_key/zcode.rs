//! ZCode (Z.ai / BigModel) Coding Plan usage fetcher.
//!
//! `GET https://zcode.z.ai/api/v1/zcode-plan/billing/balance?app_version=3.0.0`
//! with `Authorization: Bearer <access_token>` — the same token ZCode stores
//! after its `zcode://` deep-link OAuth (which SkillStar cannot host, so the
//! user pastes it as an API key here).
//!
//! The response is a **per-model token bucket** list (e.g. GLM-5.2 3M / day,
//! GLM-5-Turbo 2M / day), refreshed daily. We aggregate the buckets into a
//! single `UsageWindow` (with a per-model `breakdown`) plus per-model
//! `CreditInfo` entries, rather than a `MonetaryBalance`, since the units are
//! tokens, not currency.

use chrono::Utc;
use serde::Deserialize;
use skillstar_fingerprint::DeviceFingerprint;
use skillstar_providers::balance;

use crate::UsageResult;
use crate::subscription::{CreditInfo, SubscriptionUsage, UsageWindow};

/// Outer envelope shared by every ZCode plan API response.
#[derive(Debug, Deserialize)]
struct BalanceResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    data: BalanceData,
}

#[derive(Debug, Default, Deserialize)]
struct BalanceData {
    #[serde(default)]
    balances: Vec<ModelBucket>,
}

#[derive(Debug, Deserialize)]
struct ModelBucket {
    #[serde(default)]
    show_name: String,
    #[serde(default)]
    total_units: i64,
    #[serde(default)]
    used_units: i64,
    #[serde(default)]
    remaining_units: i64,
    /// Epoch seconds — the daily window this bucket resets at.
    #[serde(default)]
    period_end: i64,
}

pub async fn fetch(
    subscription_id: &str,
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<SubscriptionUsage> {
    let body: BalanceResponse = super::fetch_spec(&balance::ZCODE, api_key, fingerprint).await?;

    // ZCode signals non-zero `code` on failure (e.g. plan not entitled).
    if body.code != 0 {
        return Ok(SubscriptionUsage {
            subscription_id: subscription_id.to_string(),
            fetched_at: Utc::now().timestamp(),
            plan_name: Some("ZCode Coding Plan".to_string()),
            error: Some(format!("ZCode billing 返回错误码 {}（订阅未激活或额度未授予？）", body.code)),
            ..Default::default()
        });
    }

    let buckets = body.data.balances;
    if buckets.is_empty() {
        return Ok(SubscriptionUsage {
            subscription_id: subscription_id.to_string(),
            fetched_at: Utc::now().timestamp(),
            plan_name: Some("ZCode Coding Plan".to_string()),
            ..Default::default()
        });
    }

    // Aggregate totals across all model buckets, and keep a per-model breakdown
    // so the UI can render each model's slice. The window is daily, so label it
    // "今日" and surface the earliest period_end as the reset point.
    let mut total: i64 = 0;
    let mut used: i64 = 0;
    let mut reset_at: Option<i64> = None;
    let mut credits: Vec<CreditInfo> = Vec::with_capacity(buckets.len());
    let mut breakdown: Vec<UsageWindow> = Vec::with_capacity(buckets.len());

    for b in &buckets {
        total += b.total_units;
        used += b.used_units;
        if b.period_end > 0 {
            reset_at = Some(reset_at.map_or(b.period_end, |r| r.min(b.period_end)));
        }
        credits.push(CreditInfo {
            credit_type: if b.show_name.is_empty() {
                "ZCode".to_string()
            } else {
                b.show_name.clone()
            },
            credit_amount: Some(b.remaining_units.to_string()),
            minimum_credit_amount_for_usage: None,
        });
        breakdown.push(UsageWindow {
            label: if b.show_name.is_empty() {
                "模型".to_string()
            } else {
                b.show_name.clone()
            },
            used: b.used_units,
            total: if b.total_units > 0 { Some(b.total_units) } else { None },
            percent: pct(b.used_units, b.total_units),
            reset_at: if b.period_end > 0 { Some(b.period_end) } else { None },
            breakdown: Vec::new(),
        });
    }

    let window = UsageWindow {
        label: "今日".to_string(),
        used,
        total: if total > 0 { Some(total) } else { None },
        percent: pct(used, total),
        reset_at,
        breakdown,
    };

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some("ZCode Coding Plan".to_string()),
        monthly: Some(window),
        credits,
        ..Default::default()
    })
}

/// Integer percentage in 0..=100 when both `used` and `total` are positive.
fn pct(used: i64, total: i64) -> Option<i32> {
    if total <= 0 {
        return None;
    }
    let p = (used as f64 / total as f64 * 100.0).round() as i32;
    Some(p.clamp(0, 100))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real response captured from ~/.zcode/v2/logs (two model buckets, daily).
    const SAMPLE_BALANCE: &str = r#"{
        "code": 0,
        "msg": "",
        "data": {
            "server_time": 1781416086,
            "balances": [
                {
                    "bucket_id": "bucket_a",
                    "entitlement_id": "ent_zcode_v3_glm_52",
                    "show_name": "GLM-5.2",
                    "total_units": 3000000,
                    "used_units": 0,
                    "remaining_units": 3000000,
                    "available_units": 3000000,
                    "period_start": 1781366400,
                    "period_end": 1781452800
                },
                {
                    "bucket_id": "bucket_b",
                    "entitlement_id": "ent_zcode_v3_glm_5turbo",
                    "show_name": "GLM-5-Turbo",
                    "total_units": 2000000,
                    "used_units": 500000,
                    "remaining_units": 1500000,
                    "available_units": 1500000,
                    "period_start": 1781366400,
                    "period_end": 1781452800
                }
            ]
        }
    }"#;

    #[test]
    fn parses_real_balance_into_aggregated_window() {
        let resp: BalanceResponse = serde_json::from_str(SAMPLE_BALANCE).unwrap();
        assert_eq!(resp.code, 0);
        let buckets = &resp.data.balances;
        assert_eq!(buckets.len(), 2);

        // Mirror the aggregation the fetcher performs so we assert the math.
        let total: i64 = buckets.iter().map(|b| b.total_units).sum();
        let used: i64 = buckets.iter().map(|b| b.used_units).sum();
        assert_eq!(total, 5_000_000);
        assert_eq!(used, 500_000);
        assert_eq!(pct(used, total), Some(10));

        // Per-model percentages.
        assert_eq!(pct(buckets[0].used_units, buckets[0].total_units), Some(0));
        assert_eq!(pct(buckets[1].used_units, buckets[1].total_units), Some(25));
    }

    #[test]
    fn empty_balances_yield_no_window() {
        let resp: BalanceResponse =
            serde_json::from_str(r#"{"code":0,"data":{"balances":[]}}"#).unwrap();
        assert!(resp.data.balances.is_empty());
        // pct over an empty sum is None (total 0).
        assert_eq!(pct(0, 0), None);
    }

    #[test]
    fn nonzero_code_signals_error() {
        let resp: BalanceResponse =
            serde_json::from_str(r#"{"code":1003,"data":{"balances":[]}}"#).unwrap();
        assert_ne!(resp.code, 0);
    }
}

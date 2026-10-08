//! Read projections: list/summary/alert/dock views over stored subscriptions,
//! the active-per-catalog pins, and API-key retrieval.

use crate::accounts::dto::{MonthlySpendEntry, SubscriptionDto, UsageSummary};
use crate::subscription::BillingCycle;
use crate::{alerts, crypto, storage};
use ss_core::infra::error::AppError;

use super::helpers::{fill_active, map_err};

// ── List ──────────────────────────────────────────────────────────────

pub fn list_subscriptions() -> Result<Vec<SubscriptionDto>, AppError> {
    let subs = storage::list_subscriptions().map_err(map_err)?;
    let snapshots = storage::list_usage_snapshots().map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    Ok(subs
        .into_iter()
        .map(|sub| {
            let usage = snapshots.get(&sub.id).cloned();
            fill_active(SubscriptionDto::from_parts(sub, usage), &active)
        })
        .collect())
}

// ── Summary header ────────────────────────────────────────────────────

pub fn get_usage_summary() -> Result<UsageSummary, AppError> {
    use std::collections::BTreeMap;
    let subs = storage::list_subscriptions().map_err(map_err)?;
    let alerts = alerts::compute_alerts()
        .map_err(map_err)
        .unwrap_or_default();

    let mut totals: BTreeMap<String, f64> = BTreeMap::new();
    let mut reauth = 0usize;
    for sub in &subs {
        if sub.requires_reauth {
            reauth += 1;
        }
        let price = sub.monthly_price.unwrap_or(0.0);
        let amount = match sub.billing_cycle {
            BillingCycle::Monthly => price,
            BillingCycle::Annual => price / 12.0,
            // Prepaid/API-key balance and one-shots are not monthly burn.
            BillingCycle::OneTime | BillingCycle::ApiKey => 0.0,
        };
        *totals.entry(sub.currency.clone()).or_insert(0.0) += amount;
    }

    Ok(UsageSummary {
        monthly_spend: totals
            .into_iter()
            .map(|(currency, amount)| MonthlySpendEntry { currency, amount })
            .collect(),
        total_subscriptions: subs.len(),
        alert_count: alerts.len(),
        reauth_count: reauth,
    })
}

// ── Dock menu ─────────────────────────────────────────────────────────

/// Rows for the macOS Dock right-click menu and tray menu: one `"<account> · <status>"` line
/// per subscription, ordered most-urgent (least remaining) first.
pub fn dock_menu_lines_for_lang(lang: &str) -> Vec<String> {
    let subs = match storage::list_subscriptions() {
        Ok(subs) => subs,
        Err(_) => return Vec::new(),
    };
    let snapshots = match storage::list_usage_snapshots() {
        Ok(snapshots) => snapshots,
        Err(_) => return Vec::new(),
    };
    let is_zh = lang.starts_with("zh");
    let mut rows: Vec<(i32, String)> = subs
        .iter()
        .map(|sub| {
            let label = sub.display_name.trim();
            let label = if label.is_empty() {
                &sub.catalog_id
            } else {
                label
            };
            if let Some(usage) = snapshots.get(&sub.id)
                && let Some((priority, summary)) =
                    crate::dock_usage::snapshot_menu_summary(usage, lang)
            {
                return (priority, format!("{label} · {summary}"));
            }
            let not_synced = if is_zh { "未同步" } else { "Not synced" };
            (3000, format!("{label} · {not_synced}"))
        })
        .collect();
    rows.sort_by_key(|(priority, _)| *priority);
    rows.into_iter().map(|(_, line)| line).collect()
}

/// Convenience helper for default Chinese lines.
pub fn dock_menu_lines() -> Vec<String> {
    dock_menu_lines_for_lang("zh-CN")
}

// ── Active-per-catalog pins ───────────────────────────────────────────

/// Return `catalog_id -> active subscription_id` for every catalog that
/// currently has an account pinned. Catalogs without a pin are absent.
pub fn get_active_subscriptions() -> Result<std::collections::HashMap<String, String>, AppError> {
    storage::list_active_per_catalog().map_err(map_err)
}

// ── API key retrieval (for clipboard copy) ──────────────────────────────

/// Return the decrypted plaintext API key for a subscription.
///
/// Only works when the subscription has an `api_key_encrypted` credential
/// (i.e. API-key mode or Cookie-mode where the provider stores a key).
/// Returns `null` when no key is available.
pub fn get_subscription_api_key(id: String) -> Result<Option<String>, AppError> {
    let sub = storage::get_subscription(&id).map_err(map_err)?;
    let key = sub
        .api_key_encrypted
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(crypto::decrypt)
        .filter(|pt| !pt.is_empty());
    Ok(key)
}

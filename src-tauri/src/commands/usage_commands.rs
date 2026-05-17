//! Tauri commands for the `/usage` page (subscription tracker).
//!
//! All commands return `Result<T, AppError>` consistent with the rest of the
//! handler surface. Heavy work runs on `tokio::task::spawn_blocking` only
//! when truly needed; for ~18 subscriptions JSON I/O is fast enough.

use chrono::Utc;
use skillstar_core::infra::error::AppError;
use skillstar_usage::catalog::AuthMode;
use skillstar_usage::subscription::{BillingCycle, Subscription};
use skillstar_usage::{alerts, catalog, crypto, fetchers, storage, UsageError};

use super::usage_dto::*;

fn map_err(e: UsageError) -> AppError {
    AppError::Other(format!("Usage: {}", e))
}

fn ensure_catalog(id: &str) -> Result<catalog::CatalogEntry, AppError> {
    catalog::find(id)
        .ok_or_else(|| AppError::Other(format!("Usage: unknown catalog id `{}`", id)))
}

// ── Catalog ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_usage_catalog() -> Vec<CatalogEntryDto> {
    catalog::catalog().into_iter().map(Into::into).collect()
}

// ── CRUD ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_subscriptions() -> Result<Vec<SubscriptionDto>, AppError> {
    let subs = storage::list_subscriptions().map_err(map_err)?;
    let snapshots = storage::list_usage_snapshots().map_err(map_err)?;
    Ok(subs
        .into_iter()
        .map(|sub| {
            let usage = snapshots.get(&sub.id).cloned();
            SubscriptionDto::from_parts(sub, usage)
        })
        .collect())
}

#[tauri::command]
pub fn create_subscription(input: CreateSubscriptionInput) -> Result<SubscriptionDto, AppError> {
    let entry = ensure_catalog(&input.catalog_id)?;
    // Validate auth_mode against catalog whitelist.
    if !entry.auth_modes.contains(&input.auth_mode) {
        return Err(AppError::Other(format!(
            "Usage: `{}` 不支持 {:?} 模式",
            entry.id, input.auth_mode
        )));
    }

    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: input.catalog_id,
        display_name: input
            .display_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| entry.display_name.to_string()),
        auth_mode: input.auth_mode,
        plan_tier: input.plan_tier,
        monthly_price: input.monthly_price,
        currency: input
            .currency
            .unwrap_or_else(|| entry.default_currency.to_string()),
        billing_cycle: input.billing_cycle.unwrap_or(BillingCycle::Monthly),
        start_date: input.start_date.unwrap_or(0),
        renew_date: input.renew_date.unwrap_or(0),
        auto_renew: input.auto_renew.unwrap_or(false),
        api_key_encrypted: input
            .api_key
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(crypto::encrypt),
        access_token_encrypted: None,
        refresh_token_encrypted: None,
        access_token_expires_at: None,
        oauth_account_id: None,
        oauth_region: input.oauth_region,
        requires_reauth: false,
        manual_quota: input.manual_quota,
        note: input.note,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };
    let saved = storage::upsert_subscription(sub).map_err(map_err)?;
    Ok(SubscriptionDto::from_parts(saved, None))
}

#[tauri::command]
pub fn update_subscription(
    id: String,
    input: UpdateSubscriptionInput,
) -> Result<SubscriptionDto, AppError> {
    let mut sub = storage::get_subscription(&id).map_err(map_err)?;
    if let Some(name) = input.display_name {
        if !name.trim().is_empty() {
            sub.display_name = name;
        }
    }
    if input.plan_tier.is_some() {
        sub.plan_tier = input.plan_tier;
    }
    if input.monthly_price.is_some() {
        sub.monthly_price = input.monthly_price;
    }
    if let Some(c) = input.currency {
        if !c.is_empty() {
            sub.currency = c;
        }
    }
    if let Some(cycle) = input.billing_cycle {
        sub.billing_cycle = cycle;
    }
    if let Some(start) = input.start_date {
        sub.start_date = start;
    }
    if let Some(renew) = input.renew_date {
        sub.renew_date = renew;
    }
    if let Some(auto) = input.auto_renew {
        sub.auto_renew = auto;
    }
    if let Some(key) = input.api_key.filter(|k| !k.is_empty()) {
        sub.api_key_encrypted = Some(crypto::encrypt(&key));
    }
    if input.manual_quota.is_some() {
        sub.manual_quota = input.manual_quota;
    }
    if input.note.is_some() {
        sub.note = input.note;
    }
    let saved = storage::upsert_subscription(sub).map_err(map_err)?;
    let usage = storage::get_usage_snapshot(&id).map_err(map_err)?;
    Ok(SubscriptionDto::from_parts(saved, usage))
}

#[tauri::command]
pub fn delete_subscription(id: String) -> Result<(), AppError> {
    storage::delete_subscription(&id).map_err(map_err)
}

#[tauri::command]
pub fn reorder_subscriptions(ids: Vec<String>) -> Result<(), AppError> {
    storage::reorder_subscriptions(&ids).map_err(map_err)
}

// ── Usage refresh ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn refresh_subscription_usage(id: String) -> Result<SubscriptionDto, AppError> {
    let mut sub = storage::get_subscription(&id).map_err(map_err)?;
    let usage = match fetchers::refresh(&mut sub).await {
        Ok(usage) => {
            // Persist any token updates that may have happened during refresh.
            sub.requires_reauth = false;
            storage::upsert_subscription(sub.clone()).map_err(map_err)?;
            storage::save_usage_snapshot(usage.clone()).map_err(map_err)?;
            Some(usage)
        }
        Err(UsageError::AuthRequired) => {
            sub.requires_reauth = true;
            storage::upsert_subscription(sub.clone()).map_err(map_err)?;
            let snapshot = skillstar_usage::subscription::SubscriptionUsage {
                subscription_id: sub.id.clone(),
                fetched_at: chrono::Utc::now().timestamp(),
                error: Some("登录已失效，请重新授权。".into()),
                ..Default::default()
            };
            storage::save_usage_snapshot(snapshot.clone()).map_err(map_err)?;
            Some(snapshot)
        }
        Err(other) => {
            let snapshot = skillstar_usage::subscription::SubscriptionUsage {
                subscription_id: sub.id.clone(),
                fetched_at: chrono::Utc::now().timestamp(),
                error: Some(other.to_string()),
                ..Default::default()
            };
            storage::save_usage_snapshot(snapshot.clone()).map_err(map_err)?;
            Some(snapshot)
        }
    };
    Ok(SubscriptionDto::from_parts(sub, usage))
}

#[tauri::command]
pub async fn refresh_all_subscriptions() -> Result<Vec<SubscriptionDto>, AppError> {
    let subs = storage::list_subscriptions().map_err(map_err)?;
    let mut results = Vec::with_capacity(subs.len());
    for sub in subs {
        // Manual entries skip the round-trip but still get a "snapshot" with
        // whatever the user has on file.
        if sub.auth_mode == AuthMode::Manual {
            let usage = storage::get_usage_snapshot(&sub.id)
                .map_err(map_err)?
                .or_else(|| {
                    Some(skillstar_usage::subscription::SubscriptionUsage {
                        subscription_id: sub.id.clone(),
                        fetched_at: chrono::Utc::now().timestamp(),
                        plan_name: sub.plan_tier.clone(),
                        ..Default::default()
                    })
                });
            results.push(SubscriptionDto::from_parts(sub, usage));
            continue;
        }
        let id = sub.id.clone();
        match refresh_subscription_usage(id).await {
            Ok(dto) => results.push(dto),
            Err(e) => {
                tracing::warn!("[usage] refresh {} failed: {}", sub.id, e);
                let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
                results.push(SubscriptionDto::from_parts(sub, usage));
            }
        }
    }
    Ok(results)
}

// ── Alerts ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_subscription_alerts() -> Result<Vec<SubscriptionAlertDto>, AppError> {
    Ok(alerts::compute_alerts()
        .map_err(map_err)?
        .into_iter()
        .map(Into::into)
        .collect())
}

#[tauri::command]
pub fn dismiss_subscription_alert(alert_id: String) -> Result<(), AppError> {
    storage::dismiss_alert(&alert_id).map_err(map_err)
}

// ── Summary header ────────────────────────────────────────────────────

#[tauri::command]
pub fn get_usage_summary() -> Result<UsageSummary, AppError> {
    use std::collections::BTreeMap;
    let subs = storage::list_subscriptions().map_err(map_err)?;
    let alerts = alerts::compute_alerts().map_err(map_err).unwrap_or_default();

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
            BillingCycle::OneTime => 0.0,
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

// ── OAuth (Phase 6 wires the real flows; v1 phase 2 returns clear errors) ─

#[tauri::command]
pub async fn start_oauth_login(
    catalog_id: String,
    region: Option<String>,
) -> Result<OAuthStartDto, AppError> {
    let (auth_url, pending_id) =
        fetchers::oauth::start_login(&catalog_id, region.as_deref())
            .await
            .map_err(map_err)?;
    Ok(OAuthStartDto {
        pending_id,
        auth_url,
    })
}

#[tauri::command]
pub async fn await_oauth_completion(pending_id: String) -> Result<SubscriptionDto, AppError> {
    use skillstar_usage::oauth::pending_state;
    let rx = pending_state::take_receiver(&pending_id)
        .ok_or_else(|| AppError::Other("Usage: pending_id 不存在或已被取走".into()))?;
    let result = rx
        .await
        .map_err(|_| AppError::Other("Usage: OAuth 等待中断".into()))?;
    pending_state::remove(&pending_id);
    let sub = result.map_err(map_err)?;
    let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
    Ok(SubscriptionDto::from_parts(sub, usage))
}

#[tauri::command]
pub fn cancel_oauth_login(pending_id: String) -> Result<(), AppError> {
    use skillstar_usage::oauth::pending_state;
    pending_state::cancel(&pending_id).map_err(map_err)
}

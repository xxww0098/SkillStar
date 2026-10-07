//! Subscription CRUD: create/update/delete/reorder.

use crate::catalog::AuthMode;
use crate::cookie_jar;
use crate::subscription::{BillingCycle, Subscription};
use crate::{crypto, storage};
use chrono::Utc;
use ss_core::infra::error::AppError;

use super::helpers::{ensure_catalog, fill_active, map_err, mark_credentials_rotated};
use crate::accounts::dto::{CreateSubscriptionInput, SubscriptionDto, UpdateSubscriptionInput};

// ── CRUD ──────────────────────────────────────────────────────────────

/// Token-import rows are written only by `import_subscription_token`.
/// The generic form would persist a card with no credential.
fn reject_token_import_write(auth_mode: AuthMode) -> Result<(), AppError> {
    if auth_mode != AuthMode::TokenImport {
        return Ok(());
    }
    Err(AppError::Other(
        "Usage: `token-import` 账号只能通过 `import_subscription_token` 导入，不能用创建或更新订阅写入"
            .into(),
    ))
}

pub fn create_subscription(input: CreateSubscriptionInput) -> Result<SubscriptionDto, AppError> {
    let entry = ensure_catalog(&input.catalog_id)?;
    reject_token_import_write(input.auth_mode)?;
    // Validate auth_mode against catalog whitelist.
    if !entry.auth_modes.contains(&input.auth_mode) {
        return Err(AppError::Other(format!(
            "Usage: `{}` 不支持 {:?} 模式",
            entry.id, input.auth_mode
        )));
    }

    let now = Utc::now().timestamp();
    let cookie_jar_encrypted = if let Some(raw) = input
        .cookie_header
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        let entries = cookie_jar::parse_cookie_header(raw);
        if entries.is_empty() {
            return Err(AppError::Other(
                "Cookie 解析失败：请从 DevTools 复制 `name=value; ...` 格式，不要只粘贴 `Cookie:` 标签。".into(),
            ));
        }
        let json = cookie_jar::serialize_cookie_jar(&entries);
        Some(crypto::encrypt(&json))
    } else {
        None
    };
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
        platform_token_encrypted: input
            .platform_token
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(crypto::encrypt),
        access_token_encrypted: None,
        refresh_token_encrypted: None,
        access_token_expires_at: None,
        id_token_encrypted: None,
        oauth_account_id: None,
        oauth_region: input.oauth_region,
        requires_reauth: false,
        provider_state_encrypted: None,
        cookie_jar_encrypted,
        cookie_session_expires_at: None,
        manual_quota: input.manual_quota,
        note: input.note,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };
    let saved = storage::upsert_subscription(sub).map_err(map_err)?;
    // If this catalog has no active account yet, auto-pin the brand-new one.
    let active = storage::list_active_per_catalog().unwrap_or_default();
    if !active.contains_key(&saved.catalog_id) {
        let _ = storage::set_active_subscription(&saved.catalog_id, &saved.id);
    }
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    Ok(fill_active(
        SubscriptionDto::from_parts(saved, None),
        &active,
    ))
}

pub async fn update_subscription(
    id: String,
    input: UpdateSubscriptionInput,
) -> Result<SubscriptionDto, AppError> {
    let catalog_id = storage::get_subscription(&id).map_err(map_err)?.catalog_id;
    crate::refresh_guard::with_catalog_lock(&catalog_id, || async move {
        update_subscription_locked(id, input)
    })
    .await
    .map_err(map_err)?
}

fn update_subscription_locked(
    id: String,
    input: UpdateSubscriptionInput,
) -> Result<SubscriptionDto, AppError> {
    let mut sub = storage::get_subscription(&id).map_err(map_err)?;
    reject_token_import_write(sub.auth_mode)?;
    if let Some(name) = input.display_name
        && !name.trim().is_empty()
    {
        sub.display_name = name;
    }
    if input.plan_tier.is_some() {
        sub.plan_tier = input.plan_tier;
    }
    if input.monthly_price.is_some() {
        sub.monthly_price = input.monthly_price;
    }
    if let Some(c) = input.currency
        && !c.is_empty()
    {
        sub.currency = c;
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
        mark_credentials_rotated(&mut sub);
    }
    if input.clear_platform_token.unwrap_or(false) {
        sub.platform_token_encrypted = None;
        mark_credentials_rotated(&mut sub);
    } else if let Some(token) = input
        .platform_token
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        sub.platform_token_encrypted = Some(crypto::encrypt(token.trim()));
        mark_credentials_rotated(&mut sub);
    }
    if let Some(raw) = input.cookie_header.filter(|c| !c.trim().is_empty()) {
        let entries = cookie_jar::parse_cookie_header(&raw);
        if entries.is_empty() {
            return Err(AppError::Other(
                "Cookie 解析失败：请从 DevTools 复制 `name=value; ...` 格式，不要只粘贴 `Cookie:` 标签。".into(),
            ));
        }
        let json = cookie_jar::serialize_cookie_jar(&entries);
        sub.cookie_jar_encrypted = Some(crypto::encrypt(&json));
        mark_credentials_rotated(&mut sub);
    }
    if input.manual_quota.is_some() {
        sub.manual_quota = input.manual_quota;
    }
    if input.note.is_some() {
        sub.note = input.note;
    }
    let saved = storage::upsert_subscription(sub).map_err(map_err)?;
    let usage = storage::get_usage_snapshot(&id).map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    Ok(fill_active(
        SubscriptionDto::from_parts(saved, usage),
        &active,
    ))
}

pub async fn delete_subscription(id: String) -> Result<(), AppError> {
    let catalog_id = storage::get_subscription(&id).map_err(map_err)?.catalog_id;
    crate::refresh_guard::with_catalog_lock(&catalog_id, || async move {
        let sub = storage::get_subscription(&id).map_err(map_err)?;
        crate::usage_switch::forget_subscription_session(&sub.catalog_id, &sub.id)
            .map_err(map_err)?;
        storage::delete_subscription(&id).map_err(map_err)?;
        Ok(())
    })
    .await
    .map_err(map_err)?
}

pub fn reorder_subscriptions(ids: Vec<String>) -> Result<(), AppError> {
    storage::reorder_subscriptions(&ids).map_err(map_err)
}

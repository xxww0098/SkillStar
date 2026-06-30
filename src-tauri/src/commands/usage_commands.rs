//! Tauri commands for the `/usage` page (subscription tracker).
//!
//! All commands return `Result<T, AppError>` consistent with the rest of the
//! handler surface. Heavy work runs on `tokio::task::spawn_blocking` only
//! when truly needed; for ~18 subscriptions JSON I/O is fast enough.

use chrono::Utc;
use futures::future::join_all;
use skillstar_core::config::proxy;
use skillstar_core::infra::error::AppError;
use tauri::AppHandle;
use skillstar_usage::catalog::AuthMode;
use skillstar_usage::cookie_jar;
use skillstar_usage::subscription::{BillingCycle, Subscription};
use skillstar_usage::{UsageError, alerts, catalog, crypto, fetchers, storage};

use super::usage_dto::*;

fn map_err(e: UsageError) -> AppError {
    let message = append_network_hint(e.to_string());
    AppError::Other(format!("Usage: {}", message))
}

/// Rotating stored credentials should drop any prior auth-expired latch so the
/// UI can refresh again instead of staying stuck on the re-auth affordance.
fn mark_credentials_rotated(sub: &mut Subscription) {
    sub.requires_reauth = false;
    sub.cookie_session_expires_at = None;
}

fn append_network_hint(message: String) -> String {
    if !looks_like_network_transport_error(&message) || message.contains("网络代理") {
        return message;
    }

    format!("{}。{}", message, usage_network_hint(&message))
}

fn looks_like_network_transport_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "error sending request",
        "operation timed out",
        "timed out",
        "connection refused",
        "connection reset",
        "dns",
        "failed to lookup address",
        "tcp connect error",
        "network is unreachable",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn usage_network_hint(message: &str) -> String {
    let targets = network_hint_targets(message);
    match proxy::load_config() {
        Ok(config) if config.enabled && !config.host.trim().is_empty() => format!(
            "请检查 SkillStar 网络代理（{}://{}:{}）能访问 {}，或切换到可访问这些服务的节点后重试",
            config.proxy_type.as_scheme(),
            config.host.trim(),
            config.port,
            targets
        ),
        _ => format!(
            "当前 SkillStar 网络代理未启用；如果所在网络无法直连 {}，请在设置 > 网络代理启用代理后重试",
            targets
        ),
    }
}

fn network_hint_targets(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("auth.x.ai")
        || lower.contains("grok.com")
        || lower.contains("grok oauth")
        || lower.contains("grok token")
        || lower.contains("grok refresh")
        || lower.contains("grok billing")
    {
        return "x.ai / Grok";
    }
    "Google / GitHub 等海外服务"
}

fn ensure_catalog(id: &str) -> Result<catalog::CatalogEntry, AppError> {
    catalog::find(id).ok_or_else(|| AppError::Other(format!("Usage: unknown catalog id `{}`", id)))
}

/// Stamp `is_active` on a DTO based on the active-per-catalog map.
fn fill_active(
    mut dto: SubscriptionDto,
    active: &std::collections::HashMap<String, String>,
) -> SubscriptionDto {
    dto.is_active = active
        .get(&dto.catalog_id)
        .is_some_and(|sub_id| sub_id == &dto.id);
    dto
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
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    Ok(subs
        .into_iter()
        .map(|sub| {
            let usage = snapshots.get(&sub.id).cloned();
            fill_active(SubscriptionDto::from_parts(sub, usage), &active)
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
        fingerprint_id: input.fingerprint_id.filter(|s| !s.trim().is_empty()),
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

#[tauri::command]
pub fn update_subscription(
    id: String,
    input: UpdateSubscriptionInput,
) -> Result<SubscriptionDto, AppError> {
    let mut sub = storage::get_subscription(&id).map_err(map_err)?;
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
    if input.clear_platform_token {
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
    // Fingerprint binding: `clear` wins over `set` so the frontend can be
    // explicit when the user picks "无（默认）".
    if input.clear_fingerprint {
        sub.fingerprint_id = None;
    } else if let Some(fp_id) = input.fingerprint_id {
        sub.fingerprint_id = Some(fp_id);
    }
    let saved = storage::upsert_subscription(sub).map_err(map_err)?;
    let usage = storage::get_usage_snapshot(&id).map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    Ok(fill_active(
        SubscriptionDto::from_parts(saved, usage),
        &active,
    ))
}

#[tauri::command]
pub fn delete_subscription(app: AppHandle, id: String) -> Result<(), AppError> {
    storage::delete_subscription(&id).map_err(map_err)?;
    // Dismiss any floating card window bound to the deleted subscription.
    crate::commands::usage_windows::close_card_for_subscription(&app, &id);
    Ok(())
}

#[tauri::command]
pub fn reorder_subscriptions(ids: Vec<String>) -> Result<(), AppError> {
    storage::reorder_subscriptions(&ids).map_err(map_err)
}

// ── Usage refresh ─────────────────────────────────────────────────────

async fn refresh_subscription_usage_inner(id: String) -> Result<SubscriptionDto, AppError> {
    let mut sub = storage::get_subscription(&id).map_err(map_err)?;
    let catalog_id = sub.catalog_id.clone();
    skillstar_usage::refresh_guard::with_catalog_refresh(&catalog_id, || async {
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
        let active = storage::list_active_per_catalog().map_err(map_err)?;
        Ok(fill_active(
            SubscriptionDto::from_parts(sub, usage),
            &active,
        ))
    })
    .await
}

#[tauri::command]
pub async fn refresh_subscription_usage(id: String) -> Result<SubscriptionDto, AppError> {
    refresh_subscription_usage_inner(id).await
}

#[tauri::command]
pub async fn refresh_all_subscriptions() -> Result<Vec<SubscriptionDto>, AppError> {
    skillstar_usage::refresh_guard::with_refresh_all_lock(|| async {
        let subs = storage::list_subscriptions().map_err(map_err)?;
        let active_map = storage::list_active_per_catalog().map_err(map_err)?;

        let tasks = subs.into_iter().map(|sub| {
            let active_map = active_map.clone();
            async move {
                let sort_index = sub.sort_index;
                let dto = if sub.auth_mode == AuthMode::Manual {
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
                    fill_active(SubscriptionDto::from_parts(sub, usage), &active_map)
                } else {
                    let id = sub.id.clone();
                    let fallback = sub.clone();
                    match refresh_subscription_usage_inner(id).await {
                        Ok(dto) => dto,
                        Err(e) => {
                            tracing::warn!("[usage] refresh {} failed: {}", fallback.id, e);
                            let usage = storage::get_usage_snapshot(&fallback.id).map_err(map_err)?;
                            fill_active(
                                SubscriptionDto::from_parts(fallback, usage),
                                &active_map,
                            )
                        }
                    }
                };
                Ok::<_, AppError>((sort_index, dto))
            }
        });

        let mut results: Vec<SubscriptionDto> = join_all(tasks)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(_, dto)| dto)
            .collect();
        results.sort_by_key(|dto| dto.sort_index);
        Ok(results)
    })
    .await
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
    subscription_id: Option<String>,
) -> Result<OAuthStartDto, AppError> {
    let target_subscription_id = subscription_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let info = fetchers::oauth::start_login(&catalog_id, region.as_deref(), target_subscription_id)
        .await
        .map_err(map_err)?;
    Ok(OAuthStartDto {
        pending_id: info.pending_id,
        auth_url: info.auth_url,
        user_code: info.user_code,
        verification_uri: info.verification_uri,
    })
}

#[tauri::command]
pub async fn import_subscription_from_local(
    catalog_id: String,
) -> Result<SubscriptionDto, AppError> {
    if !skillstar_usage::local_import::local_import_supported(&catalog_id) {
        return Err(AppError::Other(format!(
            "Usage: 不支持从本地导入 {}",
            catalog_id
        )));
    }
    let sub = skillstar_usage::local_import::import_subscription_from_local(&catalog_id)
        .await
        .map_err(map_err)?;
    let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    Ok(fill_active(
        SubscriptionDto::from_parts(sub, usage),
        &active,
    ))
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
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    Ok(fill_active(
        SubscriptionDto::from_parts(sub, usage),
        &active,
    ))
}

#[tauri::command]
pub async fn submit_oauth_callback(
    pending_id: String,
    callback_input: String,
) -> Result<(), AppError> {
    skillstar_usage::oauth::manual_callback::submit(&pending_id, &callback_input)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub fn cancel_oauth_login(pending_id: String) -> Result<(), AppError> {
    use skillstar_usage::oauth::pending_state;
    pending_state::cancel(&pending_id).map_err(map_err)
}

// ── Multi-account: active-per-catalog (Phase 7) ───────────────────────

/// Return `catalog_id -> active subscription_id` for every catalog that
/// currently has an account pinned. Catalogs without a pin are absent.
#[tauri::command]
pub fn get_active_subscriptions() -> Result<std::collections::HashMap<String, String>, AppError> {
    storage::list_active_per_catalog().map_err(map_err)
}

/// Pin `subscription_id` as the active account for its catalog, and push
/// its credentials into the real CLI config (`~/.codex/auth.json`,
/// `~/.zcode/...`, etc.) so the switch actually takes effect in the agent
/// CLI — not just a SkillStar-internal flag.
///
/// Returns the freshly-flagged DTO (with `switch_result` describing whether
/// the CLI write succeeded) so the frontend can swap it in-place and surface
/// any CLI-sync failure.
///
/// The CLI push is best-effort: if it fails, the account is still pinned as
/// active locally (so the UI reflects intent), and the failure is reported
/// via `switch_result` for the user to act on.
#[tauri::command]
pub fn set_active_subscription(
    app: AppHandle,
    subscription_id: String,
) -> Result<SubscriptionDto, AppError> {
    let sub = storage::get_subscription(&subscription_id).map_err(map_err)?;
    let catalog_id = sub.catalog_id.clone();
    let sub_id = sub.id.clone();
    storage::set_active_subscription(&catalog_id, &sub_id).map_err(map_err)?;
    // Push credentials to the real CLI config. Best-effort: a failure here
    // must not un-pin the account; the outcome is surfaced to the UI.
    let outcome = skillstar_app::usage_switch::switch_subscription_to_cli(&sub);
    let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    let mut dto = fill_active(SubscriptionDto::from_parts(sub, usage), &active);
    dto.switch_result = Some(outcome);
    // Notify any open usage card windows so they refresh their is_active badge.
    crate::commands::usage_windows::emit_active_changed(&app, &catalog_id, &sub_id);
    Ok(dto)
}

/// Re-push the active account for `catalog_id` into its CLI config, without
/// changing which account is active. Used by the "重新同步到 CLI" button when
/// a previous switch failed (e.g. missing id_token that has since been
/// refreshed).
#[tauri::command]
pub fn switch_active_subscription_to_cli(
    catalog_id: String,
) -> Result<skillstar_app::usage_switch::SwitchOutcome, AppError> {
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    let Some(sub_id) = active.get(&catalog_id) else {
        return Err(AppError::Other(format!(
            "catalog {catalog_id} 还没有设置活跃账号"
        )));
    };
    let sub = storage::get_subscription(sub_id).map_err(map_err)?;
    Ok(skillstar_app::usage_switch::switch_subscription_to_cli(&sub))
}

/// Drop the pin for `catalog_id`. UI will fall back to no active account
/// for that catalog (typically displayed as a neutral state).
#[tauri::command]
pub fn clear_active_subscription(catalog_id: String) -> Result<(), AppError> {
    storage::clear_active_subscription(&catalog_id).map_err(map_err)
}

// ── API key retrieval (for clipboard copy) ──────────────────────────────

/// Return the decrypted plaintext API key for a subscription.
///
/// Only works when the subscription has an `api_key_encrypted` credential
/// (i.e. API-key mode or Cookie-mode where the provider stores a key).
/// Returns `null` when no key is available.
#[tauri::command]
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

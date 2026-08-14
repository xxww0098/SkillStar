//! Application use cases for the Usage subscription tracker.

use super::dto::*;
use chrono::Utc;
use futures::future::join_all;
use skillstar_core::config::proxy;
use skillstar_core::infra::error::AppError;
use skillstar_usage::catalog::AuthMode;
use skillstar_usage::cookie_jar;
use skillstar_usage::subscription::{BillingCycle, Subscription};
use skillstar_usage::{UsageError, alerts, catalog, crypto, fetchers, storage};

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

pub fn list_usage_catalog() -> Vec<CatalogEntryDto> {
    catalog::catalog().into_iter().map(Into::into).collect()
}

// ── CRUD ──────────────────────────────────────────────────────────────

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
    skillstar_usage::refresh_guard::with_catalog_lock(&catalog_id, || async move {
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
    skillstar_usage::refresh_guard::with_catalog_lock(&catalog_id, || async move {
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

// ── Usage refresh ─────────────────────────────────────────────────────

/// What a failed refresh persists: whether to latch the re-auth affordance,
/// and the snapshot that replaces (or merely annotates) the stored one.
///
/// `AuthRequired` is the **only** verdict allowed to latch `requires_reauth`
/// and blank the card — it is the only one that means "these credentials are
/// dead". A 429 / 5xx / transport blip keeps the last good numbers so a single
/// provider hiccup cannot erase a working account's quota display, and any
/// other failure replaces the snapshot because the old numbers may no longer
/// describe the account. `latch_reauth == false` deliberately does not *clear*
/// an existing latch: a row already awaiting re-authorization must keep its
/// button when the retry happens to hit a 500.
fn refresh_failure(
    subscription_id: &str,
    previous: Option<&skillstar_usage::subscription::SubscriptionUsage>,
    error: &UsageError,
) -> (bool, skillstar_usage::subscription::SubscriptionUsage) {
    use skillstar_usage::subscription::SubscriptionUsage;
    match error {
        UsageError::AuthRequired => (
            true,
            SubscriptionUsage {
                subscription_id: subscription_id.to_string(),
                fetched_at: Utc::now().timestamp(),
                error: Some("登录已失效，请重新授权。".into()),
                ..Default::default()
            },
        ),
        other => (
            false,
            SubscriptionUsage::from_refresh_error(
                subscription_id,
                previous,
                append_network_hint(other.to_string()),
                other.is_transient(),
            ),
        ),
    }
}

async fn refresh_subscription_usage_inner(id: String) -> Result<SubscriptionDto, AppError> {
    // Probe only the catalog before locking; reload the row after acquiring
    // the shared catalog lock so a queued refresh cannot write credentials
    // captured before an account switch rotated them.
    let catalog_id = storage::get_subscription(&id).map_err(map_err)?.catalog_id;
    skillstar_usage::refresh_guard::with_catalog_refresh(&catalog_id, || async move {
        let mut sub = storage::get_subscription(&id).map_err(map_err)?;
        let cli_lease = crate::usage_switch::acquire_cli_refresh_lease(&sub.catalog_id)
            .await
            .map_err(map_err)?;
        crate::usage_switch::adopt_active_cli_session_before_refresh(&mut sub, &cli_lease)
            .map_err(map_err)?;
        // Set by both arms below; a dead auth verdict is the only case that
        // skips the CLI push.
        let should_sync_cli;
        let usage = match fetchers::refresh(&mut sub).await {
            Ok(usage) => {
                should_sync_cli = true;
                // Persist any token updates that may have happened during refresh.
                sub.requires_reauth = false;
                sub = storage::patch_fetcher_state(&sub).map_err(map_err)?;
                storage::save_usage_snapshot(usage.clone()).map_err(map_err)?;
                Some(usage)
            }
            Err(error) => {
                // Only a real auth verdict skips the CLI sync; a provider
                // hiccup may still have rotated credentials worth pushing.
                should_sync_cli = !matches!(error, UsageError::AuthRequired);
                let previous = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
                let (latch_reauth, snapshot) = refresh_failure(&sub.id, previous.as_ref(), &error);
                if latch_reauth {
                    sub.requires_reauth = true;
                }
                // A fetcher may have rotated OAuth credentials before a later
                // billing request failed. Persist that narrow state without
                // replacing user-editable metadata from this network-old row.
                sub = storage::patch_fetcher_state(&sub).map_err(map_err)?;
                storage::save_usage_snapshot(snapshot.clone()).map_err(map_err)?;
                Some(snapshot)
            }
        };
        let switch_result = if should_sync_cli {
            crate::usage_switch::sync_refreshed_active_subscription(&mut sub, &cli_lease)
                .map_err(map_err)?
        } else {
            None
        };
        let active = storage::list_active_per_catalog().map_err(map_err)?;
        let mut dto = fill_active(SubscriptionDto::from_parts(sub, usage), &active);
        dto.switch_result = switch_result.map(SwitchOutcomeDto::from);
        Ok(dto)
    })
    .await
    .map_err(map_err)?
}

pub async fn refresh_subscription_usage(id: String) -> Result<SubscriptionDto, AppError> {
    refresh_subscription_usage_inner(id).await
}

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
            if let Some(usage) = snapshots.get(&sub.id) {
                if let Some((priority, summary)) =
                    skillstar_usage::dock_usage::snapshot_menu_summary(usage, lang)
                {
                    return (priority, format!("{label} · {summary}"));
                }
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

/// Refresh every subscription, or only one catalog's when `catalog_id` is set.
///
/// The Grok page asks for `Some("xai")` and must not drag every other vendor's
/// endpoint along with it — ignoring the argument turned a one-provider button
/// into a full sweep, N seconds long thanks to the per-catalog spacing.
/// Non-refreshed rows are still returned (from their stored snapshot) so the
/// caller can keep rendering the full list.
pub async fn refresh_all_subscriptions(
    catalog_id: Option<String>,
) -> Result<Vec<SubscriptionDto>, AppError> {
    let catalog_filter = catalog_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    if let Some(id) = catalog_filter.as_deref() {
        ensure_catalog(id)?;
    }
    skillstar_usage::refresh_guard::with_refresh_all_lock(|| async {
        let subs = storage::list_subscriptions().map_err(map_err)?;
        let active_map = storage::list_active_per_catalog().map_err(map_err)?;

        let tasks = subs.into_iter().map(|sub| {
            let catalog_filter = catalog_filter.clone();
            let active_map = active_map.clone();
            async move {
                let sort_index = sub.sort_index;
                let out_of_scope = catalog_filter
                    .as_deref()
                    .is_some_and(|wanted| wanted != sub.catalog_id);
                let dto = if out_of_scope {
                    // Not this refresh's catalog: report what storage already
                    // knows instead of touching the provider.
                    let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
                    fill_active(SubscriptionDto::from_parts(sub, usage), &active_map)
                } else if sub.auth_mode == AuthMode::Manual {
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
                            let usage =
                                storage::get_usage_snapshot(&fallback.id).map_err(map_err)?;
                            fill_active(SubscriptionDto::from_parts(fallback, usage), &active_map)
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

pub fn get_subscription_alerts() -> Result<Vec<SubscriptionAlertDto>, AppError> {
    Ok(alerts::compute_alerts()
        .map_err(map_err)?
        .into_iter()
        .map(Into::into)
        .collect())
}

pub fn dismiss_subscription_alert(alert_id: String) -> Result<(), AppError> {
    storage::dismiss_alert(&alert_id).map_err(map_err)
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

// ── OAuth (Phase 6 wires the real flows; v1 phase 2 returns clear errors) ─

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

pub async fn await_oauth_completion(pending_id: String) -> Result<SubscriptionDto, AppError> {
    use skillstar_usage::oauth::pending_state;
    let rx = pending_state::take_receiver(&pending_id)
        .ok_or_else(|| AppError::Other("Usage: pending_id 不存在或已被取走".into()))?;
    let result = rx
        .await
        .map_err(|_| AppError::Other("Usage: OAuth 等待中断".into()))?;
    pending_state::remove(&pending_id);
    let mut sub = result.map_err(map_err)?;
    let mut switch_result = None;
    if sub.catalog_id == "xai"
        && storage::get_active_subscription("xai")
            .map_err(map_err)?
            .as_deref()
            == Some(sub.id.as_str())
    {
        // OAuth can rotate the active row's credentials or even bind its stable
        // row id to a different xAI subject. Re-activate immediately so the UI
        // cannot say "active" while auth.json still represents the old token.
        let activation = crate::usage_switch::activate_subscription(&sub.id)
            .await
            .map_err(map_err)?;
        sub = activation.subscription;
        switch_result = Some(activation.switch_result);
    }
    let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    let mut dto = fill_active(SubscriptionDto::from_parts(sub, usage), &active);
    dto.switch_result = switch_result.map(SwitchOutcomeDto::from);
    Ok(dto)
}

pub async fn submit_oauth_callback(
    pending_id: String,
    callback_input: String,
) -> Result<(), AppError> {
    skillstar_usage::oauth::manual_callback::submit(&pending_id, &callback_input)
        .await
        .map_err(map_err)
}

pub fn cancel_oauth_login(pending_id: String) -> Result<(), AppError> {
    use skillstar_usage::oauth::pending_state;
    pending_state::cancel(&pending_id).map_err(map_err)
}

// ── Multi-account: active-per-catalog (Phase 7) ───────────────────────

/// Return `catalog_id -> active subscription_id` for every catalog that
/// currently has an account pinned. Catalogs without a pin are absent.
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
/// Most CLI pushes are best-effort. Grok uses a stricter transaction: if its
/// credential validation/write fails, the previous active pin is restored so
/// the UI cannot claim an account that the CLI did not actually activate.
pub async fn set_active_subscription(subscription_id: String) -> Result<SubscriptionDto, AppError> {
    let activation = crate::usage_switch::activate_subscription(&subscription_id)
        .await
        .map_err(map_err)?;
    let sub = activation.subscription;
    let outcome = activation.switch_result;
    let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    let mut dto = fill_active(SubscriptionDto::from_parts(sub, usage), &active);
    dto.switch_result = Some(outcome.into());
    Ok(dto)
}

/// Re-push the active account for `catalog_id` into its CLI config, without
/// changing which account is active. Used by the "重新同步到 CLI" button when
/// a previous switch failed (e.g. missing id_token that has since been
/// refreshed).
///
/// Returns the DTO rather than the domain outcome: `set_active_subscription`
/// already contracts against `SwitchOutcomeDto`, and two commands describing
/// the same event through two different types is how a new field lands on one
/// of them and not the other.
pub async fn switch_active_subscription_to_cli(
    catalog_id: String,
) -> Result<SwitchOutcomeDto, AppError> {
    crate::usage_switch::resync_active_subscription(&catalog_id)
        .await
        .map(SwitchOutcomeDto::from)
        .map_err(map_err)
}

/// Which account each CLI is *actually* serving, keyed by catalog id.
///
/// The pin returned by [`get_active_subscriptions`] is a cache of this. When
/// the two disagree the file wins — it is what the CLI opens — so the
/// "current" badge is drawn from here, and only falls back to the pin for
/// catalogs absent from this map (no CLI adapter behind them, or unreadable).
pub async fn reconcile_cli_accounts()
-> Result<std::collections::HashMap<String, CliAccountStateDto>, AppError> {
    Ok(crate::usage_switch::reconcile_cli_accounts()
        .await
        .map_err(map_err)?
        .into_iter()
        .map(|(catalog_id, state)| (catalog_id, CliAccountStateDto::from(state)))
        .collect())
}

/// Drop the pin for `catalog_id`. UI will fall back to no active account
/// for that catalog (typically displayed as a neutral state).
pub fn clear_active_subscription(catalog_id: String) -> Result<(), AppError> {
    storage::clear_active_subscription(&catalog_id).map_err(map_err)
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

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;

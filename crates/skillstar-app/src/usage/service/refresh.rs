//! Usage refresh: the single-subscription refresh, the xAI quota reset, and
//! the catalog-scoped refresh-all sweep, plus the refresh-failure snapshot
//! rules that decide who may latch re-auth or blank a card.

use chrono::Utc;
use futures::future::join_all;
use skillstar_core::infra::error::AppError;
use skillstar_usage::catalog::AuthMode;
use skillstar_usage::{UsageError, fetchers, storage};

use super::helpers::{append_network_hint, ensure_catalog, fill_active, map_err};
use crate::usage::dto::{SubscriptionDto, SwitchOutcomeDto};

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
pub(super) fn refresh_failure(
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
        let cli_lease = skillstar_usage::usage_switch::acquire_cli_refresh_lease(&sub.catalog_id)
            .await
            .map_err(map_err)?;
        skillstar_usage::usage_switch::adopt_active_cli_session_before_refresh(&mut sub, &cli_lease)
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
            skillstar_usage::usage_switch::sync_refreshed_active_subscription(&mut sub, &cli_lease)
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

/// Consume one real Grok reset credit for exactly one subscription card.
/// Unlike [`refresh_subscription_usage`], this calls xAI's provider-side reset
/// mutation before reading the resulting billing snapshot.
pub async fn reset_subscription_quota(id: String) -> Result<SubscriptionDto, AppError> {
    let catalog_id = storage::get_subscription(&id).map_err(map_err)?.catalog_id;
    if catalog_id != "xai" {
        return Err(AppError::Other(format!(
            "Usage: quota reset is only supported for Grok subscriptions (got `{catalog_id}`)"
        )));
    }

    skillstar_usage::refresh_guard::with_catalog_refresh(&catalog_id, || async move {
        let mut sub = storage::get_subscription(&id).map_err(map_err)?;
        if sub.catalog_id != "xai" || sub.auth_mode != AuthMode::OAuth {
            return Err(AppError::Other(
                "Usage: Grok quota reset requires an xAI OAuth subscription".into(),
            ));
        }

        let reset_result = skillstar_usage::fetchers::oauth::xai::reset_quota(&mut sub).await;
        // A reset can refresh an expiring OAuth access token before the
        // provider mutation. Persist that narrow credential change even when
        // the later billing projection fails, so the next click does not use
        // an obsolete token.
        sub = storage::patch_fetcher_state(&sub).map_err(map_err)?;

        let usage = match reset_result {
            Ok(usage) => {
                sub.requires_reauth = false;
                storage::patch_fetcher_state(&sub).map_err(map_err)?;
                storage::save_usage_snapshot(usage.clone()).map_err(map_err)?;
                usage
            }
            Err(error) => {
                if matches!(error, UsageError::AuthRequired) {
                    sub.requires_reauth = true;
                    storage::patch_fetcher_state(&sub).map_err(map_err)?;
                }
                return Err(map_err(error));
            }
        };

        let active = storage::list_active_per_catalog().map_err(map_err)?;
        Ok(fill_active(
            SubscriptionDto::from_parts(sub, Some(usage)),
            &active,
        ))
    })
    .await
    .map_err(map_err)?
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

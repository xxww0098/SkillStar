//! Multi-account switching: pin the active account per catalog, push its
//! credentials into the real CLI configs, and reconcile what each CLI
//! actually serves.

use crate::storage;
use ss_core::infra::error::AppError;

use super::helpers::{fill_active, map_err};
use crate::accounts::dto::{CliAccountStateDto, SubscriptionDto, SwitchOutcomeDto};

// ── Multi-account: active-per-catalog (Phase 7) ───────────────────────

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
/// The pin returned by [`crate::accounts::get_active_subscriptions`] is a cache
/// of this. When the two disagree the file wins — it is what the CLI opens —
/// so the "current" badge is drawn from here, and only falls back to the pin
/// for catalogs absent from this map (no CLI adapter behind them, or
/// unreadable).
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

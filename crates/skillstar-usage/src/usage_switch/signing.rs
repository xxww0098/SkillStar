//! Signing material seam (live-first): upstream signing no longer treats
//! "decrypt the stored row's token" as the source of truth.
//!
//! Order of truth (specs/usage-models-evolution slice 02):
//!
//! 1. [`Custody::probe`] decides who the CLI is serving right now —
//!    [`LinkState::LinkedTo`] → the credential the CLI actually opens
//!    (`Freshness::Live`); [`LinkState::Diverged`] → the same credential, but
//!    attributable to no known subscription (`Freshness::Diverged`: the token
//!    still signs, the ledger cannot attribute); [`LinkState::Missing`] → the
//!    CLI has no usable credential, degrade to the stored row
//!    (`Freshness::Row`).
//! 2. Catalogs without a CLI target (IDE adapters or row-only accounts) have
//!    no live file that could disagree with the row; the row is the only
//!    truth, so they go straight to `Freshness::Row`.
//!
//! The seam is read-only and synchronous: it takes no custody CLI lease,
//! enters no catalog serialization domain, and creates no directory or file
//! (signing_tests pins that; reading storage does touch storage's own
//! pre-existing process lock, which account() already did before the
//! re-routing — it is not a new lock).

use std::fmt;
use std::path::Path;

use serde_json::Value;

use super::custody::{Custody, LinkState};
use super::target::CliCredentialTarget;
use super::target_for;
use crate::storage;
use crate::subscription::Subscription;

/// How fresh the signing material is: which layer of truth it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Custody says the CLI is serving a known subscription; the material is
    /// read straight from the credential the CLI actually opens.
    Live,
    /// The CLI has no usable credential (or reading it failed); the material
    /// comes from the stored-row fallback.
    Row,
    /// The CLI is serving a credential attributable to no known subscription
    /// (a `codex login` done in a terminal, say). The token works, but there
    /// is no subscription id to attribute.
    Diverged,
}

/// Everything one upstream signature needs.
///
/// `Debug` is hand-written: access_token / api_key never reach the output
/// (spec firewall 2; the assertion pattern lives in signing_tests and mirrors
/// gateway trace.rs).
#[derive(Clone)]
pub struct SigningMaterial {
    pub access_token: Option<String>,
    /// Owning account id; same slot as the row's `oauth_account_id`.
    pub account_id: Option<String>,
    /// Row-held API key (zcode and the like). A live credential's key shapes
    /// are already covered by [`CliCredentialTarget::access_token`], so this
    /// field is only non-empty on the row fallback.
    pub api_key: Option<String>,
    /// For ledger attribution; always `None` under `Freshness::Diverged`.
    pub subscription_id: Option<String>,
    pub freshness: Freshness,
}

impl fmt::Debug for SigningMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningMaterial")
            .field("access_token", &Self::redacted(&self.access_token))
            .field("account_id", &self.account_id)
            .field("api_key", &Self::redacted(&self.api_key))
            .field("subscription_id", &self.subscription_id)
            .field("freshness", &self.freshness)
            .finish()
    }
}

impl SigningMaterial {
    /// Presence is fine, the value is not.
    fn redacted(secret: &Option<String>) -> Option<&'static str> {
        secret.as_ref().map(|_| "<redacted>")
    }
}

/// Signing material for one catalog. `None` = not even a stored row with
/// something to sign.
///
/// Blocking IO (the CLI credential file + storage), same synchronous shape as
/// the gateway's `AccountBook::account()`; wrap in `spawn_blocking` on an
/// async hot path.
pub fn signing_material(catalog_id: &str) -> Option<SigningMaterial> {
    let Some(target) = target_for(catalog_id) else {
        // No CLI target: no live file, the stored row is the only truth.
        return row_material(catalog_id, Freshness::Row);
    };
    let custody = Custody::open(target).ok()?;
    match custody.probe() {
        Ok(LinkState::LinkedTo(id)) => live_material(target, custody.live_path(), Some(id))
            .or_else(|| row_material(catalog_id, Freshness::Row)),
        Ok(LinkState::Diverged) => live_material(target, custody.live_path(), None)
            .or_else(|| row_material(catalog_id, Freshness::Row)),
        Ok(LinkState::Missing) => row_material(catalog_id, Freshness::Row),
        Err(error) => {
            tracing::warn!(
                catalog = catalog_id,
                %error,
                "could not read which account the CLI is serving; signing falls back to the stored row"
            );
            row_material(catalog_id, Freshness::Row)
        }
    }
}

/// Extract material from the credential the CLI actually opens: the
/// authoritative root (second store — macOS Codex keychain — first, else the
/// live file itself).
///
/// Kept in step with `Custody`'s internal same-named judgement; reproduced
/// here via the target trait rather than opening a new pub exit on custody.
/// Under a symlink live and snapshot are the same file, so the two reads
/// agree; when the CLI has clobbered the link with a real file and rotated
/// the token, live is the token the CLI is actually sending — that is what
/// live-first means.
fn live_material(
    target: &'static dyn CliCredentialTarget,
    live: &Path,
    subscription_id: Option<String>,
) -> Option<SigningMaterial> {
    let root = target
        .external_root(live)
        .or_else(|| read_live_root(live))?;
    // The token existed when probe attributed it; the file may have been
    // swapped by the CLI since. Return None so the caller degrades to the
    // stored row instead of signing a token that just vanished.
    let access_token = target.access_token(&root)?;
    let identity = target.identity(&root);
    Some(SigningMaterial {
        access_token: Some(access_token),
        // Subject (JWT sub / tokens.account_id / user_id) first, email as the
        // fallback — same slot as the row's oauth_account_id.
        account_id: identity
            .subject()
            .or_else(|| identity.email())
            .map(str::to_string),
        api_key: None,
        freshness: match subscription_id {
            Some(_) => Freshness::Live,
            None => Freshness::Diverged,
        },
        subscription_id,
    })
}

/// The live file root: a whole JSON object; absent, non-JSON, or not an
/// object all count as nothing.
fn read_live_root(live: &Path) -> Option<Value> {
    let raw = std::fs::read(live).ok()?;
    let value: Value = serde_json::from_slice(&raw).ok()?;
    value.is_object().then_some(value)
}

/// Stored-row fallback: the pinned row first, else the catalog's first row
/// (account()'s pre-re-routing semantics — that is how the pinned semantics
/// carry over).
fn row_material(catalog_id: &str, freshness: Freshness) -> Option<SigningMaterial> {
    let rows = storage::list_subscriptions().ok()?;
    let active = storage::get_active_subscription(catalog_id).ok().flatten();
    let row = pinned_row(&rows, catalog_id, active.as_deref())?.clone();
    let subscription_id = row.id.clone();
    Some(SigningMaterial {
        access_token: super::target::secret(row.access_token_encrypted.as_deref()),
        account_id: nonempty(row.oauth_account_id),
        api_key: super::target::secret(row.api_key_encrypted.as_deref()),
        subscription_id: Some(subscription_id),
        freshness,
    })
}

/// The row catalog selection works from: the pinned (active) row when it
/// belongs to this catalog, else the catalog's first row. One rule shared
/// by signing, the gateway account book, and the heal's row pick, so
/// pinning semantics cannot drift between them.
pub fn pinned_row<'a>(
    rows: &'a [Subscription],
    catalog_id: &str,
    active: Option<&str>,
) -> Option<&'a Subscription> {
    if let Some(active) = active
        && let Some(row) = rows
            .iter()
            .find(|row| row.id == active && row.catalog_id == catalog_id)
    {
        return Some(row);
    }
    rows.iter().find(|row| row.catalog_id == catalog_id)
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

#[cfg(test)]
#[path = "signing_tests.rs"]
mod signing_tests;

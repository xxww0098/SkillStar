//! Account switching: make the CLI read another account's credentials.
//!
//! ## The model
//!
//! SkillStar does **not** hold the credential. It holds a *snapshot* of the
//! CLI's own credential file, at
//! `~/.skillstar/accounts/<catalog_id>/<subscription_id>.json` (0600), and the
//! CLI's live path becomes a symlink to whichever snapshot is current.
//!
//! Switching used to mean copying a credential into the live file. That makes
//! a second copy of something the CLI *also* rotates, so the two diverge by
//! construction — and the lease / revision / read-back / rollback machinery
//! that used to fill this module existed only to manage divergence SkillStar
//! had created. With a link there is one copy: the CLI's rotation writes
//! straight through into the snapshot, which is therefore never stale.
//!
//! A snapshot is the whole file, not one account's slice of it, because that
//! is what a symlink can stand in for. See `docs/decisions.md` for the
//! consequences of that choice.
//!
//! ## What custody covers
//!
//! | catalog_id | CLI       | live path                                  |
//! |------------|-----------|--------------------------------------------|
//! | `codex`    | Codex CLI | `$CODEX_HOME/auth.json` + macOS keychain    |
//! | `xai`      | Grok CLI  | `$GROK_HOME/auth.json`                      |
//! | `opencode` | OpenCode  | `$XDG_DATA_HOME/opencode/auth.json`         |
//!
//! Support is derived from that registry ([`target_for`]), not from a
//! hand-maintained whitelist, so the UI affordance cannot outlive the
//! adapter. Cursor and Antigravity are IDEs whose credentials live in
//! `state.vscdb` under an entirely different mechanism; they stay
//! unsupported on purpose.
//!
//! Domain glue lives in `skillstar-app` because it bridges `skillstar-usage`
//! (subscriptions, crypto, storage) and `skillstar-models` (tool_sync path
//! resolution and rolling backups) without either depending on the other.

mod custody;
mod error;
#[cfg(target_os = "macos")]
mod keychain;
mod target;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageError, UsageResult, storage};

use custody::{Activated, Custody, CustodyLease, LinkState};
use target::CliCredentialTarget;

pub use custody::LinkMode;

/// Outcome of a single account-switch attempt. Always serialised to the DTO
/// so the UI can show success / failure / "not a CLI provider" distinctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcome {
    /// CLI tool id that was targeted (`"codex"` / `"opencode"` / `"grok"`).
    pub tool_id: String,
    /// Resolved credential file that was (or would be) bound.
    pub config_path: String,
    /// Path to the rolling backup taken before the live file was replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    /// `true` on macOS when the Codex keychain entry was updated too.
    #[serde(default)]
    pub keychain_updated: bool,
    /// How the live path ended up bound: [`LinkMode::Symlink`] pass-through, or
    /// a degraded [`LinkMode::Copy`]. `None` when nothing was bound (a failed
    /// or unsupported switch).
    ///
    /// The degradation used to exist only in a `warn!` log, which meant a
    /// Windows user was told "switched" and never told that the CLI's own token
    /// rotation would no longer flow back into SkillStar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_mode: Option<LinkMode>,
    /// `true` when the switch fully succeeded.
    pub success: bool,
    /// Human-readable error when `success` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SwitchOutcome {
    fn ok(tool_id: &str, config_path: &std::path::Path, activated: &Activated) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            config_path: config_path.to_string_lossy().to_string(),
            backup_path: activated
                .backup
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            keychain_updated: activated.external_updated,
            link_mode: Some(activated.mode),
            success: true,
            error: None,
        }
    }

    fn fail(tool_id: &str, config_path: &std::path::Path, error: impl Into<String>) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            config_path: config_path.to_string_lossy().to_string(),
            backup_path: None,
            keychain_updated: false,
            link_mode: None,
            success: false,
            error: Some(error.into()),
        }
    }

    /// Not a CLI-backed catalog: no error, nothing was attempted.
    fn unsupported(catalog_id: &str) -> Self {
        Self {
            tool_id: catalog_id.to_string(),
            config_path: String::new(),
            backup_path: None,
            keychain_updated: false,
            link_mode: None,
            success: false,
            error: None,
        }
    }
}

/// Which account a CLI is *actually* serving, as read back from disk.
///
/// The pin in `active_per_catalog.json` is a cache of this; when the two
/// disagree the file wins, because the file is what the CLI opens. Surfaced to
/// the UI as three distinct states on purpose — folding them into one boolean
/// is precisely how a card came to claim "current" while the CLI was serving
/// somebody else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CliAccountState {
    /// The CLI is serving this subscription — through a symlink, or through a
    /// real file whose access token is byte-identical to its snapshot.
    #[serde(rename_all = "camelCase")]
    LinkedTo { subscription_id: String },
    /// The CLI is serving a credential that belongs to no known subscription
    /// (a `codex login` done in a terminal, say). Not "inactive": someone
    /// *is* logged in, just nobody SkillStar can name.
    Diverged,
    /// The CLI has no usable credential at all.
    Missing,
}

impl From<LinkState> for CliAccountState {
    fn from(state: LinkState) -> Self {
        match state {
            LinkState::LinkedTo(subscription_id) => Self::LinkedTo { subscription_id },
            LinkState::Diverged => Self::Diverged,
            LinkState::Missing => Self::Missing,
        }
    }
}

// ── registry ─────────────────────────────────────────────────────────────

/// Every catalog whose credentials live in a local CLI file.
///
/// This replaces the old hardcoded `matches!(id, …)` whitelist. The predicate,
/// the capability and the reconcile sweep are now the same fact, so the UI can
/// never offer a switch that has no implementation behind it — nor report a
/// live state for a catalog it cannot read.
const TARGETS: &[&'static dyn CliCredentialTarget] = &[
    &target::codex::CodexTarget,
    &target::grok::GrokTarget,
    &target::opencode::OpenCodeTarget,
];

/// The custody adapter for a catalog, if it has a local credential file.
fn target_for(catalog_id: &str) -> Option<&'static dyn CliCredentialTarget> {
    TARGETS
        .iter()
        .copied()
        .find(|target| target.catalog_id() == catalog_id)
}

/// Whether this catalog's account can be switched in a real CLI. Surfaced to
/// the UI so it hides the affordance for IDE-backed providers.
pub fn supports_cli_switch(catalog_id: &str) -> bool {
    target_for(catalog_id).is_some()
}

/// Result returned by the activation facade.
#[derive(Debug, Clone)]
pub struct ActivationResult {
    pub subscription: Subscription,
    pub switch_result: SwitchOutcome,
}

/// Held while a Usage refresh may rotate credentials. For Grok this is the
/// official `auth.json.lock` the CLI takes too; elsewhere it is a private lock
/// that at least serialises SkillStar against its own other processes.
///
/// Symlinks remove stale copies, not the fact that a refresh token is
/// single-use and whoever spends it first revokes the other side — so the
/// lock stays.
pub struct CliRefreshLease {
    target: Option<&'static dyn CliCredentialTarget>,
    _lease: Option<CustodyLease>,
}

fn other(error: String) -> UsageError {
    UsageError::Other(error)
}

/// Run a blocking custody operation off the async runtime.
async fn blocking<T, F>(work: F) -> UsageResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> UsageResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| other(format!("CLI 凭证任务失败：{error}")))?
}

// ── the three operations ─────────────────────────────────────────────────

/// Pin a subscription and point its CLI at it, under the same per-catalog
/// serialization domain Usage refreshes use.
///
/// The pin is written **only after** the credential is verifiably in place, so
/// a failed switch leaves both the previous pin and the previous live file
/// intact — the "switch not applied" contract, now true for every catalog and
/// not just Grok.
pub async fn activate_subscription(subscription_id: &str) -> UsageResult<ActivationResult> {
    let catalog_id = storage::get_subscription(subscription_id)?.catalog_id;
    let subscription_id = subscription_id.to_string();
    skillstar_usage::refresh_guard::with_catalog_lock(&catalog_id.clone(), || async move {
        let Some(target) = target_for(&catalog_id) else {
            // Non-CLI catalogs still get a pin; it is a UI preference, not a
            // claim about any file on disk.
            let subscription = storage::get_subscription(&subscription_id)?;
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            return Ok(ActivationResult {
                switch_result: SwitchOutcome::unsupported(&catalog_id),
                subscription,
            });
        };
        blocking(move || activate_blocking(target, &subscription_id)).await
    })
    .await?
}

fn activate_blocking(
    target: &'static dyn CliCredentialTarget,
    subscription_id: &str,
) -> UsageResult<ActivationResult> {
    let custody = Custody::open(target)?;
    let _lease = custody.lock()?;
    let mut subscription = storage::get_subscription(subscription_id)?;

    match custody.activate(&mut subscription) {
        Ok(activated) => {
            // Also carried to the UI on `SwitchOutcome::link_mode`; logged
            // here so a support transcript records how it was bound.
            tracing::info!(
                tool = custody.tool_id(),
                mode = activated.mode.as_str(),
                keychain = activated.external_updated,
                "CLI account activated"
            );
            let switch_result =
                SwitchOutcome::ok(custody.tool_id(), custody.live_path(), &activated);
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            // The installed credential is the truth; fold it back into the row
            // so the card and the CLI cannot disagree.
            if let Ok(saved) = storage::patch_oauth_credentials(&subscription) {
                subscription = saved;
            }
            Ok(ActivationResult {
                subscription,
                switch_result,
            })
        }
        Err(error) => {
            // `rolled_back` is the staged half of the error made visible: a
            // support transcript has to say whether the CLI was left holding
            // the previous credential or a restored one.
            tracing::warn!(
                tool = custody.tool_id(),
                rolled_back = error.stage.needs_rollback(),
                %error,
                "CLI account activation failed"
            );
            Ok(ActivationResult {
                switch_result: SwitchOutcome::fail(
                    custody.tool_id(),
                    custody.live_path(),
                    error.to_string(),
                ),
                // Re-read: capture may legitimately have absorbed a CLI rotation
                // even though the switch itself failed.
                subscription: storage::get_subscription(subscription_id)?,
            })
        }
    }
}

/// Re-bind the currently active account without changing the pin. Backs the
/// "重新同步到 CLI" button.
pub async fn resync_active_subscription(catalog_id: &str) -> UsageResult<SwitchOutcome> {
    let catalog_id = catalog_id.to_string();
    skillstar_usage::refresh_guard::with_catalog_lock(&catalog_id.clone(), || async move {
        let Some(target) = target_for(&catalog_id) else {
            return Ok(SwitchOutcome::unsupported(&catalog_id));
        };
        let active = storage::get_active_subscription(&catalog_id)?
            .ok_or_else(|| other(format!("catalog {catalog_id} 还没有设置活跃账号")))?;
        blocking(move || activate_blocking(target, &active).map(|result| result.switch_result))
            .await
    })
    .await?
}

/// Which account each CLI is actually serving right now, keyed by catalog id.
///
/// This — not the pin — is what the "current" badge must be drawn from. The pin
/// records what the user last asked for; a `codex login` in a terminal, a CLI
/// that rotated its own token, or a switch that failed after the pin was read
/// all move the file without moving the pin.
///
/// A catalog is absent from the map when it has no CLI adapter (its pin *is*
/// the truth — nothing on disk to disagree with) or when reading it failed; in
/// both cases the caller falls back to the pin, which is exactly today's
/// behaviour and never a worse claim than the one being replaced.
pub async fn reconcile_cli_accounts() -> UsageResult<HashMap<String, CliAccountState>> {
    let mut states = HashMap::with_capacity(TARGETS.len());
    for target in TARGETS {
        let catalog_id = target.catalog_id();
        match reconcile_cli_account(catalog_id).await {
            Ok(Some(state)) => {
                states.insert(catalog_id.to_string(), state);
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(
                catalog = catalog_id,
                %error,
                "could not read which account the CLI is serving; falling back to the pin"
            ),
        }
    }
    Ok(states)
}

/// [`reconcile_cli_accounts`] for one catalog. `None` when the catalog has no
/// CLI behind it.
///
/// Runs in the catalog's serialization domain so it cannot observe an
/// activation half-applied, and repairs as it reads: a token the CLI rotated
/// gets absorbed, a symlink the CLI clobbered gets rebuilt.
pub async fn reconcile_cli_account(catalog_id: &str) -> UsageResult<Option<CliAccountState>> {
    let catalog_id = catalog_id.to_string();
    skillstar_usage::refresh_guard::with_catalog_lock(&catalog_id.clone(), || async move {
        let Some(target) = target_for(&catalog_id) else {
            return Ok(None);
        };
        blocking(move || {
            let custody = Custody::open(target)?;
            // A CLI that was never installed has nothing to reconcile, and
            // taking the lock would *create* its home directory and lock file
            // just to discover that.
            if !custody.has_live_credential() {
                return Ok(Some(CliAccountState::Missing));
            }
            let _lease = custody.lock()?;
            Ok(Some(CliAccountState::from(custody.reconcile()?)))
        })
        .await
    })
    .await?
}

/// Drop a deleted account's snapshot.
///
/// If the CLI is currently being served by it, a real file is left behind
/// first: a dangling symlink is not "logged out", it is "cannot log in".
pub fn forget_subscription_session(catalog_id: &str, subscription_id: &str) -> UsageResult<()> {
    let Some(target) = target_for(catalog_id) else {
        return Ok(());
    };
    let custody = Custody::open(target)?;
    let _lease = custody.lock()?;
    custody.forget(subscription_id).map_err(UsageError::from)
}

// ── the refresh window ───────────────────────────────────────────────────

/// Take the CLI's lock for the duration of a Usage refresh.
///
/// Previously Grok-only, which is why SkillStar's background refresh could
/// silently revoke the Codex CLI's own refresh-token generation.
pub async fn acquire_cli_refresh_lease(catalog_id: &str) -> UsageResult<CliRefreshLease> {
    let Some(target) = target_for(catalog_id) else {
        return Ok(CliRefreshLease {
            target: None,
            _lease: None,
        });
    };
    blocking(move || {
        let custody = Custody::open(target)?;
        let lease = custody.lock()?;
        Ok(CliRefreshLease {
            target: Some(target),
            _lease: Some(lease),
        })
    })
    .await
}

/// Before spending a refresh token, adopt any newer generation the CLI already
/// rotated into the snapshot.
///
/// A symlink means SkillStar and the CLI write the same file, so there is no
/// stale copy — but refresh tokens are still single-use, and spending one the
/// CLI already replaced would 401 and look like a dead login.
pub fn adopt_active_cli_session_before_refresh(
    subscription: &mut Subscription,
    lease: &CliRefreshLease,
) -> UsageResult<()> {
    let Some(target) = lease.target else {
        return Ok(());
    };
    if storage::get_active_subscription(&subscription.catalog_id)?.as_deref()
        != Some(subscription.id.as_str())
    {
        return Ok(());
    }
    let custody = Custody::open(target)?;
    custody.reconcile()?;
    *subscription = storage::get_subscription(&subscription.id)?;
    Ok(())
}

/// After SkillStar's own refresh produced a newer generation, write it into
/// the snapshot — which *is* the CLI's live file.
pub fn sync_refreshed_active_subscription(
    subscription: &mut Subscription,
    lease: &CliRefreshLease,
) -> UsageResult<Option<SwitchOutcome>> {
    let Some(target) = lease.target else {
        return Ok(None);
    };
    if storage::get_active_subscription(&subscription.catalog_id)?.as_deref()
        != Some(subscription.id.as_str())
    {
        return Ok(None);
    }
    let custody = Custody::open(target)?;
    Ok(Some(match custody.project_refreshed(subscription) {
        Ok(activated) => SwitchOutcome::ok(custody.tool_id(), custody.live_path(), &activated),
        Err(error) => {
            SwitchOutcome::fail(custody.tool_id(), custody.live_path(), error.to_string())
        }
    }))
}

#[cfg(test)]
#[path = "usage_switch/custody_tests.rs"]
mod custody_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// The predicate is the registry, so an entry can never be advertised
    /// without an adapter behind it (the OpenCode chip used to be exactly
    /// that: permanently visible, permanently failing).
    #[test]
    fn supports_cli_switch_follows_the_target_registry() {
        for catalog in ["codex", "opencode", "xai"] {
            assert!(supports_cli_switch(catalog), "{catalog}");
            assert_eq!(target_for(catalog).unwrap().catalog_id(), catalog);
        }
        // IDEs keep their credentials in state.vscdb — a different mechanism,
        // deliberately out of scope. `anthropic` is out of scope for a second
        // reason: the fetcher only ever *reads* Claude Code's own login, so
        // there is no snapshot for custody to take.
        for catalog in [
            "cursor",
            "antigravity",
            "deepseek",
            "glm",
            "stepfun",
            "anthropic",
        ] {
            assert!(!supports_cli_switch(catalog), "{catalog}");
        }
    }

    #[test]
    fn unsupported_outcome_carries_no_error() {
        let outcome = SwitchOutcome::unsupported("cursor");
        assert!(!outcome.success);
        assert!(outcome.error.is_none(), "unsupported is not a failure");
        assert_eq!(outcome.tool_id, "cursor");
    }

    #[test]
    fn link_mode_is_named_so_a_windows_copy_is_never_silent() {
        assert_eq!(custody::LinkMode::Symlink.as_str(), "symlink");
        assert_eq!(custody::LinkMode::Copy.as_str(), "copy");
    }
}

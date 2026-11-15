//! The facade's orchestration: tokens through the OAuth module, quota through
//! the usage module, and the one place that decides what a failed read means
//! for a row.
//!
//! Two rules shape this file. A refresh writes narrowly - tokens, provider
//! state, quota, the reauth flag - so a rename the user made while the network
//! was busy survives. A switch verifies against the client's own disk before it
//! moves anything, so a client that refuses or ignores the write stays on the
//! account it was already serving.

use crate::oauth::{RefreshOutcome, TokenSet};
use crate::usage::{Credentials, UsageOutcome};

use crate::error::{AccountError, AccountResult};
use crate::record::Account;
use crate::reconcile::{self, AgentLive};
use crate::store::{self, IndexEntry};
use crate::switch::{self, ClientSnapshot, SwitchRequest};
use crate::token_import::ImportedToken;
use crate::{AccountView, ActivateOutcome, Agent};

/// The OAuth refresh the facade uses, as a function pointer so tests can stand
/// in for a provider that is rejecting or unavailable.
type OAuthRefresh = fn(
    &str,
    &TokenSet,
    Option<&serde_json::Value>,
) -> Result<RefreshOutcome, crate::oauth::OAuthError>;

/// The quota read the facade uses, for the same reason.
type UsageRead = fn(&str, &Credentials) -> Result<UsageOutcome, crate::usage::UsageError>;

/// Now, in unix seconds.
pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default()
}

/// Every stored account, oldest first, with the client's truth already applied.
pub fn list() -> AccountResult<Vec<AccountView>> {
    let entries = store::index()?;
    let lives = reconcile::live_all()?;
    Ok(entries
        .iter()
        .map(|entry| view_from_entry(entry, &lives))
        .collect())
}

/// Refresh one account's tokens and quota, then return its fresh view.
pub fn refresh(id: &str) -> AccountResult<AccountView> {
    refresh_with(id, crate::oauth::refresh, crate::usage::read)
}

/// Refresh with injectable provider calls.
pub(crate) fn refresh_with(
    id: &str,
    refresh_tokens: OAuthRefresh,
    read_usage: UsageRead,
) -> AccountResult<AccountView> {
    // One refresh per agent at a time: two rotations of the same refresh token
    // would leave one of them storing a token the provider already replaced.
    // The record is read again under that lock, so the credential refreshed is
    // the one on disk now and not one another window already rotated.
    let agent = store::get(id)?.agent;
    let _refresh_guard = store::AgentRefreshLock::acquire(&agent)?;
    let stored = store::get(id)?;
    let mut tokens = stored.tokens()?;
    let mut state = stored.provider_state()?;

    if tokens.needs_refresh(now_secs(), crate::oauth::refresh_skew_seconds(&agent)) {
        match refresh_tokens(&agent, &tokens, state.as_ref()) {
            Ok(outcome) => {
                tokens = outcome.tokens;
                if outcome.provider_state.is_some() {
                    state = outcome.provider_state;
                }
            }
            Err(error) => {
                let classified = AccountError::from(error);
                if matches!(classified, AccountError::AuthRequired(_)) {
                    mark_requires_reauth(id)?;
                }
                return Err(classified);
            }
        }
    }

    let credentials = Credentials {
        tokens: tokens.clone(),
        provider_state: state.clone(),
    };
    let mut usage = stored.usage.clone();
    let mut requires_reauth = stored.requires_reauth;
    match read_usage(&agent, &credentials) {
        Ok(outcome) => {
            tokens = outcome.tokens;
            if outcome.provider_state.is_some() {
                state = outcome.provider_state;
            }
            usage = Some(outcome.snapshot);
            requires_reauth = false;
        }
        Err(error) => {
            let classified = AccountError::from(error);
            if matches!(classified, AccountError::AuthRequired(_)) {
                mark_requires_reauth(id)?;
                return Err(classified);
            }
            // Transient, unreadable or not wired up yet: keep the last good
            // snapshot and say why it is stale.
            tracing::warn!(
                target: "accounts",
                account = %id,
                error = %classified,
                "quota read failed; keeping the last snapshot"
            );
            if let Some(previous) = usage.as_mut() {
                previous.error = Some(classified.to_string());
            }
        }
    }

    let updated = store::update(id, |account| {
        account.set_tokens(&tokens)?;
        account.set_provider_state(state.as_ref())?;
        account.usage = usage;
        account.requires_reauth = requires_reauth;
        Ok(())
    })?;
    Ok(view_for(&updated))
}

/// Make the account's client serve it.
pub fn activate(id: &str) -> AccountResult<ActivateOutcome> {
    activate_with(id, crate::oauth::refresh, crate::usage::read)
}

/// Activate with injectable provider calls.
pub(crate) fn activate_with(
    id: &str,
    refresh_tokens: OAuthRefresh,
    read_usage: UsageRead,
) -> AccountResult<ActivateOutcome> {
    // 1. The credential has to work before the client is touched at all.
    refresh_with(id, refresh_tokens, read_usage)?;
    let account = store::get(id)?;
    let agent = account.agent.clone();
    let previous = live_account_id(&agent);
    let tokens = account.tokens()?;
    let state = account.provider_state()?;

    // 2. Whatever the client holds now, so any failure below can put it back.
    let snapshot = switch::snapshot(&agent)?;
    let request = SwitchRequest {
        agent: &agent,
        tokens: &tokens,
        provider_state: state.as_ref(),
        display_name: &account.display_name,
        account_id: account.account_id.as_deref(),
        email: account.email.as_deref(),
    };
    let outcome = match switch::write(&request) {
        Ok(outcome) => outcome,
        Err(error) => return Err(roll_back(&snapshot, error)),
    };

    // 3. Read the client back: the disk decides, not the writer.
    match switch::live(&agent) {
        Ok(live) if serves(&live, &tokens, &account) => {}
        Ok(live) => {
            let error = AccountError::Client(format!(
                "{} did not start serving this account (the client shows {:?})",
                outcome.target, live.identity
            ));
            return Err(roll_back(&snapshot, error));
        }
        Err(error) => return Err(roll_back(&snapshot, error)),
    }

    // 4. Only a verified switch moves the row.
    let updated = store::update(id, |account| {
        account.last_used_at = Some(now_secs());
        Ok(())
    })?;
    Ok(ActivateOutcome {
        account: view_for_with(&updated, Some(updated.id.as_str())),
        previous,
        target: outcome.target,
    })
}

/// Forget one account (the client is left as it is).
pub fn remove(id: &str) -> AccountResult<()> {
    store::remove(id)
}

/// Add an account from a pasted token or credential JSON.
pub fn add_from_token(agent: &str, payload: &str) -> AccountResult<AccountView> {
    add_from_token_with(agent, payload, crate::usage::read)
}

/// Add a pasted credential with an injectable quota read.
pub(crate) fn add_from_token_with(
    agent: &str,
    payload: &str,
    read_usage: UsageRead,
) -> AccountResult<AccountView> {
    let agent = Agent::from_id(agent)?;
    let imported = crate::token_import::parse(agent.id(), payload)?;
    store_new(agent.id(), imported, read_usage)
}

/// Add an account from the machine's own client credential.
pub fn add_from_local(agent: &str) -> AccountResult<AccountView> {
    add_from_local_with(agent, crate::usage::read)
}

/// Import the machine's credential with an injectable quota read.
pub(crate) fn add_from_local_with(
    agent: &str,
    read_usage: UsageRead,
) -> AccountResult<AccountView> {
    let agent = Agent::from_id(agent)?;
    let local = switch::read(agent.id())?;
    store_new(
        agent.id(),
        ImportedToken {
            tokens: local.tokens,
            identity: local.identity,
            provider_state: local.provider_state,
        },
        read_usage,
    )
}

/// Finish a pending login and store the account; an existing row for the same
/// agent + identity is replaced in place.
pub fn finish_login(login_id: &str) -> AccountResult<AccountView> {
    let outcome = crate::oauth::complete(login_id)?;
    let agent = Agent::from_id(&outcome.agent)?;
    let account = Account::from_login(
        agent.id(),
        &outcome.identity,
        &outcome.tokens,
        outcome.provider_state.as_ref(),
        now_secs(),
    )?;
    let stored = store::upsert_replacing_identity(account)?;
    Ok(view_for(&stored))
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn store_new(
    agent: &str,
    imported: ImportedToken,
    read_usage: UsageRead,
) -> AccountResult<AccountView> {
    let mut account = Account::from_login(
        agent,
        &imported.identity,
        &imported.tokens,
        imported.provider_state.as_ref(),
        now_secs(),
    )?;
    verify_usage(&mut account, read_usage)?;
    let stored = store::upsert_replacing_identity(account)?;
    Ok(view_for(&stored))
}

/// Read the quota once, so a token the provider already rejects is not stored
/// at all. A transient failure still stores the credential: the token may be
/// fine and the endpoint busy.
fn verify_usage(account: &mut Account, read_usage: UsageRead) -> AccountResult<()> {
    let credentials = Credentials {
        tokens: account.tokens()?,
        provider_state: account.provider_state()?,
    };
    match read_usage(&account.agent, &credentials).map_err(AccountError::from) {
        Ok(outcome) => {
            account.set_tokens(&outcome.tokens)?;
            if outcome.provider_state.is_some() {
                account.set_provider_state(outcome.provider_state.as_ref())?;
            }
            account.usage = Some(outcome.snapshot);
            account.requires_reauth = false;
            Ok(())
        }
        Err(classified @ AccountError::AuthRequired(_)) => Err(classified),
        Err(tolerated) => {
            tracing::warn!(
                target: "accounts",
                agent = %account.agent,
                error = %tolerated,
                "quota read failed; storing the credential without a snapshot"
            );
            account.requires_reauth = false;
            Ok(())
        }
    }
}

fn mark_requires_reauth(id: &str) -> AccountResult<()> {
    store::update(id, |account| {
        account.requires_reauth = true;
        Ok(())
    })
    .map(|_| ())
}

/// Whether a client read-back is the account that was just written: the token
/// fingerprint first (a switch wrote that exact access token), then identity.
fn serves(live: &switch::LiveCredential, tokens: &TokenSet, account: &Account) -> bool {
    if live.access_fingerprint.as_deref() == Some(crate::identity::fingerprint(&tokens.access_token).as_str())
    {
        return true;
    }
    let identity = live.identity.as_deref();
    crate::identity::same_identity(identity, account.account_id.as_deref())
        || crate::identity::same_identity(identity, account.email.as_deref())
}

/// Put the client's previous credential back and keep the original failure as
/// the one the user sees (an addendum is appended when the rollback also fails).
fn roll_back(snapshot: &ClientSnapshot, cause: AccountError) -> AccountError {
    match switch::restore(snapshot) {
        Ok(()) => cause,
        Err(rollback) => {
            tracing::error!(
                target: "accounts",
                cause = %cause,
                error = %rollback,
                "putting the client's previous credential back failed"
            );
            AccountError::Client(format!("{cause}; the previous credential could not be restored either: {rollback}"))
        }
    }
}

/// The account the agent's client is serving, read from the client itself.
fn live_account_id(agent: &str) -> Option<String> {
    reconcile::live_all()
        .ok()?
        .into_iter()
        .find(|live| live.agent == agent)?
        .account_id
}

fn view_for(account: &Account) -> AccountView {
    let active = live_account_id(&account.agent);
    view_for_with(account, active.as_deref())
}

fn view_for_with(account: &Account, active: Option<&str>) -> AccountView {
    AccountView {
        id: account.id.clone(),
        agent: account.agent.clone(),
        display_name: account.display_name.clone(),
        email: account.email.clone(),
        plan: account
            .plan
            .clone()
            .or_else(|| account.usage.as_ref().and_then(|usage| usage.plan.clone())),
        created_at: account.created_at,
        last_used_at: account.last_used_at,
        requires_reauth: account.requires_reauth,
        is_active: active == Some(account.id.as_str()),
        usage: account.usage.clone(),
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;

fn view_from_entry(entry: &IndexEntry, lives: &[AgentLive]) -> AccountView {
    let active = lives
        .iter()
        .find(|live| live.agent == entry.agent)
        .and_then(|live| live.account_id.as_deref());
    AccountView {
        id: entry.id.clone(),
        agent: entry.agent.clone(),
        display_name: entry.display_name.clone(),
        email: entry.email.clone(),
        plan: entry
            .plan
            .clone()
            .or_else(|| entry.usage.as_ref().and_then(|usage| usage.plan.clone())),
        created_at: entry.created_at,
        last_used_at: entry.last_used_at,
        requires_reauth: entry.requires_reauth,
        is_active: active == Some(entry.id.as_str()),
        usage: entry.usage.clone(),
    }
}

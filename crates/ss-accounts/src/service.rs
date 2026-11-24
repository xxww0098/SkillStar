//! The facade's orchestration: tokens through the OAuth module, quota through
//! the usage module, and the one place that decides what a failed read means
//! for a row.
//!
//! Two rules shape this file. A refresh writes narrowly - tokens, provider
//! state, quota, the reauth flag - so a rename the user made while the network
//! was busy survives. A switch verifies against the client's own disk before it
//! moves anything, so a client that refuses or ignores the write stays on the
//! account it was already serving. A switch never touches the client process:
//! only the stored credential is rewritten, and restarting the client to pick it
//! up is the user's call (D-121).

use crate::oauth::{RefreshOutcome, TokenSet};
use crate::usage::{Credentials, UsageError, UsageOutcome};

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

/// Supported stored accounts, oldest first, with the client's truth already applied.
///
/// Reading the list is also the moment a retired client's rows go away: this
/// build cannot log in, read quota for, or switch those agents any more, so
/// their records are deleted instead of lingering as unreadable leftovers
/// (D-119).
pub fn list() -> AccountResult<Vec<AccountView>> {
    let entries = store::index_supported()?;
    let lives = reconcile::live_all_with(&entries);
    Ok(entries
        .iter()
        .filter(|entry| Agent::from_id(&entry.agent).is_ok())
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
    let updated = refresh_record_with(id, refresh_tokens, read_usage)?;
    Ok(view_for(&updated))
}

fn refresh_record_with(
    id: &str,
    refresh_tokens: OAuthRefresh,
    read_usage: UsageRead,
) -> AccountResult<Account> {
    let mut stored = store::get(id)?;
    let agent = stored.agent.clone();
    Agent::from_id(&agent)?;
    let disk_tokens = stored.tokens()?;
    let mut tokens = disk_tokens.clone();
    let disk_state = stored.provider_state()?;
    let mut state = disk_state.clone();
    heal_cursor_tokens_from_local(&stored, &mut tokens);
    let mut tokens_changed = tokens != disk_tokens;
    let mut state_changed = false;

    // Align pre-refresh skew with usage::fetch::tokens_for_read (300s) for
    // OAuth-refreshable agents so rotation stays serialized under AgentRefreshLock
    // and never leaks into the unlocked quota-HTTP phase.
    let refresh_skew = if agent == Agent::DevinDesktop.id() {
        crate::oauth::refresh_skew_seconds(&agent)
    } else {
        crate::oauth::refresh_skew_seconds(&agent).max(300)
    };

    if tokens.needs_refresh(now_secs(), refresh_skew) {
        // Hold per-agent lock ONLY around token rotation + persistence so slow
        // quota HTTP calls never block activate() or other accounts' refreshes.
        let _refresh_guard = store::AgentRefreshLock::acquire(&agent)?;
        stored = store::get(id)?;
        let locked_disk_tokens = stored.tokens()?;
        tokens = locked_disk_tokens.clone();
        let locked_disk_state = stored.provider_state()?;
        state = locked_disk_state.clone();
        heal_cursor_tokens_from_local(&stored, &mut tokens);
        tokens_changed = tokens != locked_disk_tokens;
        state_changed = false;
        if tokens.needs_refresh(now_secs(), refresh_skew) {
            match refresh_tokens(&agent, &tokens, state.as_ref()) {
                Ok(outcome) => {
                    tokens = outcome.tokens;
                    if outcome.provider_state.is_some() {
                        state = outcome.provider_state;
                    }
                    stored = store::update(id, |account| {
                        account.set_tokens(&tokens)?;
                        account.set_provider_state(state.as_ref())?;
                        Ok(())
                    })?;
                    tokens_changed = false;
                    state_changed = false;
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
    }

    let local_identity = if agent == Agent::Grok.id() {
        switch::live(&agent)
            .ok()
            .filter(|live| serves(live, &tokens, &stored))
            .and_then(|live| live.display_name)
            .map(|name| crate::oauth::Identity {
                display_name: Some(name),
                ..crate::oauth::Identity::default()
            })
    } else {
        None
    };

    let credentials = Credentials {
        tokens: tokens.clone(),
        account_id: stored.account_id.clone(),
        provider_state: state.clone(),
        needs_identity: needs_profile_identity(&stored),
    };
    let mut usage = clean_usage(&agent, stored.usage.clone());
    let mut requires_reauth = stored.requires_reauth;
    let mut discovered_identity = None;
    match read_usage(&agent, &credentials) {
        Ok(outcome) => {
            if outcome.tokens != credentials.tokens {
                tokens = outcome.tokens;
                tokens_changed = true;
            }
            if outcome.provider_state.is_some() && outcome.provider_state != credentials.provider_state {
                state = outcome.provider_state;
                state_changed = true;
            }
            discovered_identity = outcome.identity;
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
                agent = %agent,
                account = %id,
                error = %classified,
                "quota read failed; keeping the last snapshot"
            );
            if let Some(previous) = usage.as_mut() {
                previous.error = Some(classified.to_string());
            } else {
                usage = Some(crate::usage::UsageSnapshot {
                    fetched_at: now_secs(),
                    error: Some(classified.to_string()),
                    ..Default::default()
                });
            }
        }
    }

    let token_identity = identity_from_tokens(&tokens);
    store::update(id, |account| {
        if tokens_changed {
            account.set_tokens(&tokens)?;
        }
        if state_changed {
            account.set_provider_state(state.as_ref())?;
        }
        if let Some(identity) = discovered_identity.as_ref() {
            apply_discovered_identity(account, identity);
        }
        apply_discovered_identity(account, &token_identity);
        if let Some(identity) = local_identity.as_ref() {
            apply_discovered_identity(account, identity);
        }
        account.usage = usage;
        account.requires_reauth = requires_reauth;
        Ok(())
    })
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
    let refreshed = refresh_record_with(id, refresh_tokens, read_usage)?;
    let agent = refreshed.agent.clone();
    let _switch_guard = store::AgentRefreshLock::acquire(&agent)?;
    let account = store::get(id)?;
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
    let live_display_name = match switch::live(&agent) {
        Ok(live) if serves(&live, &tokens, &account) => live.display_name,
        Ok(live) => {
            let error = AccountError::Client(format!(
                "{} did not start serving this account (the client shows {:?})",
                outcome.target, live.identity
            ));
            return Err(roll_back(&snapshot, error));
        }
        Err(error) => return Err(roll_back(&snapshot, error)),
    };

    // 4. Only a verified switch moves the row.
    let updated = store::update(id, |account| {
        account.last_used_at = Some(now_secs());
        Ok(())
    })?;
    // 5. 客户端进程不动：切号只写凭据。客户端要重启才读到新会话时，由用户自己
    //    决定什么时候重启（D-121）。
    Ok(ActivateOutcome {
        account: view_for_with(&updated, Some(updated.id.as_str()), live_display_name.as_deref()),
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
    let mut account = Account::from_login(
        agent.id(),
        &outcome.identity,
        &outcome.tokens,
        outcome.provider_state.as_ref(),
        now_secs(),
    )?;
    let _ = verify_usage(&mut account, crate::usage::read);
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
        account_id: account.account_id.clone(),
        provider_state: account.provider_state()?,
        needs_identity: needs_profile_identity(account),
    };
    let token_identity = identity_from_tokens(&credentials.tokens);
    match read_usage(&account.agent, &credentials) {
        Ok(outcome) => {
            account.set_tokens(&outcome.tokens)?;
            if outcome.provider_state.is_some() {
                account.set_provider_state(outcome.provider_state.as_ref())?;
            }
            if let Some(identity) = outcome.identity.as_ref() {
                apply_discovered_identity(account, identity);
            }
            apply_discovered_identity(account, &identity_from_tokens(&outcome.tokens));
            account.usage = Some(outcome.snapshot);
            account.requires_reauth = false;
            Ok(())
        }
        Err(UsageError::AuthRequired(message)) => Err(AccountError::AuthRequired(message)),
        Err(tolerated) => {
            // An unreadable answer is durable, so the reason is kept on the row
            // and the card can say why the quota is missing. Transient and
            // unwired agents stay quiet: the next refresh retries them.
            let diagnostic = matches!(tolerated, UsageError::Unreadable(_));
            let tolerated = AccountError::from(tolerated);
            apply_discovered_identity(account, &token_identity);
            tracing::warn!(
                target: "accounts",
                agent = %account.agent,
                error = %tolerated,
                "quota read failed; storing the credential without a snapshot"
            );
            if diagnostic {
                account.usage = Some(crate::usage::UsageSnapshot {
                    fetched_at: now_secs(),
                    error: Some(tolerated.to_string()),
                    ..Default::default()
                });
            }
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
fn live_for(agent: &str) -> Option<AgentLive> {
    reconcile::live_for_agent(agent)
}

fn live_account_id(agent: &str) -> Option<String> {
    live_for(agent).and_then(|live| live.account_id)
}

fn heal_cursor_tokens_from_local(stored: &Account, tokens: &mut TokenSet) {
    if stored.agent != Agent::Cursor.id() {
        return;
    }
    if tokens
        .refresh_token
        .as_deref()
        .is_some_and(switch::cursor::is_safe_storage_ciphertext)
    {
        tokens.refresh_token = None;
    }
    let needs_heal = switch::cursor::is_safe_storage_ciphertext(&tokens.access_token)
        || (tokens.refresh_token.is_none()
            && tokens.needs_refresh(now_secs(), crate::oauth::refresh_skew_seconds(&stored.agent)));
    if !needs_heal {
        return;
    }
    let Ok(local) = switch::read(Agent::Cursor.id()) else {
        return;
    };
    let same_email = crate::identity::same_identity(
        local.identity.email.as_deref(),
        stored.email.as_deref(),
    );
    let same_account = crate::identity::same_identity(
        local.identity.account_id.as_deref(),
        stored.account_id.as_deref(),
    );
    if same_email || same_account {
        *tokens = local.tokens;
    }
}

fn needs_profile_identity(account: &Account) -> bool {
    let name = account.display_name.trim();
    let email = account.email.as_deref().unwrap_or("").trim();
    (account.agent == Agent::Antigravity.id() || account.agent == Agent::Cursor.id())
        && (email.is_empty()
            || name.is_empty()
            || name.eq_ignore_ascii_case(&account.agent)
            || name.eq_ignore_ascii_case(email))
}

fn identity_from_tokens(tokens: &TokenSet) -> crate::oauth::Identity {
    let claim = |key| {
        tokens
            .id_token
            .as_deref()
            .and_then(|token| crate::identity::jwt_claim(token, key))
            .or_else(|| crate::identity::jwt_claim(&tokens.access_token, key))
    };
    crate::oauth::Identity {
        display_name: claim("name").or_else(|| claim("preferred_username")),
        email: claim("email"),
        ..crate::oauth::Identity::default()
    }
}

fn apply_discovered_identity(account: &mut Account, identity: &crate::oauth::Identity) {
    let current = account.display_name.trim();
    let is_placeholder = current.is_empty()
        || current.eq_ignore_ascii_case(&account.agent)
        || account
            .email
            .as_deref()
            .is_some_and(|email| current.eq_ignore_ascii_case(email.trim()))
        || identity
            .email
            .as_deref()
            .is_some_and(|email| current.eq_ignore_ascii_case(email.trim()));
    if is_placeholder {
        let candidate = identity
            .display_name
            .as_deref()
            .or(identity.email.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(&account.agent));
        if let Some(name) = candidate {
            account.display_name = name.to_string();
        }
    }
    if account
        .email
        .as_deref()
        .is_none_or(|email| email.trim().is_empty())
        && let Some(email) = identity
            .email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
    {
        account.email = Some(email.to_string());
    }
    if account.account_id.is_none()
        && let Some(account_id) = identity
            .account_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    {
        account.account_id = Some(account_id.to_string());
    }
}

fn public_account_id(account_id: Option<&str>) -> Option<String> {
    let raw = account_id?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(raw.to_string())
}

fn effective_display_name(
    agent: &str,
    stored_name: &str,
    email: Option<&str>,
    is_active: bool,
    live_name: Option<&str>,
) -> String {
    let trimmed = stored_name.trim();
    if (trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case(agent)
        || email.is_some_and(|email| trimmed.eq_ignore_ascii_case(email.trim())))
        && is_active
        && let Some(name) = live_name
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(agent))
    {
        return name.to_string();
    }
    stored_name.to_string()
}

fn clean_usage(
    agent: &str,
    usage: Option<crate::usage::UsageSnapshot>,
) -> Option<crate::usage::UsageSnapshot> {
    let mut usage = usage?;
    if agent == Agent::Grok.id() {
        usage.windows.retain(|window| window.key != "month");
    }
    Some(usage)
}

fn view_for(account: &Account) -> AccountView {
    let live = live_for(&account.agent);
    view_for_with(
        account,
        live.as_ref().and_then(|item| item.account_id.as_deref()),
        live.as_ref().and_then(|item| item.display_name.as_deref()),
    )
}

fn view_for_with(
    account: &Account,
    active: Option<&str>,
    live_display_name: Option<&str>,
) -> AccountView {
    let is_active = active == Some(account.id.as_str());
    AccountView {
        id: account.id.clone(),
        agent: account.agent.clone(),
        display_name: effective_display_name(
            &account.agent,
            &account.display_name,
            account.email.as_deref(),
            is_active,
            live_display_name,
        ),
        email: account.email.clone(),
        account_id: public_account_id(account.account_id.as_deref()),
        plan: account
            .plan
            .clone()
            .or_else(|| account.usage.as_ref().and_then(|usage| usage.plan.clone())),
        created_at: account.created_at,
        last_used_at: account.last_used_at,
        requires_reauth: account.requires_reauth,
        is_active,
        usage: clean_usage(&account.agent, account.usage.clone()),
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;

fn view_from_entry(entry: &IndexEntry, lives: &[AgentLive]) -> AccountView {
    let live = lives.iter().find(|live| live.agent == entry.agent);
    let active = live.and_then(|live| live.account_id.as_deref());
    let is_active = active == Some(entry.id.as_str());
    AccountView {
        id: entry.id.clone(),
        agent: entry.agent.clone(),
        display_name: effective_display_name(
            &entry.agent,
            &entry.display_name,
            entry.email.as_deref(),
            is_active,
            live.and_then(|live| live.display_name.as_deref()),
        ),
        email: entry.email.clone(),
        account_id: public_account_id(entry.account_id.as_deref()),
        plan: entry
            .plan
            .clone()
            .or_else(|| entry.usage.as_ref().and_then(|usage| usage.plan.clone())),
        created_at: entry.created_at,
        last_used_at: entry.last_used_at,
        requires_reauth: entry.requires_reauth,
        is_active,
        usage: clean_usage(&entry.agent, entry.usage.clone()),
    }
}

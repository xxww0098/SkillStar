//! Cursor account switching through Cursor's real `state.vscdb` session.
//!
//! Cursor stores OAuth values as individual `ItemTable` rows rather than one
//! JSON file. The adapter therefore stays outside the CLI symlink custody
//! engine and treats the database write plus read-back as one switch.

use std::path::PathBuf;

use skillstar_usage::crypto;
use skillstar_usage::oauth::token_refresh;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageError, UsageResult, storage, tool_paths, vscdb};

use super::{CliAccountState, SwitchOutcome};

pub(super) const CATALOG_ID: &str = "cursor";

pub(super) fn activate(subscription_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
    let subscription = storage::get_subscription(subscription_id)?;
    match write_subscription(&subscription) {
        Ok(outcome) => {
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            Ok((subscription, outcome))
        }
        Err(error) => Ok((
            subscription,
            SwitchOutcome::fail(CATALOG_ID, &display_path(), error.to_string()),
        )),
    }
}

pub(super) fn sync(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    write_subscription(subscription).or_else(|error| {
        Ok(SwitchOutcome::fail(
            CATALOG_ID,
            &display_path(),
            error.to_string(),
        ))
    })
}

pub(super) fn reconcile() -> UsageResult<CliAccountState> {
    let Some(live) = read_live()? else {
        return Ok(CliAccountState::Missing);
    };
    let subscriptions = storage::list_subscriptions()?;
    let Some(subscription) = subscriptions.into_iter().find(|subscription| {
        subscription.catalog_id == CATALOG_ID && matches_session(subscription, &live)
    }) else {
        return Ok(CliAccountState::Diverged);
    };

    let updated = absorb_live_session(&subscription, &live);
    if updated.access_token_encrypted != subscription.access_token_encrypted
        || updated.refresh_token_encrypted != subscription.refresh_token_encrypted
        || updated.oauth_account_id != subscription.oauth_account_id
    {
        storage::patch_oauth_credentials(&updated)?;
    }
    Ok(CliAccountState::LinkedTo {
        subscription_id: subscription.id,
    })
}

/// Adopt a Cursor-side token rotation before SkillStar spends its own refresh
/// token. This is safe only when the live session still belongs to the row;
/// an unrelated manual Cursor login must remain `Diverged`.
pub(super) fn adopt_active_session(subscription: &mut Subscription) -> UsageResult<()> {
    let Some(live) = read_live()? else {
        return Ok(());
    };
    if !matches_session(subscription, &live) {
        return Ok(());
    }
    let updated = absorb_live_session(subscription, &live);
    if updated.access_token_encrypted != subscription.access_token_encrypted
        || updated.refresh_token_encrypted != subscription.refresh_token_encrypted
        || updated.oauth_account_id != subscription.oauth_account_id
    {
        *subscription = storage::patch_oauth_credentials(&updated)?;
    }
    Ok(())
}

fn live_path() -> UsageResult<PathBuf> {
    tool_paths::cursor_state_db_path()
        .ok_or_else(|| UsageError::Other("无法解析 Cursor 数据目录".into()))
}

fn display_path() -> PathBuf {
    live_path().unwrap_or_else(|_| PathBuf::from("Cursor state.vscdb"))
}

fn read_live() -> UsageResult<Option<vscdb::CursorOAuthSession>> {
    vscdb::read_cursor_oauth_session(&live_path()?)
}

fn write_subscription(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    let access_token = secret(
        subscription.access_token_encrypted.as_deref(),
        "access_token",
    )?;
    let refresh_token = secret(
        subscription.refresh_token_encrypted.as_deref(),
        "refresh_token",
    )?;
    let path = live_path()?;
    let current_email = vscdb::read_cursor_oauth_session(&path)
        .ok()
        .flatten()
        .and_then(|session| session.email);
    let email = subscription_email(subscription).or(current_email);

    vscdb::write_cursor_oauth_session(
        &path,
        &access_token,
        &refresh_token,
        email.as_deref(),
        subscription.oauth_account_id.as_deref(),
    )?;
    Ok(SwitchOutcome::direct_ok(CATALOG_ID, &path, false))
}

fn secret(value: Option<&str>, name: &str) -> UsageResult<String> {
    value
        .map(crypto::decrypt)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| UsageError::Other(format!("Cursor 账号缺少 {name}，切换未生效")))
}

fn subscription_email(subscription: &Subscription) -> Option<String> {
    subscription
        .oauth_account_id
        .as_deref()
        .or_else(|| Some(subscription.display_name.as_str()))
        .map(str::trim)
        .filter(|value| value.contains('@'))
        .map(str::to_string)
}

fn matches_session(subscription: &Subscription, live: &vscdb::CursorOAuthSession) -> bool {
    let stored_access = subscription
        .access_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|value| !value.trim().is_empty());
    let stored_refresh = subscription
        .refresh_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|value| !value.trim().is_empty());

    stored_refresh
        .as_deref()
        .zip(live.refresh_token.as_deref())
        .is_some_and(|(stored, current)| stored == current)
        || stored_access
            .as_deref()
            .is_some_and(|stored| stored == live.access_token)
        || subscription
            .oauth_account_id
            .as_deref()
            .zip(live.auth_id.as_deref())
            .is_some_and(|(stored, current)| stored == current)
        || subscription_email(subscription)
            .as_deref()
            .zip(live.email.as_deref())
            .is_some_and(|(stored, current)| stored.eq_ignore_ascii_case(current))
}

fn absorb_live_session(
    subscription: &Subscription,
    live: &vscdb::CursorOAuthSession,
) -> Subscription {
    let mut updated = subscription.clone();
    updated.access_token_encrypted = Some(crypto::encrypt(&live.access_token));
    if let Some(refresh_token) = live.refresh_token.as_deref() {
        updated.refresh_token_encrypted = Some(crypto::encrypt(refresh_token));
    }
    updated.access_token_expires_at =
        token_refresh::jwt_exp(&live.access_token).or(subscription.access_token_expires_at);
    if updated.oauth_account_id.is_none() {
        updated.oauth_account_id = live.auth_id.clone();
    }
    updated
}

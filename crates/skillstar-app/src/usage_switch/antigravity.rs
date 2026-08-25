//! Antigravity IDE account switching.
//!
//! Antigravity is not a CLI credential file: the legacy desktop build stores
//! its OAuth session in `state.vscdb`, while current macOS builds prefer the
//! `gemini`/`antigravity` system credential. Keep this adapter separate from
//! the JSON/symlink custody engine so the UI only advertises what is actually
//! written and verified.

use std::path::PathBuf;

use skillstar_usage::crypto;
use skillstar_usage::oauth::token_refresh;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageError, UsageResult, storage, tool_paths, vscdb};

use super::{CliAccountState, SwitchOutcome};

pub(super) const CATALOG_ID: &str = "antigravity";

#[derive(Debug, Clone, PartialEq, Eq)]
struct LiveSession {
    access_token: String,
    refresh_token: String,
    expires_at: Option<i64>,
    email: Option<String>,
}

pub(super) fn activate(subscription_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
    let subscription = storage::get_subscription(subscription_id)?;
    let outcome = write_subscription(&subscription);
    match outcome {
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
    let Some(live) = read_live_session()? else {
        return Ok(CliAccountState::Missing);
    };
    let subscriptions = storage::list_subscriptions()?;
    if let Some(subscription) = subscriptions.into_iter().find(|subscription| {
        subscription.catalog_id == CATALOG_ID && matches_session(subscription, &live)
    }) {
        let updated = absorb_live_session(&subscription, &live);
        if updated.access_token_encrypted != subscription.access_token_encrypted
            || updated.refresh_token_encrypted != subscription.refresh_token_encrypted
            || updated.access_token_expires_at != subscription.access_token_expires_at
        {
            storage::patch_oauth_credentials(&updated)?;
        }
        return Ok(CliAccountState::LinkedTo {
            subscription_id: subscription.id,
        });
    }
    Ok(CliAccountState::Diverged)
}

/// Adopt a token generation that Antigravity itself rotated before Usage got
/// the refresh lock. The email is the stable ownership witness when the IDE
/// exposes it and the token values have already changed.
pub(super) fn adopt_active_session(subscription: &mut Subscription) -> UsageResult<()> {
    let Some(live) = read_live_session()? else {
        return Ok(());
    };
    if !matches_session(subscription, &live) {
        return Ok(());
    }
    let updated = absorb_live_session(subscription, &live);
    if updated.access_token_encrypted != subscription.access_token_encrypted
        || updated.refresh_token_encrypted != subscription.refresh_token_encrypted
        || updated.access_token_expires_at != subscription.access_token_expires_at
    {
        *subscription = storage::patch_oauth_credentials(&updated)?;
    }
    Ok(())
}

pub(super) fn live_path() -> UsageResult<PathBuf> {
    tool_paths::antigravity_state_db_path()
        .ok_or_else(|| UsageError::Other("无法解析 Antigravity IDE 数据目录".into()))
}

fn display_path() -> PathBuf {
    live_path().unwrap_or_else(|_| PathBuf::from("Antigravity state.vscdb"))
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
    let email = subscription
        .oauth_account_id
        .as_deref()
        .filter(|value| value.contains('@'))
        .or_else(|| {
            subscription
                .display_name
                .trim()
                .contains('@')
                .then_some(subscription.display_name.trim())
        });

    #[cfg(target_os = "macos")]
    if !skillstar_usage::tool_paths::is_tool_sync_sandboxed()
        && (read_system_session()?.is_some() || should_prefer_system_store(&path))
    {
        write_system_credential(
            &access_token,
            &refresh_token,
            subscription
                .access_token_expires_at
                .or_else(|| token_refresh::jwt_exp(&access_token))
                .unwrap_or_default(),
        )?;
        if read_system_session()?.is_none_or(|session| session.refresh_token != refresh_token) {
            return Err(UsageError::Other(
                "Antigravity macOS Keychain 回读校验失败，切换未生效".into(),
            ));
        }
        return Ok(SwitchOutcome::direct_ok(
            CATALOG_ID,
            &PathBuf::from("macOS Keychain: gemini / antigravity"),
            true,
        ));
    }

    vscdb::write_antigravity_oauth_token(
        &path,
        &access_token,
        &refresh_token,
        subscription
            .access_token_expires_at
            .or_else(|| token_refresh::jwt_exp(&access_token))
            .unwrap_or_default(),
        email,
    )?;
    Ok(SwitchOutcome::direct_ok(CATALOG_ID, &path, false))
}

#[cfg(target_os = "macos")]
fn should_prefer_system_store(state_db_path: &std::path::Path) -> bool {
    if let Some(prefers_system) = tool_paths::antigravity_prefers_system_credentials() {
        return prefers_system;
    }
    if !state_db_path.exists() {
        return true;
    }
    vscdb::read_antigravity_refresh_token(state_db_path)
        .ok()
        .flatten()
        .is_none()
}

fn secret(value: Option<&str>, name: &str) -> UsageResult<String> {
    value
        .map(crypto::decrypt)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| UsageError::Other(format!("Antigravity 账号缺少 {name}，切换未生效")))
}

fn read_live_session() -> UsageResult<Option<LiveSession>> {
    #[cfg(target_os = "macos")]
    if !tool_paths::is_tool_sync_sandboxed()
        && let Some(session) = read_system_session()?
    {
        return Ok(Some(session));
    }

    Ok(
        vscdb::read_antigravity_oauth_session(&live_path()?)?.map(|session| LiveSession {
            access_token: session.access_token,
            refresh_token: session.refresh_token,
            expires_at: session.expires_at,
            email: session.email,
        }),
    )
}

fn matches_session(subscription: &Subscription, live: &LiveSession) -> bool {
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
        .is_some_and(|stored| stored == live.refresh_token)
        || stored_access
            .as_deref()
            .is_some_and(|stored| stored == live.access_token)
        || subscription_email(subscription)
            .as_deref()
            .zip(live.email.as_deref())
            .is_some_and(|(stored, current)| stored.eq_ignore_ascii_case(current))
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

fn absorb_live_session(subscription: &Subscription, live: &LiveSession) -> Subscription {
    let mut updated = subscription.clone();
    updated.access_token_encrypted = Some(crypto::encrypt(&live.access_token));
    updated.refresh_token_encrypted = Some(crypto::encrypt(&live.refresh_token));
    updated.access_token_expires_at = live
        .expires_at
        .or_else(|| token_refresh::jwt_exp(&live.access_token))
        .or(subscription.access_token_expires_at);
    updated
}

#[cfg(target_os = "macos")]
fn read_system_session() -> UsageResult<Option<LiveSession>> {
    let output = std::process::Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "gemini",
            "-a",
            "antigravity",
            "-w",
        ])
        .output()
        .map_err(|error| {
            UsageError::Other(format!("读取 Antigravity macOS Keychain 失败：{error}"))
        })?;
    if !output.status.success() {
        return Ok(None);
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let encoded = raw
        .strip_prefix("go-keyring-base64:")
        .ok_or_else(|| UsageError::Other("Antigravity macOS Keychain 凭据格式无法识别".into()))?;
    let payload = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|error| {
            UsageError::Other(format!("解析 Antigravity macOS Keychain 失败：{error}"))
        })?;
    let value: serde_json::Value = serde_json::from_slice(&payload).map_err(|error| {
        UsageError::Other(format!("解析 Antigravity Keychain JSON 失败：{error}"))
    })?;
    let token = value
        .get("token")
        .ok_or_else(|| UsageError::Other("Antigravity Keychain 缺少 token 对象".into()))?;
    let refresh_token = token
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    let Some(refresh_token) = refresh_token else {
        return Ok(None);
    };
    let access_token = token
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| UsageError::Other("Antigravity Keychain 缺少 access_token".into()))?
        .to_string();
    let expires_at = token
        .get("expiry")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp())
        .or_else(|| token_refresh::jwt_exp(&access_token));
    let email = token_refresh::jwt_string(&access_token, &["email"]);
    Ok(Some(LiveSession {
        access_token,
        refresh_token,
        expires_at,
        email,
    }))
}

#[cfg(target_os = "macos")]
fn write_system_credential(
    access_token: &str,
    refresh_token: &str,
    expires_at: i64,
) -> UsageResult<()> {
    let expiry = chrono::DateTime::from_timestamp(expires_at, 0)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let payload = serde_json::json!({
        "token": {
            "access_token": access_token,
            "token_type": "Bearer",
            "refresh_token": refresh_token,
            "expiry": expiry,
        },
        "auth_method": "consumer",
    });
    let payload = serde_json::to_string(&payload).map_err(|error| {
        UsageError::Other(format!("序列化 Antigravity Keychain 凭据失败：{error}"))
    })?;
    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        payload.as_bytes(),
    );
    let value = format!("go-keyring-base64:{encoded}");
    let output = std::process::Command::new("/usr/bin/security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            "gemini",
            "-a",
            "antigravity",
            "-w",
            &value,
            "-A",
        ])
        .output()
        .map_err(|error| {
            UsageError::Other(format!("写入 Antigravity macOS Keychain 失败：{error}"))
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(UsageError::Other(format!(
            "写入 Antigravity macOS Keychain 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

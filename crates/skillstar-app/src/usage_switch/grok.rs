//! Grok CLI account activation.
//!
//! This private module is the only code allowed to understand the Grok
//! `auth.json` schema or the per-subscription session snapshot store.  The
//! parent module exposes the small activation/resync interface used by Tauri.

mod io;

use std::collections::HashSet;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde_json::Value;
use skillstar_usage::oauth::token_refresh;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageError, UsageResult, crypto, storage};

use self::io::{
    AuthCommitError, AuthFileLease, DiskGrokAuthFile, DiskGrokSessionStore, GrokAuthFile,
    GrokSessionStore,
};
#[cfg(test)]
use self::io::{LoadedAuth, StoredSessions, revision};
use super::SwitchOutcome;

const REQUIRED_CLI_SCOPES: [&str; 3] = [
    "grok-cli:access",
    "conversations:read",
    "conversations:write",
];

#[derive(Debug)]
struct GrokSwitchError {
    message: String,
    requires_reauth: bool,
    target_installed: bool,
}

impl GrokSwitchError {
    fn operational(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            requires_reauth: false,
            target_installed: false,
        }
    }

    fn reauth(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            requires_reauth: true,
            target_installed: false,
        }
    }

    fn commit(error: AuthCommitError) -> Self {
        Self {
            message: error.message,
            requires_reauth: false,
            target_installed: error.target_installed,
        }
    }
}

struct InstallAttempt {
    outcome: SwitchOutcome,
    target_installed: bool,
}

#[derive(Debug)]
struct InstalledTarget {
    backup: Option<PathBuf>,
    entry: Value,
}

pub(super) struct GrokRefreshLease {
    _auth: AuthFileLease,
}

async fn acquire_auth_lease(auth: &DiskGrokAuthFile) -> UsageResult<AuthFileLease> {
    let path = auth.path().to_path_buf();
    tokio::task::spawn_blocking(move || DiskGrokAuthFile { path }.lock_transaction())
        .await
        .map_err(|error| UsageError::Other(format!("Grok auth lock task failed: {error}")))?
        .map_err(UsageError::Other)
}

pub(super) async fn acquire_refresh_lease() -> UsageResult<GrokRefreshLease> {
    let auth = DiskGrokAuthFile::resolved().map_err(UsageError::Other)?;
    acquire_auth_lease(&auth)
        .await
        .map(|auth| GrokRefreshLease { _auth: auth })
}

fn oidc_client_id() -> &'static str {
    skillstar_usage::fetchers::oauth::xai::client_id()
}

fn oidc_scope_key() -> String {
    format!("https://auth.x.ai::{}", oidc_client_id())
}

/// Activate a Grok subscription. Capture/preflight happens before changing
/// the active pin. The target pin is tentative until installation succeeds;
/// any validation/write failure restores the previous binding.
pub(super) async fn activate(subscription_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
    let mut target = storage::get_subscription(subscription_id)?;
    if target.catalog_id != "xai" {
        return Err(UsageError::Other(format!(
            "Grok activation received catalog {}",
            target.catalog_id
        )));
    }

    let auth = DiskGrokAuthFile::resolved().map_err(UsageError::Other)?;
    let _auth_lease = acquire_auth_lease(&auth).await?;
    let preflight = auth.load().map_err(UsageError::Other)?;
    let mut sessions = DiskGrokSessionStore::open_default().map_err(UsageError::Other)?;
    let previous_id = storage::get_active_subscription("xai")?;
    capture_disk_owner(&preflight.root, &mut sessions, previous_id.as_deref())?;
    // Capture may have copied a CLI-side token rotation back to this target.
    target = storage::get_subscription(subscription_id)?;

    // Network refresh can yield or be cancelled, so it must finish before the
    // persisted pin changes. From the tentative pin through install/rollback
    // there are deliberately no `.await` points.
    if let Err(outcome) = prepare_target(&auth, &mut target).await {
        return Ok((target, outcome));
    }
    storage::set_active_subscription("xai", &target.id)?;
    let attempt = install_prepared(&auth, &mut sessions, &mut target);
    if !attempt.outcome.success && !attempt.target_installed {
        reconcile_active_pin(&auth, previous_id.as_deref())?;
    }
    Ok((target, attempt.outcome))
}

pub(super) async fn resync(subscription_id: &str) -> UsageResult<SwitchOutcome> {
    let mut target = storage::get_subscription(subscription_id)?;
    if target.catalog_id != "xai" {
        return Err(UsageError::Other(format!(
            "Grok resync received catalog {}",
            target.catalog_id
        )));
    }
    let auth = DiskGrokAuthFile::resolved().map_err(UsageError::Other)?;
    let _auth_lease = acquire_auth_lease(&auth).await?;
    let preflight = auth.load().map_err(UsageError::Other)?;
    let mut sessions = DiskGrokSessionStore::open_default().map_err(UsageError::Other)?;
    capture_disk_owner(&preflight.root, &mut sessions, Some(&target.id))?;
    target = storage::get_subscription(subscription_id)?;
    if let Err(outcome) = prepare_target(&auth, &mut target).await {
        return Ok(outcome);
    }
    Ok(install_prepared(&auth, &mut sessions, &mut target).outcome)
}

pub(super) fn forget(subscription_id: &str) -> UsageResult<()> {
    let mut sessions = DiskGrokSessionStore::open_default().map_err(UsageError::Other)?;
    sessions.remove(subscription_id).map_err(UsageError::Other)
}

pub(super) fn sync_refreshed_active(
    target: &mut Subscription,
    _lease: &GrokRefreshLease,
) -> UsageResult<SwitchOutcome> {
    if storage::get_active_subscription("xai")?.as_deref() != Some(target.id.as_str()) {
        return Err(UsageError::Other(format!(
            "Grok refresh sync target {} is not active",
            target.id
        )));
    }
    let auth = DiskGrokAuthFile::resolved().map_err(UsageError::Other)?;
    let preflight = auth.load().map_err(UsageError::Other)?;
    let mut sessions = DiskGrokSessionStore::open_default().map_err(UsageError::Other)?;
    capture_disk_owner(&preflight.root, &mut sessions, Some(&target.id))?;
    *target = storage::get_subscription(&target.id)?;
    Ok(install_prepared(&auth, &mut sessions, target).outcome)
}

pub(super) fn adopt_active_before_refresh(
    target: &mut Subscription,
    _lease: &GrokRefreshLease,
) -> UsageResult<()> {
    if storage::get_active_subscription("xai")?.as_deref() != Some(target.id.as_str()) {
        return Ok(());
    }
    // Grok may have rotated R1→R2 since the last Usage refresh. Once the
    // official auth lock is held, adopt that sibling generation before making
    // any token request so SkillStar never double-spends stale R1.
    let auth = DiskGrokAuthFile::resolved().map_err(UsageError::Other)?;
    let loaded = auth.load().map_err(UsageError::Other)?;
    let mut sessions = DiskGrokSessionStore::open_default().map_err(UsageError::Other)?;
    capture_disk_owner(&loaded.root, &mut sessions, Some(&target.id))?;
    *target = storage::get_subscription(&target.id)?;
    Ok(())
}

fn reconcile_active_pin<A: GrokAuthFile>(auth: &A, fallback_id: Option<&str>) -> UsageResult<()> {
    // Another SkillStar process may have completed a different switch while
    // this attempt waited on the auth-file lock. Prefer the identity that is
    // actually on disk over blindly restoring our stale `previous_id`.
    let desired = match auth.load() {
        Ok(loaded) => {
            let Some(entry) = loaded.root.get(oidc_scope_key()) else {
                storage::clear_active_subscription("xai")?;
                return Ok(());
            };
            let mut matches = storage::list_subscriptions()?
                .into_iter()
                .filter(|subscription| {
                    subscription.catalog_id == "xai"
                        && entry_matches_subscription(entry, subscription)
                });
            let owner = matches.next();
            match (owner, matches.next()) {
                (Some(owner), None) => Some(owner.id),
                // A real auth entry exists but no longer belongs uniquely to a
                // row (for example targeted OAuth rebound that row). Never pin
                // a fallback identity that disagrees with disk reality.
                _ => None,
            }
        }
        Err(_) => fallback_id
            .filter(|id| storage::get_subscription(id).is_ok())
            .map(str::to_string),
    };
    match desired {
        Some(id) => storage::set_active_subscription("xai", &id),
        None => storage::clear_active_subscription("xai"),
    }
}

fn capture_disk_owner<S: GrokSessionStore>(
    root: &Value,
    sessions: &mut S,
    preferred_id: Option<&str>,
) -> UsageResult<()> {
    let Some(entry) = root.get(oidc_scope_key()) else {
        return Ok(());
    };
    let subscriptions: Vec<_> = storage::list_subscriptions()?
        .into_iter()
        .filter(|subscription| subscription.catalog_id == "xai")
        .collect();
    let preferred = preferred_id
        .and_then(|id| {
            subscriptions
                .iter()
                .find(|subscription| subscription.id == id)
        })
        .filter(|subscription| entry_matches_subscription(entry, subscription));
    let mut matches = subscriptions
        .iter()
        .filter(|subscription| entry_matches_subscription(entry, subscription));
    let owner = if let Some(preferred) = preferred {
        preferred
    } else {
        let Some(owner) = matches.next() else {
            return Ok(());
        };
        if matches.next().is_some() {
            return Err(UsageError::Other(
                "Grok 磁盘会话同时匹配多个订阅，已拒绝覆盖；请重新授权目标账号".into(),
            ));
        }
        owner
    };

    // A Usage refresh can be newer than the CLI file. Prefer a CLI-viable card
    // entry in that case; otherwise preserve the real disk session without
    // downgrading the card. Snapshot persistence remains a precondition for
    // changing either the pin or auth.json.
    let normalized_disk = normalize_entry(entry.clone(), owner)
        .and_then(|entry| {
            validate_restorable_snapshot(&entry)?;
            Ok(entry)
        })
        .ok();
    let disk_is_usable = normalized_disk.is_some();
    let disk_is_fresh = normalized_disk
        .as_ref()
        .is_some_and(|entry| entry_is_at_least_as_fresh(entry, owner));
    let snapshot = if disk_is_fresh {
        normalized_disk.expect("disk_is_fresh requires normalized disk entry")
    } else if let Ok(card_entry) = build_entry_from_subscription(owner, Some(entry)) {
        card_entry
    } else if disk_is_usable {
        normalized_disk.expect("disk_is_usable requires normalized disk entry")
    } else {
        return Err(UsageError::Other(
            "Grok 当前磁盘会话无法安全保存，已中止切换以避免覆盖；请先在 Grok 中重新登录".into(),
        ));
    };
    sessions
        .save(&owner.id, &snapshot)
        .map_err(UsageError::Other)?;
    if disk_is_fresh {
        let mut owner = owner.clone();
        sync_subscription_from_entry(&mut owner, &snapshot);
        storage::patch_oauth_credentials(&owner)?;
    }
    Ok(())
}

async fn prepare_target<A: GrokAuthFile>(
    auth: &A,
    target: &mut Subscription,
) -> Result<(), SwitchOutcome> {
    let effective_expiry = effective_access_token_expiry(target);
    let has_refresh = target
        .refresh_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .is_some_and(|token| !token.trim().is_empty());
    let refresh_was_needed = token_refresh::needs_refresh(effective_expiry)
        || (effective_expiry.is_none() && has_refresh);
    if refresh_was_needed {
        match skillstar_usage::fetchers::oauth::xai::refresh_for_cli_switch(target).await {
            Ok(()) => {
                if !token_is_known_valid(target) {
                    return Err(SwitchOutcome::fail(
                        "grok",
                        auth.path().to_path_buf(),
                        "Grok token 刷新后仍缺少可验证的有效期，请重新授权该账号",
                    ));
                }
                target.requires_reauth = false;
                match storage::patch_oauth_credentials(target) {
                    Ok(saved) => *target = saved,
                    Err(error) => {
                        return Err(SwitchOutcome::fail(
                            "grok",
                            auth.path().to_path_buf(),
                            format!("Grok 刷新成功但凭据保存失败：{error}"),
                        ));
                    }
                }
            }
            Err(UsageError::AuthRequired) => {
                target.requires_reauth = true;
                let _ = storage::patch_oauth_credentials(target);
                return Err(SwitchOutcome::fail(
                    "grok",
                    auth.path().to_path_buf(),
                    "Grok 登录授权已失效，请重新授权该账号后再同步到 CLI",
                ));
            }
            Err(error) if !token_is_known_valid(target) => {
                return Err(SwitchOutcome::fail(
                    "grok",
                    auth.path().to_path_buf(),
                    format!("Grok token 已过期或有效期未知，且刷新失败：{error}"),
                ));
            }
            Err(error) => {
                tracing::warn!(error = %error, "grok CLI pre-switch refresh failed; using still-valid token");
            }
        }
    }
    Ok(())
}

fn install_prepared<A: GrokAuthFile, S: GrokSessionStore>(
    auth: &A,
    sessions: &mut S,
    target: &mut Subscription,
) -> InstallAttempt {
    match install_target(auth, sessions, target) {
        Ok(installed) => {
            sync_subscription_from_entry(target, &installed.entry);
            target.requires_reauth = false;
            if let Ok(saved) = storage::patch_oauth_credentials(target) {
                *target = saved;
            }
            InstallAttempt {
                outcome: SwitchOutcome::ok(
                    "grok",
                    auth.path().to_path_buf(),
                    installed.backup,
                    false,
                ),
                target_installed: true,
            }
        }
        Err(error) => {
            if error.requires_reauth {
                target.requires_reauth = true;
                if let Ok(saved) = storage::patch_oauth_credentials(target) {
                    *target = saved;
                }
            }
            InstallAttempt {
                outcome: SwitchOutcome::fail("grok", auth.path().to_path_buf(), error.message),
                target_installed: error.target_installed,
            }
        }
    }
}

fn effective_access_token_expiry(subscription: &Subscription) -> Option<i64> {
    let jwt_expiry = subscription
        .access_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|token| !token.is_empty())
        .and_then(|token| token_refresh::jwt_exp(&token));
    match (subscription.access_token_expires_at, jwt_expiry) {
        (Some(stored), Some(jwt)) => Some(stored.min(jwt)),
        (stored, jwt) => stored.or(jwt),
    }
}

fn token_is_known_valid(subscription: &Subscription) -> bool {
    effective_access_token_expiry(subscription)
        .is_some_and(|expires_at| expires_at > Utc::now().timestamp())
}

fn install_target<A: GrokAuthFile, S: GrokSessionStore>(
    auth: &A,
    sessions: &mut S,
    target: &Subscription,
) -> Result<InstalledTarget, GrokSwitchError> {
    let mut loaded = auth.load().map_err(GrokSwitchError::operational)?;
    let scope = oidc_scope_key();
    let current = loaded
        .root
        .get(&scope)
        .filter(|entry| entry_matches_subscription(entry, target))
        .cloned();
    let stored = sessions
        .load(&target.id)
        .map_err(GrokSwitchError::operational)?
        .filter(|entry| !entry_identity_conflicts(entry, target));
    // A matching live disk entry may contain a refresh-token rotation made by
    // Grok after our preflight capture; it is newer than the stored template.
    let current_is_freshest = current
        .as_ref()
        .is_some_and(|entry| entry_is_at_least_as_fresh(entry, target));
    let base = current.clone().or(stored);

    let entry = if current_is_freshest {
        current.expect("current_is_freshest requires a current entry")
    } else {
        match build_entry_from_subscription(target, base.as_ref()) {
            Ok(entry) => entry,
            Err(token_error) => {
                let Some(snapshot) = base else {
                    return Err(token_error);
                };
                snapshot
            }
        }
    };
    let entry = normalize_entry(entry, target)?;
    validate_restorable_snapshot(&entry)?;
    validate_entry_identity(&entry, target)?;

    // Snapshot persistence is a precondition for touching the CLI file.
    sessions
        .save(&target.id, &entry)
        .map_err(GrokSwitchError::operational)?;
    loaded
        .root
        .as_object_mut()
        .expect("load() guarantees an object root")
        .insert(scope.clone(), entry.clone());
    let backup = auth
        .commit_verified(&loaded, &scope, &entry)
        .map_err(GrokSwitchError::commit)?;
    Ok(InstalledTarget { backup, entry })
}

fn entry_is_at_least_as_fresh(entry: &Value, subscription: &Subscription) -> bool {
    let entry_expiry = entry
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp());
    match (entry_expiry, subscription.access_token_expires_at) {
        (Some(entry), Some(subscription)) => entry >= subscription,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => true,
    }
}

fn build_entry_from_subscription(
    subscription: &Subscription,
    base: Option<&Value>,
) -> Result<Value, GrokSwitchError> {
    let access_token = subscription
        .access_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| GrokSwitchError::reauth("Grok 切号缺少 access_token，请重新授权该账号"))?;
    match inspect_token_scopes(&access_token) {
        ScopeInspection::Valid => {}
        ScopeInspection::Missing(missing) => {
            return Err(GrokSwitchError::reauth(format!(
                "Grok 账号授权缺少 CLI scope（{}），请重新授权一次",
                missing.join(", ")
            )));
        }
        ScopeInspection::Opaque => {
            return Err(GrokSwitchError::reauth(
                "无法验证此 Grok 账号的 CLI scope，请重新授权一次",
            ));
        }
    }

    let mut entry = base.and_then(Value::as_object).cloned().unwrap_or_default();
    entry.insert("key".into(), Value::String(access_token));
    if let Some(refresh_token) = subscription
        .refresh_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|token| !token.is_empty())
    {
        entry.insert("refresh_token".into(), Value::String(refresh_token));
    }
    let expires_at = effective_access_token_expiry(subscription);
    if let Some(expires_at) = expires_at.and_then(format_expires_at) {
        entry.insert("expires_at".into(), Value::String(expires_at));
    }
    normalize_entry(Value::Object(entry), subscription)
}

fn normalize_entry(entry: Value, subscription: &Subscription) -> Result<Value, GrokSwitchError> {
    let mut entry = entry
        .as_object()
        .cloned()
        .ok_or_else(|| GrokSwitchError::reauth("Grok session snapshot 不是 JSON 对象"))?;
    entry
        .entry("auth_mode")
        .or_insert_with(|| Value::String("oidc".into()));
    entry
        .entry("oidc_issuer")
        .or_insert_with(|| Value::String("https://auth.x.ai".into()));
    entry
        .entry("oidc_client_id")
        .or_insert_with(|| Value::String(oidc_client_id().into()));

    let key = entry
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let claims = token_refresh::decode_jwt_payload(&key);
    if let Some(claims) = claims.as_ref() {
        for field in [
            "email",
            "user_id",
            "principal_id",
            "principal_type",
            "team_id",
            "team_name",
            "team_role",
            "subscription_tier",
            "first_name",
            "last_name",
            "profile_image_asset_id",
        ] {
            if let Some(value) = claims.get(field).and_then(Value::as_str) {
                entry
                    .entry(field)
                    .or_insert_with(|| Value::String(value.to_string()));
            }
        }
        if !entry.contains_key("user_id")
            && let Some(subject) = claims.get("sub").and_then(Value::as_str)
        {
            entry.insert("user_id".into(), Value::String(subject.to_string()));
        }
        if let Some(value) = claims
            .get("coding_data_retention_opt_out")
            .and_then(Value::as_bool)
        {
            entry
                .entry("coding_data_retention_opt_out")
                .or_insert(Value::Bool(value));
        }
    }
    if let Some(email) = subscription_email(subscription) {
        entry.entry("email").or_insert(Value::String(email));
    }
    if let Some(user_id) = subscription_user_id(subscription) {
        entry
            .entry("user_id")
            .or_insert_with(|| Value::String(user_id.clone()));
        entry
            .entry("principal_id")
            .or_insert(Value::String(user_id));
    }

    let target_token = subscription
        .access_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|token| !token.is_empty());
    let same_token = target_token.as_deref() == Some(key.as_str());
    if !entry.contains_key("refresh_token")
        && same_token
        && let Some(refresh_token) = subscription
            .refresh_token_encrypted
            .as_deref()
            .map(crypto::decrypt)
            .filter(|token| !token.trim().is_empty())
    {
        entry.insert("refresh_token".into(), Value::String(refresh_token));
    }
    if !entry.contains_key("expires_at") {
        let expiry = token_refresh::jwt_exp(&key).or_else(|| {
            same_token
                .then(|| effective_access_token_expiry(subscription))
                .flatten()
        });
        if let Some(expires_at) = expiry.and_then(format_expires_at) {
            entry.insert("expires_at".into(), Value::String(expires_at));
        }
    }

    let email_prefix = entry
        .get("email")
        .and_then(Value::as_str)
        .and_then(|email| email.split('@').next())
        .unwrap_or_default()
        .to_string();
    entry
        .entry("create_time")
        .or_insert_with(|| Value::String(now_rfc3339()));
    entry
        .entry("first_name")
        .or_insert_with(|| Value::String(email_prefix));
    for field in ["last_name", "profile_image_asset_id", "team_id"] {
        entry
            .entry(field)
            .or_insert_with(|| Value::String(String::new()));
    }
    entry
        .entry("principal_type")
        .or_insert_with(|| Value::String("user".into()));
    entry
        .entry("coding_data_retention_opt_out")
        .or_insert(Value::Bool(false));
    Ok(Value::Object(entry))
}

fn validate_restorable_snapshot(entry: &Value) -> Result<(), GrokSwitchError> {
    let expected_fields = [
        ("auth_mode", "oidc"),
        ("oidc_issuer", "https://auth.x.ai"),
        ("oidc_client_id", oidc_client_id()),
    ];
    for (field, expected) in expected_fields {
        if entry.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(GrokSwitchError::reauth(format!(
                "Grok session snapshot 的 {field} 不匹配，请重新授权一次"
            )));
        }
    }
    let token = entry
        .get("key")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| GrokSwitchError::reauth("Grok session snapshot 缺少 access token"))?;
    for field in [
        "create_time",
        "user_id",
        "email",
        "first_name",
        "last_name",
        "profile_image_asset_id",
        "principal_type",
        "principal_id",
        "team_id",
    ] {
        if entry.get(field).and_then(Value::as_str).is_none() {
            return Err(GrokSwitchError::reauth(format!(
                "Grok session snapshot 缺少必填字段 {field}"
            )));
        }
    }
    if !entry
        .get("coding_data_retention_opt_out")
        .is_some_and(Value::is_boolean)
    {
        return Err(GrokSwitchError::reauth(
            "Grok session snapshot 缺少必填字段 coding_data_retention_opt_out",
        ));
    }
    let entry_expiry = entry
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp());
    let jwt_expiry = token_refresh::jwt_exp(token);
    let parsed_expiry = match (entry_expiry, jwt_expiry) {
        (Some(entry), Some(jwt)) => Some(entry.min(jwt)),
        (entry, jwt) => entry.or(jwt),
    };
    let has_refresh = entry
        .get("refresh_token")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.trim().is_empty());
    if !has_refresh {
        return Err(GrokSwitchError::reauth(
            "Grok session snapshot 缺少 refresh token，请重新授权一次",
        ));
    }
    if parsed_expiry.is_none() {
        return Err(GrokSwitchError::reauth(
            "Grok session snapshot 缺少可验证的有效期，请重新授权一次",
        ));
    }
    match inspect_token_scopes(token) {
        ScopeInspection::Valid | ScopeInspection::Opaque => Ok(()),
        ScopeInspection::Missing(missing) => Err(GrokSwitchError::reauth(format!(
            "Grok session snapshot 缺少 CLI scope（{}），请重新授权一次",
            missing.join(", ")
        ))),
    }
}

fn validate_entry_identity(
    entry: &Value,
    subscription: &Subscription,
) -> Result<(), GrokSwitchError> {
    if entry_identity_conflicts(entry, subscription) {
        return Err(GrokSwitchError::operational(
            "Grok session snapshot 的账号身份与目标订阅不匹配，已拒绝写入",
        ));
    }
    Ok(())
}

enum ScopeInspection {
    Valid,
    Missing(Vec<&'static str>),
    Opaque,
}

fn inspect_token_scopes(token: &str) -> ScopeInspection {
    let Some(claims) = token_refresh::decode_jwt_payload(token) else {
        return ScopeInspection::Opaque;
    };
    let scopes = scopes_from_value(&claims);
    let missing: Vec<_> = REQUIRED_CLI_SCOPES
        .iter()
        .copied()
        .filter(|required| !scopes.contains(*required))
        .collect();
    if missing.is_empty() {
        ScopeInspection::Valid
    } else {
        ScopeInspection::Missing(missing)
    }
}

fn scopes_from_value(value: &Value) -> HashSet<String> {
    let mut scopes = HashSet::new();
    for key in ["scope", "scp"] {
        match value.get(key) {
            Some(Value::String(raw)) => {
                scopes.extend(raw.split_whitespace().map(str::to_string));
            }
            Some(Value::Array(values)) => {
                scopes.extend(values.iter().filter_map(Value::as_str).map(str::to_string));
            }
            _ => {}
        }
    }
    scopes
}

fn entry_matches_subscription(entry: &Value, subscription: &Subscription) -> bool {
    identities_match(&entry_identity(entry), &subscription_identity(subscription))
}

fn entry_identity_conflicts(entry: &Value, subscription: &Subscription) -> bool {
    identities_conflict(&entry_identity(entry), &subscription_identity(subscription))
}

#[derive(Default)]
struct AccountIdentity {
    subjects: HashSet<String>,
    emails: HashSet<String>,
}

fn entry_identity(entry: &Value) -> AccountIdentity {
    let mut identity = AccountIdentity::default();
    for field in ["sub", "user_id", "principal_id"] {
        insert_identity_value(&mut identity.subjects, entry.get(field));
    }
    insert_identity_value(&mut identity.emails, entry.get("email"));
    identity
}

fn subscription_identity(subscription: &Subscription) -> AccountIdentity {
    let mut identity = AccountIdentity::default();
    // The current access token is authoritative. An omitted id_token may have
    // been preserved from an older authorization, so only use it for identity
    // categories that the access token cannot provide. Stored account metadata
    // is the final fallback, never a peer that can make conflicting identities
    // appear to match.
    for encrypted in [
        subscription.access_token_encrypted.as_deref(),
        subscription.id_token_encrypted.as_deref(),
    ] {
        if let Some(claims) = encrypted
            .map(crypto::decrypt)
            .filter(|token| !token.is_empty())
            .and_then(|token| token_refresh::decode_jwt_payload(&token))
        {
            if identity.subjects.is_empty() {
                for field in ["sub", "user_id", "principal_id"] {
                    insert_identity_value(&mut identity.subjects, claims.get(field));
                }
            }
            if identity.emails.is_empty() {
                insert_identity_value(&mut identity.emails, claims.get("email"));
            }
        }
    }
    if let Some(account_id) = subscription.oauth_account_id.as_deref() {
        if account_id.contains('@') && identity.emails.is_empty() {
            insert_identity_str(&mut identity.emails, account_id);
        } else if !account_id.contains('@') && identity.subjects.is_empty() {
            insert_identity_str(&mut identity.subjects, account_id);
        }
    }
    identity
}

fn identities_match(left: &AccountIdentity, right: &AccountIdentity) -> bool {
    if identity_is_ambiguous(left) || identity_is_ambiguous(right) {
        return false;
    }
    if !left.subjects.is_empty() && !right.subjects.is_empty() {
        return left
            .subjects
            .iter()
            .any(|subject| right.subjects.contains(subject));
    }
    !left.emails.is_empty()
        && !right.emails.is_empty()
        && left.emails.iter().any(|email| right.emails.contains(email))
}

fn identities_conflict(left: &AccountIdentity, right: &AccountIdentity) -> bool {
    if identity_is_ambiguous(left) || identity_is_ambiguous(right) {
        return true;
    }
    if !left.subjects.is_empty() && !right.subjects.is_empty() {
        return !left
            .subjects
            .iter()
            .any(|subject| right.subjects.contains(subject));
    }
    !left.emails.is_empty()
        && !right.emails.is_empty()
        && !left.emails.iter().any(|email| right.emails.contains(email))
}

fn identity_is_ambiguous(identity: &AccountIdentity) -> bool {
    identity.subjects.len() > 1 || identity.emails.len() > 1
}

fn insert_identity_value(values: &mut HashSet<String>, value: Option<&Value>) {
    if let Some(value) = value.and_then(Value::as_str) {
        insert_identity_str(values, value);
    }
}

fn insert_identity_str(values: &mut HashSet<String>, value: &str) {
    let value = normalize_identity(value);
    if !value.is_empty() {
        values.insert(value);
    }
}

fn subscription_email(subscription: &Subscription) -> Option<String> {
    if let Some(email) = subscription_identity(subscription)
        .emails
        .into_iter()
        .next()
    {
        return Some(email);
    }
    // Display name is only a construction fallback, never an ownership key.
    let display = subscription.display_name.trim();
    let display = display.strip_prefix("Grok · ").unwrap_or(display).trim();
    display.contains('@').then(|| display.to_string())
}

fn subscription_user_id(subscription: &Subscription) -> Option<String> {
    subscription_identity(subscription)
        .subjects
        .into_iter()
        .next()
}

fn normalize_identity(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn sync_subscription_from_entry(subscription: &mut Subscription, entry: &Value) {
    if let Some(token) = entry.get("key").and_then(Value::as_str) {
        subscription.access_token_encrypted = Some(crypto::encrypt(token));
    }
    subscription.refresh_token_encrypted = entry
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(crypto::encrypt);
    subscription.access_token_expires_at = entry
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|expires_at| DateTime::parse_from_rfc3339(expires_at).ok())
        .map(|parsed| parsed.timestamp());
    if let Some(user_id) = entry
        .get("user_id")
        .or_else(|| entry.get("principal_id"))
        .or_else(|| entry.get("sub"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        subscription.oauth_account_id = Some(user_id.to_string());
    }
    subscription.requires_reauth = false;
}

fn format_expires_at(seconds: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(seconds, 0)
        .map(|date| date.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string())
}

fn now_rfc3339() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

#[cfg(test)]
#[path = "grok_tests.rs"]
mod tests;

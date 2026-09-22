//! CodeBuddy and CodeBuddy CN account switching through `state.vscdb`.
//!
//! One adapter, two catalogs. The suffix difference (`accessToken` vs
//! `accessTokencn`, session id `…-cn`) lives on [`Profile`] and is not appended
//! at the write site. Backup, one ItemTable upsert, read-back, then pin. Only
//! that catalog's `secret://` auth key is written or cleared. Values use
//! [`skillstar_usage::tool_store::safe_storage`] and a test-injected password.
//! This path does not read the system keychain and does not create or copy a
//! database. Restarting the official CodeBuddy app is not verified here.

use std::path::{Path, PathBuf};

use serde_json::Value;
use skillstar_models::tool_sync::create_rolling_backup;
use skillstar_usage::crypto;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::tool_store::safe_storage::{self, KeyMaterial};
use skillstar_usage::{UsageError, UsageResult, storage, tool_paths, vscdb};

use super::ide::IdeCredentialAdapter;
use super::{CliAccountState, SwitchOutcome};

const SECRET_EXTENSION: &str = "tencent-cloud.coding-copilot";

#[cfg(test)]
const READBACK_FAIL_ENV: &str = "SKILLSTAR_CODEBUDDY_READBACK_FAIL";

struct Profile {
    catalog_id: &'static str,
    display_name: &'static str,
    /// Full safe-storage item name. CN is `accessTokencn`, not a hyphenated
    /// suffix and not something [`Profile::item_key`] appends.
    secret_key: &'static str,
    session_id: &'static str,
    password_env: &'static str,
    state_db: fn() -> Option<PathBuf>,
}

impl Profile {
    fn item_key(&self) -> String {
        format!(
            r#"secret://{{"extensionId":"{SECRET_EXTENSION}","key":"{}"}}"#,
            self.secret_key
        )
    }
}

static GLOBAL: Profile = Profile {
    catalog_id: "codebuddy",
    display_name: "CodeBuddy",
    secret_key: "planning-genie.new.accessToken",
    session_id: "Tencent-Cloud.genie-ide",
    password_env: "SKILLSTAR_CODEBUDDY_SAFE_STORAGE_PASSWORD",
    state_db: tool_paths::codebuddy_state_db_path,
};

static CN: Profile = Profile {
    catalog_id: "codebuddy-cn",
    display_name: "CodeBuddy CN",
    secret_key: "planning-genie.new.accessTokencn",
    session_id: "Tencent-Cloud.genie-ide-cn",
    password_env: "SKILLSTAR_CODEBUDDY_CN_SAFE_STORAGE_PASSWORD",
    state_db: tool_paths::codebuddy_cn_state_db_path,
};

pub(super) struct Adapter {
    profile: &'static Profile,
}

impl IdeCredentialAdapter for Adapter {
    fn catalog_id(&self) -> &'static str {
        self.profile.catalog_id
    }

    fn available(&self) -> bool {
        // Addressable store, not "a database is present". A missing file stays
        // in the reconcile map as `Missing`.
        (self.profile.state_db)().is_some()
    }

    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
        activate(self.profile, sub_id)
    }

    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome> {
        sync(self.profile, sub)
    }

    fn reconcile(&self) -> UsageResult<Option<CliAccountState>> {
        if !self.available() {
            return Ok(None);
        }
        reconcile(self.profile).map(Some)
    }

    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()> {
        adopt_active_session(self.profile, sub)
    }

    fn forget(&self, sub_id: &str) -> UsageResult<()> {
        forget_account(self.profile, sub_id)
    }
}

pub(super) static GLOBAL_ADAPTER: Adapter = Adapter { profile: &GLOBAL };
pub(super) static CN_ADAPTER: Adapter = Adapter { profile: &CN };

fn activate(
    profile: &Profile,
    subscription_id: &str,
) -> UsageResult<(Subscription, SwitchOutcome)> {
    let subscription = storage::get_subscription(subscription_id)?;
    match write_subscription(profile, &subscription) {
        Ok(outcome) => {
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            Ok((subscription, outcome))
        }
        Err(error) => Ok((subscription, failed(profile, error))),
    }
}

fn sync(profile: &Profile, subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    write_subscription(profile, subscription).or_else(|error| Ok(failed(profile, error)))
}

fn reconcile(profile: &Profile) -> UsageResult<CliAccountState> {
    let Some(live) = read_live(profile, &live_path(profile)?)? else {
        return Ok(CliAccountState::Missing);
    };
    if live.locked {
        return Ok(CliAccountState::Diverged);
    }
    let Some(subscription) = matching_subscription(profile, &live)? else {
        return Ok(CliAccountState::Diverged);
    };
    let updated = absorb(&subscription, &live);
    if credentials_changed(&updated, &subscription) {
        storage::patch_oauth_credentials(&updated)?;
    }
    Ok(CliAccountState::LinkedTo {
        subscription_id: subscription.id,
    })
}

fn adopt_active_session(profile: &Profile, subscription: &mut Subscription) -> UsageResult<()> {
    if subscription.catalog_id != profile.catalog_id {
        return Ok(());
    }
    let Ok(path) = live_path(profile) else {
        return Ok(());
    };
    let Some(live) = read_live(profile, &path)? else {
        return Ok(());
    };
    if !same_account(subscription, &live) {
        return Ok(());
    }
    let updated = absorb(subscription, &live);
    if credentials_changed(&updated, subscription) {
        *subscription = storage::patch_oauth_credentials(&updated)?;
    }
    Ok(())
}

fn forget_account(profile: &Profile, subscription_id: &str) -> UsageResult<()> {
    let Ok(subscription) = storage::get_subscription(subscription_id) else {
        return Ok(());
    };
    if subscription.catalog_id != profile.catalog_id {
        return Ok(());
    }
    let Ok(path) = live_path(profile) else {
        return Ok(());
    };
    if !path.is_file() {
        return Ok(());
    }
    let Some(live) = read_live(profile, &path)? else {
        return Ok(());
    };
    if !same_account(&subscription, &live) {
        return Ok(());
    }
    let backup = backup_db(profile, &path)?;
    if let Err(error) = clear_auth(profile, &path) {
        return Err(restore_or_combine(profile, &path, &backup, error));
    }
    Ok(())
}

fn write_subscription(
    profile: &Profile,
    subscription: &Subscription,
) -> UsageResult<SwitchOutcome> {
    if subscription.catalog_id != profile.catalog_id {
        return Err(UsageError::Other(format!(
            "{} 切换收到了其它 catalog 的订阅",
            profile.display_name
        )));
    }
    let path = live_path(profile)?;
    require_db(profile, &path)?;
    let prepared = prepare(profile, subscription)?;
    let backup = commit(profile, &path, &prepared)?;
    Ok(success_outcome(profile, &path, &backup))
}

fn live_path(profile: &Profile) -> UsageResult<PathBuf> {
    (profile.state_db)()
        .ok_or_else(|| UsageError::Other(format!("无法解析 {} 数据目录", profile.display_name)))
}

fn display_path(profile: &Profile) -> PathBuf {
    live_path(profile)
        .unwrap_or_else(|_| PathBuf::from(format!("{} state.vscdb", profile.display_name)))
}

fn require_db(profile: &Profile, path: &Path) -> UsageResult<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(UsageError::Other(format!(
            "未找到 {} state.vscdb：{}。请先启动 {} 并完成一次登录",
            profile.display_name,
            path.display(),
            profile.display_name
        )))
    }
}

struct Material {
    token: String,
    refresh: Option<String>,
    expires_at: Option<i64>,
    uid: Option<String>,
    nickname: Option<String>,
    enterprise_id: Option<String>,
    enterprise_name: Option<String>,
    domain: Option<String>,
}

struct AuthWrite {
    plain: String,
    stored: String,
}

struct LiveAuth {
    token: Option<String>,
    uid: Option<String>,
    refresh: Option<String>,
    expires_at: Option<i64>,
    locked: bool,
}

impl LiveAuth {
    fn locked() -> Self {
        Self {
            token: None,
            uid: None,
            refresh: None,
            expires_at: None,
            locked: true,
        }
    }
}

fn prepare(profile: &Profile, subscription: &Subscription) -> UsageResult<AuthWrite> {
    let material = material_of(profile, subscription)?;
    let key = safe_storage_key(profile)?;
    let plain = session_json(profile, &material)?;
    let stored = encrypt_with(profile, &key, &plain)?;
    Ok(AuthWrite { plain, stored })
}

fn material_of(profile: &Profile, subscription: &Subscription) -> UsageResult<Material> {
    let raw = secret_text(&subscription.access_token_encrypted).ok_or_else(|| {
        UsageError::Other(format!(
            "{} 账号缺少 access_token，切换未生效",
            profile.display_name
        ))
    })?;
    let (uid_from_token, token) = split_uid_token(&raw);
    if token.is_empty() {
        return Err(UsageError::Other(format!(
            "{} 账号缺少 access_token，切换未生效",
            profile.display_name
        )));
    }
    let uid = nonempty(subscription.oauth_account_id.clone())
        .filter(|id| id != &token)
        .or(uid_from_token);
    let (enterprise_id, enterprise_name, domain) = enterprise_of(subscription);
    Ok(Material {
        token,
        refresh: secret_text(&subscription.refresh_token_encrypted),
        expires_at: subscription
            .access_token_expires_at
            .filter(|value| *value > 0),
        nickname: nickname_of(profile, subscription, uid.as_deref()),
        uid,
        enterprise_id,
        enterprise_name,
        domain,
    })
}

fn session_json(profile: &Profile, material: &Material) -> UsageResult<String> {
    let uid = material.uid.as_deref().unwrap_or("");
    let nickname = material.nickname.as_deref().unwrap_or("");
    let enterprise_id = material.enterprise_id.as_deref().unwrap_or("");
    let enterprise_name = material.enterprise_name.as_deref().unwrap_or("");
    let domain = material.domain.as_deref().unwrap_or("");
    let refresh = material.refresh.as_deref().unwrap_or("");
    let expires_at = material.expires_at.unwrap_or(0);
    let value = serde_json::json!({
        "id": profile.session_id,
        "token": material.token,
        "refreshToken": refresh,
        "expiresAt": expires_at,
        "domain": domain,
        "accessToken": composite_access_token(uid, &material.token),
        "converted": true,
        "account": {
            "id": uid,
            "uid": uid,
            "label": nickname,
            "nickname": nickname,
            "enterpriseId": enterprise_id,
            "enterpriseName": enterprise_name,
            "pluginEnabled": true,
            "lastLogin": true,
        },
        "auth": {
            "accessToken": material.token,
            "refreshToken": refresh,
            "tokenType": "Bearer",
            "domain": domain,
            "expiresAt": expires_at,
            "expiresIn": expires_at,
            "refreshExpiresIn": 0,
            "refreshExpiresAt": 0,
            "lastRefreshTime": chrono::Utc::now().timestamp_millis(),
        }
    });
    serde_json::to_string(&value).map_err(|err| {
        UsageError::Other(format!("序列化 {} 登录态失败：{err}", profile.display_name))
    })
}

/// Cockpit always formats `{uid}+{token}`. An empty uid would write `+token`
/// and the import splitter would keep the plus. Omit it.
fn composite_access_token(uid: &str, token: &str) -> String {
    if uid.is_empty() {
        token.to_string()
    } else {
        format!("{uid}+{token}")
    }
}

fn commit(profile: &Profile, path: &Path, prepared: &AuthWrite) -> UsageResult<PathBuf> {
    let backup = backup_db(profile, path)?;
    if let Err(error) = apply_and_verify(profile, path, prepared) {
        return Err(restore_or_combine(profile, path, &backup, error));
    }
    Ok(backup)
}

fn apply_and_verify(profile: &Profile, path: &Path, prepared: &AuthWrite) -> UsageResult<()> {
    let item_key = profile.item_key();
    vscdb::mutate_labeled_items(
        path,
        profile.display_name,
        &[(item_key.as_str(), prepared.stored.as_str())],
        &[],
    )?;
    verify_write(profile, path, prepared)
}

fn verify_write(profile: &Profile, path: &Path, prepared: &AuthWrite) -> UsageResult<()> {
    if readback_forced_failure() {
        return Err(readback_error(profile));
    }
    let raw = vscdb::read_item_string(path, &profile.item_key())?;
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Err(readback_error(profile));
    };
    match decrypt_stored(profile, &raw) {
        Ok(plain) if plain == prepared.plain => Ok(()),
        _ => Err(readback_error(profile)),
    }
}

fn clear_auth(profile: &Profile, path: &Path) -> UsageResult<()> {
    let item_key = profile.item_key();
    vscdb::mutate_labeled_items(path, profile.display_name, &[], &[item_key.as_str()])?;
    let raw = vscdb::read_item_string(path, &item_key)?;
    if raw.is_some_and(|value| !value.trim().is_empty()) {
        return Err(readback_error(profile));
    }
    if path.is_file() {
        Ok(())
    } else {
        Err(UsageError::Other(format!(
            "{} 清键后 state.vscdb 不见了，切换未生效",
            profile.display_name
        )))
    }
}

fn backup_db(profile: &Profile, path: &Path) -> UsageResult<PathBuf> {
    let backup = create_rolling_backup(path).map_err(|err| {
        UsageError::Other(format!(
            "备份 {} state.vscdb 失败：{err}",
            profile.display_name
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600));
    }
    Ok(backup)
}

fn restore_backup(profile: &Profile, live: &Path, backup: &Path) -> UsageResult<()> {
    std::fs::copy(backup, live).map_err(|err| {
        UsageError::Other(format!(
            "回滚 {} state.vscdb 失败：{err}（备份 {}）",
            profile.display_name,
            backup.display()
        ))
    })?;
    Ok(())
}

fn restore_or_combine(
    profile: &Profile,
    live: &Path,
    backup: &Path,
    error: UsageError,
) -> UsageError {
    match restore_backup(profile, live, backup) {
        Ok(()) => error,
        Err(restore) => UsageError::Other(format!("{error}；{restore}")),
    }
}

fn success_outcome(profile: &Profile, path: &Path, backup: &Path) -> SwitchOutcome {
    SwitchOutcome {
        tool_id: profile.catalog_id.to_string(),
        config_path: path.display().to_string(),
        backup_path: Some(backup.display().to_string()),
        keychain_updated: false,
        link_mode: None,
        success: true,
        error: None,
    }
}

fn failed(profile: &Profile, error: UsageError) -> SwitchOutcome {
    SwitchOutcome::fail(
        profile.catalog_id,
        &display_path(profile),
        error.to_string(),
    )
}

fn readback_error(profile: &Profile) -> UsageError {
    UsageError::Other(format!(
        "{} state.vscdb 回读校验失败，切换未生效",
        profile.display_name
    ))
}

fn readback_forced_failure() -> bool {
    #[cfg(test)]
    {
        std::env::var_os(READBACK_FAIL_ENV).is_some()
    }
    #[cfg(not(test))]
    {
        false
    }
}

fn read_live(profile: &Profile, path: &Path) -> UsageResult<Option<LiveAuth>> {
    if !path.is_file() {
        return Ok(None);
    }
    let raw = vscdb::read_item_string(path, &profile.item_key())?;
    match open_stored(profile, raw.as_deref()) {
        Opened::Absent => Ok(None),
        Opened::Locked => Ok(Some(LiveAuth::locked())),
        Opened::Plain(text) => {
            let live = parse_plain(&text);
            if live.token.is_none() {
                Ok(None)
            } else {
                Ok(Some(live))
            }
        }
    }
}

enum Opened {
    Absent,
    Plain(String),
    Locked,
}

fn open_stored(profile: &Profile, raw: Option<&str>) -> Opened {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Opened::Absent;
    };
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        if is_buffer(&value) {
            return decrypt_opened(profile, &secret_base64(raw));
        }
        if let Some(text) = value.as_str() {
            if looks_like_safe_storage(text) {
                return decrypt_opened(profile, text.trim());
            }
            return Opened::Plain(text.to_string());
        }
        return Opened::Plain(raw.to_string());
    }
    if looks_like_safe_storage(raw) {
        return decrypt_opened(profile, raw);
    }
    Opened::Plain(raw.to_string())
}

fn decrypt_opened(profile: &Profile, encoded: &str) -> Opened {
    match decrypt_stored(profile, encoded) {
        Ok(plain) => Opened::Plain(plain),
        Err(_) => Opened::Locked,
    }
}

fn parse_plain(text: &str) -> LiveAuth {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return LiveAuth {
            token: None,
            uid: None,
            refresh: None,
            expires_at: None,
            locked: false,
        };
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return match value {
            Value::String(text) => from_raw_token(&text),
            Value::Object(_) => parse_object(&value),
            _ => from_raw_token(trimmed),
        };
    }
    from_raw_token(trimmed)
}

fn parse_object(value: &Value) -> LiveAuth {
    let (uid_from_token, token) =
        match pick_string(value, &["accessToken", "access_token", "token"]) {
            Some(raw) => {
                let (uid, token) = split_uid_token(&raw);
                (uid, nonempty(Some(token)))
            }
            None => (None, None),
        };
    let uid = uid_from_token.or_else(|| pick_string(value, &["uid", "userId", "user_id"]));
    LiveAuth {
        token,
        uid: uid.filter(|value| !value.is_empty()),
        refresh: pick_string(value, &["refreshToken", "refresh_token"]),
        expires_at: pick_i64(value, &["expiresAt", "expires_at"]).filter(|value| *value > 0),
        locked: false,
    }
}

fn from_raw_token(raw: &str) -> LiveAuth {
    let (uid, token) = split_uid_token(raw);
    LiveAuth {
        token: nonempty(Some(token)),
        uid,
        refresh: None,
        expires_at: None,
        locked: false,
    }
}

fn matching_subscription(profile: &Profile, live: &LiveAuth) -> UsageResult<Option<Subscription>> {
    if live.locked {
        return Ok(None);
    }
    let subscriptions = storage::list_subscriptions()?;
    if let Some(subscription) = subscriptions.iter().find(|subscription| {
        subscription.catalog_id == profile.catalog_id && token_matches(subscription, live)
    }) {
        return Ok(Some(subscription.clone()));
    }
    Ok(subscriptions.into_iter().find(|subscription| {
        subscription.catalog_id == profile.catalog_id && uid_matches(subscription, live)
    }))
}

fn same_account(subscription: &Subscription, live: &LiveAuth) -> bool {
    !live.locked && (token_matches(subscription, live) || uid_matches(subscription, live))
}

fn token_matches(subscription: &Subscription, live: &LiveAuth) -> bool {
    match (
        normalized_access_token(subscription).as_deref(),
        live.token.as_deref(),
    ) {
        (Some(stored), Some(live_token)) => stored == live_token,
        _ => false,
    }
}

fn uid_matches(subscription: &Subscription, live: &LiveAuth) -> bool {
    match (stored_uid(subscription).as_deref(), live.uid.as_deref()) {
        (Some(stored), Some(live_uid)) => !stored.is_empty() && stored == live_uid,
        _ => false,
    }
}

fn normalized_access_token(subscription: &Subscription) -> Option<String> {
    let raw = secret_text(&subscription.access_token_encrypted)?;
    nonempty(Some(split_uid_token(&raw).1))
}

fn stored_uid(subscription: &Subscription) -> Option<String> {
    if let Some(uid) = nonempty(subscription.oauth_account_id.clone()) {
        return Some(uid);
    }
    secret_text(&subscription.access_token_encrypted).and_then(|raw| split_uid_token(&raw).0)
}

fn absorb(subscription: &Subscription, live: &LiveAuth) -> Subscription {
    if live.locked {
        return subscription.clone();
    }
    let mut updated = subscription.clone();
    assign_secret(&mut updated.access_token_encrypted, live.token.as_deref());
    assign_secret(
        &mut updated.refresh_token_encrypted,
        live.refresh.as_deref(),
    );
    if let Some(expires) = live.expires_at.filter(|value| *value > 0)
        && updated.access_token_expires_at != Some(expires)
    {
        updated.access_token_expires_at = Some(expires);
    }
    if let Some(uid) = live.uid.clone().filter(|uid| !uid.is_empty())
        && live.token.as_deref() != Some(uid.as_str())
        && updated.oauth_account_id.as_deref() != Some(uid.as_str())
    {
        updated.oauth_account_id = Some(uid);
    }
    updated
}

fn assign_secret(slot: &mut Option<String>, plain: Option<&str>) {
    let Some(plain) = plain.filter(|value| !value.is_empty()) else {
        return;
    };
    let current = slot.as_deref().map(crypto::decrypt).unwrap_or_default();
    if current != plain {
        *slot = Some(crypto::encrypt(plain));
    }
}

fn credentials_changed(updated: &Subscription, original: &Subscription) -> bool {
    updated.access_token_encrypted != original.access_token_encrypted
        || updated.refresh_token_encrypted != original.refresh_token_encrypted
        || updated.access_token_expires_at != original.access_token_expires_at
        || updated.oauth_account_id != original.oauth_account_id
}

fn nickname_of(
    profile: &Profile,
    subscription: &Subscription,
    uid: Option<&str>,
) -> Option<String> {
    let name = subscription.display_name.trim();
    if name.is_empty()
        || looks_like_email(name)
        || name.eq_ignore_ascii_case(profile.display_name)
        || name == profile.catalog_id
        || uid.is_some_and(|uid| name == uid)
    {
        None
    } else {
        Some(name.to_string())
    }
}

fn enterprise_of(subscription: &Subscription) -> (Option<String>, Option<String>, Option<String>) {
    let plain = subscription
        .provider_state_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .unwrap_or_default();
    let Ok(value) = serde_json::from_str::<Value>(plain.trim()) else {
        return (None, None, None);
    };
    (
        pick_string(&value, &["enterpriseId", "enterprise_id"]),
        pick_string(&value, &["enterpriseName", "enterprise_name"]),
        pick_string(&value, &["domain"]),
    )
}

fn secret_text(slot: &Option<String>) -> Option<String> {
    nonempty(slot.as_deref().map(crypto::decrypt))
}

fn split_uid_token(raw: &str) -> (Option<String>, String) {
    let trimmed = raw.trim();
    if let Some((prefix, suffix)) = trimmed.split_once('+') {
        let prefix = prefix.trim();
        let suffix = suffix.trim();
        if !prefix.is_empty()
            && !suffix.is_empty()
            && prefix.len() <= 128
            && !prefix.contains('.')
            && !prefix.contains(' ')
        {
            return (Some(prefix.to_string()), suffix.to_string());
        }
    }
    (None, trimmed.to_string())
}

fn pick_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = object_string(value, key) {
            return Some(text);
        }
        for nest in ["data", "auth", "account", "user"] {
            if let Some(child) = value.get(nest)
                && let Some(text) = object_string(child, key)
            {
                return Some(text);
            }
        }
    }
    None
}

fn pick_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(json_i64) {
            return Some(number);
        }
        for nest in ["auth", "data", "account"] {
            if let Some(child) = value.get(nest)
                && let Some(number) = child.get(*key).and_then(json_i64)
            {
                return Some(number);
            }
        }
    }
    None
}

fn object_string(value: &Value, key: &str) -> Option<String> {
    let item = value
        .as_object()?
        .iter()
        .find_map(|(name, item)| name.eq_ignore_ascii_case(key).then_some(item))?;
    scalar_string(item)
}

fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => nonempty(Some(text.clone())),
        Value::Number(number) => nonempty(Some(number.to_string())),
        _ => None,
    }
}

fn json_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn looks_like_email(value: &str) -> bool {
    let value = value.trim();
    value.len() > 3 && value.contains('@') && !value.contains(char::is_whitespace)
}

fn encrypt_with(profile: &Profile, key: &KeyMaterial, plain: &str) -> UsageResult<String> {
    safe_storage::encrypt_secret(key, plain.as_bytes()).map_err(|err| {
        UsageError::Other(format!(
            "{} secret:// 加密失败：{err}",
            profile.display_name
        ))
    })
}

fn safe_storage_key(profile: &Profile) -> UsageResult<KeyMaterial> {
    let password = std::env::var(profile.password_env).map_err(|_| missing_password(profile))?;
    if password.trim().is_empty() {
        return Err(missing_password(profile));
    }
    Ok(host_key(&password))
}

fn missing_password(profile: &Profile) -> UsageError {
    UsageError::Other(format!(
        "{} Safe Storage 口令未注入（不会读取系统钥匙串），切换未生效",
        profile.display_name
    ))
}

fn host_key(password: &str) -> KeyMaterial {
    #[cfg(target_os = "macos")]
    {
        KeyMaterial::macos_v10(password)
    }
    #[cfg(target_os = "linux")]
    {
        KeyMaterial::linux_v10(password)
    }
    #[cfg(target_os = "windows")]
    {
        KeyMaterial::OsCryptKey(windows_os_crypt_key(password))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = password;
        KeyMaterial::linux_v10(password)
    }
}

/// Windows Safe Storage wants the DPAPI-unwrapped os_crypt key. This slice does
/// not call DPAPI; the injected password is hashed so tests can round-trip.
#[cfg(target_os = "windows")]
fn windows_os_crypt_key(password: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(password.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

fn decrypt_stored(profile: &Profile, stored: &str) -> UsageResult<String> {
    let key = safe_storage_key(profile)?;
    let encoded = secret_base64(stored);
    let bytes = safe_storage::decrypt_secret(&key, &encoded).map_err(|err| {
        UsageError::Other(format!(
            "{} secret:// 解密失败：{err}",
            profile.display_name
        ))
    })?;
    String::from_utf8(bytes)
        .map_err(|_| UsageError::Other(format!("{} secret:// 不是 UTF-8", profile.display_name)))
}

fn is_buffer(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("Buffer") && value.get("data").is_some()
}

fn looks_like_safe_storage(encoded: &str) -> bool {
    let Ok(raw) =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded.trim())
    else {
        return false;
    };
    raw.starts_with(b"v10") || raw.starts_with(b"v11")
}

/// Official rows are standard base64. Cockpit also writes
/// `{"type":"Buffer","data":[...]}`.
fn secret_base64(stored: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(stored.trim()) else {
        return stored.trim().to_string();
    };
    if !is_buffer(&value) {
        return stored.trim().to_string();
    }
    match value.get("data") {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Array(bytes)) => {
            let raw: Vec<u8> = bytes
                .iter()
                .filter_map(|item| item.as_u64().map(|byte| byte as u8))
                .collect();
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, raw)
        }
        _ => stored.trim().to_string(),
    }
}

#[cfg(test)]
#[path = "codebuddy_tests.rs"]
mod tests;

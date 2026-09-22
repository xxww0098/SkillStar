//! Trae, TRAE SOLO, Trae CN, and TRAE SOLO CN account switching.
//!
//! One adapter, four catalogs. Product differences live on [`TraePlatformKind`].
//! The write set is iCube auth keys only, encrypted with `byte_crypto` and
//! stored as standard base64:
//! `iCubeAuthInfo://icube.cloudide` (or the file's existing user-auth key when
//! that default is absent) and, when a device id is already known,
//! `iCubeAuthInfo://icube-dc:<id>`. `iCubeServerData`, `iCubeEntitlementInfo`,
//! `iCubeAuthInfo://usertag`, and every other key stay put.
//!
//! Activate backs up the whole file, replaces it atomically, decrypts the
//! written keys, and pins only when they match. A failed read-back restores
//! the backup. A write that hits a busy file is retried, then reported; the
//! file is never updated key by key. Restarting the official app is not
//! verified here.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};
use skillstar_models::tool_sync::create_rolling_backup;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::trae_platform::TraePlatformKind;
use skillstar_usage::{UsageError, UsageResult, storage, tool_paths};

use super::ide::IdeCredentialAdapter;
use super::{CliAccountState, SwitchOutcome};

#[path = "trae_store.rs"]
mod store;

use store::{
    absorb, choose_host, choose_text, credentials_changed, device_key_name, encode_value,
    epoch_seconds, expiry_iso, identity_names, live_from_root, matching_subscription, nonempty,
    opened_value, provider_of, put_access, put_refresh, put_text, put_user, read_live,
    removal_keys, same_account, secret_text, user_auth_key,
};

const AUTH_PREFIX: &str = "iCubeAuthInfo://";
const DEVICE_PREFIX: &str = "iCubeAuthInfo://icube-dc:";
const USERTAG_KEY: &str = "iCubeAuthInfo://usertag";
const DEFAULT_AUTH_KEY: &str = "iCubeAuthInfo://icube.cloudide";

#[cfg(test)]
const READBACK_FAIL_ENV: &str = "SKILLSTAR_TRAE_READBACK_FAIL";

pub(super) struct Adapter {
    kind: TraePlatformKind,
}

impl IdeCredentialAdapter for Adapter {
    fn catalog_id(&self) -> &'static str {
        self.kind.catalog_id()
    }

    fn available(&self) -> bool {
        tool_paths::trae_storage_path_for(self.kind).is_some()
    }

    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
        activate(self.kind, sub_id)
    }

    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome> {
        sync(self.kind, sub)
    }

    fn reconcile(&self) -> UsageResult<Option<CliAccountState>> {
        if !self.available() {
            return Ok(None);
        }
        reconcile(self.kind).map(Some)
    }

    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()> {
        adopt_active_session(self.kind, sub)
    }

    fn forget(&self, sub_id: &str) -> UsageResult<()> {
        forget_account(self.kind, sub_id)
    }
}

pub(super) static TRAE: Adapter = Adapter {
    kind: TraePlatformKind::Trae,
};
pub(super) static SOLO: Adapter = Adapter {
    kind: TraePlatformKind::TraeSolo,
};
pub(super) static CN: Adapter = Adapter {
    kind: TraePlatformKind::TraeCn,
};
pub(super) static SOLO_CN: Adapter = Adapter {
    kind: TraePlatformKind::TraeSoloCn,
};

fn activate(
    kind: TraePlatformKind,
    subscription_id: &str,
) -> UsageResult<(Subscription, SwitchOutcome)> {
    let subscription = storage::get_subscription(subscription_id)?;
    let path = display_path(kind);
    match write_subscription(kind, &subscription) {
        Ok(outcome) => {
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            Ok((subscription, outcome))
        }
        Err(error) => Ok((subscription, failed(kind, &path, error))),
    }
}

fn sync(kind: TraePlatformKind, subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    write_subscription(kind, subscription)
        .or_else(|error| Ok(failed(kind, &display_path(kind), error)))
}

fn reconcile(kind: TraePlatformKind) -> UsageResult<CliAccountState> {
    let Ok(path) = live_path(kind) else {
        return Ok(CliAccountState::Missing);
    };
    let Some(live) = read_live(kind, &path)? else {
        return Ok(CliAccountState::Missing);
    };
    if live.locked {
        return Ok(CliAccountState::Diverged);
    }
    let Some(subscription) = matching_subscription(kind, &live)? else {
        return Ok(CliAccountState::Diverged);
    };
    let updated = absorb(kind, &subscription, &live);
    if credentials_changed(&updated, &subscription) {
        storage::patch_oauth_credentials(&updated)?;
    }
    Ok(CliAccountState::LinkedTo {
        subscription_id: subscription.id,
    })
}

fn adopt_active_session(
    kind: TraePlatformKind,
    subscription: &mut Subscription,
) -> UsageResult<()> {
    if subscription.catalog_id != kind.catalog_id() {
        return Ok(());
    }
    let Ok(path) = live_path(kind) else {
        return Ok(());
    };
    let Some(live) = read_live(kind, &path)? else {
        return Ok(());
    };
    if !same_account(subscription, &live) {
        return Ok(());
    }
    let updated = absorb(kind, subscription, &live);
    if credentials_changed(&updated, subscription) {
        *subscription = storage::patch_oauth_credentials(&updated)?;
    }
    Ok(())
}

fn forget_account(kind: TraePlatformKind, subscription_id: &str) -> UsageResult<()> {
    let Ok(subscription) = storage::get_subscription(subscription_id) else {
        return Ok(());
    };
    if subscription.catalog_id != kind.catalog_id() {
        return Ok(());
    }
    let Ok(path) = live_path(kind) else {
        return Ok(());
    };
    if !path.is_file() {
        return Ok(());
    }
    let root = read_root(kind, &path)?;
    let Some(live) = live_from_root(&root) else {
        return Ok(());
    };
    if !same_account(&subscription, &live) {
        return Ok(());
    }
    let keys = removal_keys(&root, &subscription);
    if keys.is_empty() {
        return Ok(());
    }
    let backup = backup_file(kind, &path)?;
    let mut next = root;
    for key in &keys {
        next.remove(key);
    }
    let bytes = to_pretty(kind, &Value::Object(next))?;
    if let Err(error) = write_and_confirm_removed(kind, &path, &bytes, &keys) {
        return Err(restore_or_combine(kind, &path, &backup, error));
    }
    Ok(())
}

fn write_subscription(
    kind: TraePlatformKind,
    subscription: &Subscription,
) -> UsageResult<SwitchOutcome> {
    if subscription.catalog_id != kind.catalog_id() {
        return Err(UsageError::Other(format!(
            "{} 切换收到了其它 catalog 的订阅",
            kind.display_name()
        )));
    }
    ensure_token(kind, subscription)?;
    let path = live_path(kind)?;
    require_file(kind, &path)?;
    let root = read_root(kind, &path)?;
    let existing = root.get(&user_auth_key(&root)).and_then(opened_value);
    let mut material = material_of(kind, subscription, existing.as_ref())?;
    bind_slots(&root, &mut material);
    let expected = expectation_of(kind, &material, existing.as_ref());
    let mut next = root;
    next.insert(
        expected.auth_key.clone(),
        encode_value(kind, &expected.auth)?,
    );
    if let Some(device_key) = expected.device_key.clone() {
        let device = expected
            .device
            .clone()
            .ok_or_else(|| UsageError::Other(format!("{} 设备密钥为空", kind.display_name())))?;
        next.insert(device_key, encode_value(kind, &device)?);
    }
    let bytes = to_pretty(kind, &Value::Object(next))?;
    let backup = backup_file(kind, &path)?;
    if let Err(error) = write_and_verify(kind, &path, &bytes, &expected) {
        return Err(restore_or_combine(kind, &path, &backup, error));
    }
    tighten(&path);
    Ok(success_outcome(kind, &path, &backup))
}

struct Expected {
    auth_key: String,
    auth: Value,
    device_key: Option<String>,
    device: Option<Value>,
}

fn expectation_of(
    kind: TraePlatformKind,
    material: &Material,
    existing: Option<&Value>,
) -> Expected {
    Expected {
        auth_key: material.auth_key.clone(),
        auth: auth_document(kind, material, existing),
        device_key: material.device_key.clone(),
        device: material.device.as_ref().map(|(private_pem, public_pem)| {
            serde_json::json!({
                "privateKeyPEM": private_pem,
                "publicKeyPEM": public_pem,
            })
        }),
    }
}

struct Material {
    auth_key: String,
    access: String,
    refresh: Option<String>,
    user_id: Option<String>,
    email: Option<String>,
    username: Option<String>,
    expires_at: Option<i64>,
    login_region: Option<String>,
    client_id: String,
    login_host: String,
    auth_domain: String,
    device: Option<(String, String)>,
    /// Raw device id from provider state. [`bind_slots`] turns it into a storage key.
    device_id: Option<String>,
    device_key: Option<String>,
}

fn material_of(
    kind: TraePlatformKind,
    subscription: &Subscription,
    existing: Option<&Value>,
) -> UsageResult<Material> {
    ensure_token(kind, subscription)?;
    let provider = provider_of(subscription);
    let (email, username) = identity_names(kind, subscription);
    let device = provider.device.clone();
    Ok(Material {
        auth_key: String::new(),
        access: secret_text(&subscription.access_token_encrypted).unwrap_or_default(),
        refresh: secret_text(&subscription.refresh_token_encrypted),
        user_id: nonempty(subscription.oauth_account_id.clone()),
        email,
        username,
        expires_at: subscription.access_token_expires_at.and_then(epoch_seconds),
        login_region: nonempty(subscription.oauth_region.clone()),
        client_id: choose_text(
            provider.client_id.as_deref(),
            existing,
            &[&["authClientId"], &["clientId"], &["ClientID"]],
            kind.auth_client_id(),
        ),
        login_host: choose_host(kind, &provider, existing),
        auth_domain: choose_text(
            provider.auth_domain.as_deref(),
            existing,
            &[&["authDomain"]],
            kind.auth_domain(),
        ),
        device_id: provider.device_id.clone(),
        device_key: None,
        device,
    })
}

fn auth_document(kind: TraePlatformKind, material: &Material, existing: Option<&Value>) -> Value {
    let mut obj = existing
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    put_access(&mut obj, &material.access);
    put_refresh(&mut obj, material.refresh.as_deref());
    put_user(&mut obj, material.user_id.as_deref());
    put_text(&mut obj, "email", material.email.as_deref());
    put_text(&mut obj, "loginRegion", material.login_region.as_deref());
    obj.insert(
        "platformId".to_string(),
        Value::String(kind.provider_key().to_string()),
    );
    obj.insert(
        "platformName".to_string(),
        Value::String(kind.display_name().to_string()),
    );
    obj.insert(
        "authClientId".to_string(),
        Value::String(material.client_id.clone()),
    );
    obj.insert(
        "clientId".to_string(),
        Value::String(material.client_id.clone()),
    );
    obj.insert(
        "authDomain".to_string(),
        Value::String(material.auth_domain.clone()),
    );
    obj.insert(
        "host".to_string(),
        Value::String(material.login_host.clone()),
    );
    obj.insert(
        "loginHost".to_string(),
        Value::String(material.login_host.clone()),
    );
    obj.remove("expiredAt");
    obj.remove("expiresAt");
    obj.remove("refreshExpiredAt");
    if let Some(seconds) = material.expires_at {
        obj.insert(
            "expiresAt".to_string(),
            Value::Number(serde_json::Number::from(seconds)),
        );
        if let Some(iso) = expiry_iso(seconds) {
            obj.insert("expiredAt".to_string(), Value::String(iso));
        }
    }
    match &material.device {
        Some((private_pem, public_pem)) => {
            obj.insert(
                "deviceKeyPair".to_string(),
                serde_json::json!({
                    "privateKeyPEM": private_pem,
                    "publicKeyPEM": public_pem,
                }),
            );
        }
        None => {
            obj.remove("deviceKeyPair");
        }
    }
    merge_account(
        &mut obj,
        material.email.as_deref(),
        material.username.as_deref(),
        material.user_id.as_deref(),
    );
    Value::Object(obj)
}

fn merge_account(
    obj: &mut Map<String, Value>,
    email: Option<&str>,
    username: Option<&str>,
    uid: Option<&str>,
) {
    let mut account = obj
        .get("account")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    put_text(&mut account, "email", email);
    put_text(&mut account, "username", username);
    put_text(&mut account, "uid", uid);
    if account.is_empty() {
        obj.remove("account");
    } else {
        obj.insert("account".to_string(), Value::Object(account));
    }
}

/// Device-key slot is resolved from the file, not from [`material_of`], because
/// the subscription does not learn the id until the root is in hand.
fn bind_slots(root: &Map<String, Value>, material: &mut Material) {
    material.auth_key = user_auth_key(root);
    if material.device.is_none() {
        material.device_key = None;
        return;
    }
    material.device_key = device_key_name(root, material.device_id.as_deref());
}

fn write_and_verify(
    kind: TraePlatformKind,
    path: &Path,
    bytes: &[u8],
    expected: &Expected,
) -> UsageResult<()> {
    replace_file(kind, path, bytes)?;
    if readback_forced_failure() {
        return Err(readback_error(kind));
    }
    let root = read_root(kind, path)?;
    let auth = root
        .get(&expected.auth_key)
        .and_then(opened_value)
        .ok_or_else(|| readback_error(kind))?;
    if auth != expected.auth {
        return Err(readback_error(kind));
    }
    if let Some(device_key) = &expected.device_key {
        let device = root
            .get(device_key)
            .and_then(opened_value)
            .ok_or_else(|| readback_error(kind))?;
        if Some(&device) != expected.device.as_ref() {
            return Err(readback_error(kind));
        }
    }
    Ok(())
}

fn write_and_confirm_removed(
    kind: TraePlatformKind,
    path: &Path,
    bytes: &[u8],
    keys: &[String],
) -> UsageResult<()> {
    replace_file(kind, path, bytes)?;
    if readback_forced_failure() {
        return Err(readback_error(kind));
    }
    let root = read_root(kind, path)?;
    if keys.iter().any(|key| root.contains_key(key)) || !path.is_file() {
        return Err(readback_error(kind));
    }
    Ok(())
}

fn replace_file(kind: TraePlatformKind, path: &Path, bytes: &[u8]) -> UsageResult<()> {
    let delays_ms = [0u64, 50, 100, 200];
    let mut last = std::io::Error::other("storage.json write failed");
    for (attempt, delay_ms) in delays_ms.iter().copied().enumerate() {
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
        match skillstar_core::infra::fs_ops::atomic_write(path, bytes) {
            Ok(()) => return Ok(()),
            Err(err) => {
                let again = attempt + 1 < delays_ms.len() && retryable(&err);
                if !again {
                    return Err(write_error(kind, err));
                }
                last = err;
            }
        }
    }
    Err(write_error(kind, last))
}

fn retryable(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::ResourceBusy
    ) || matches!(err.raw_os_error(), Some(32 | 33))
}

fn live_path(kind: TraePlatformKind) -> UsageResult<PathBuf> {
    tool_paths::trae_storage_path_for(kind)
        .ok_or_else(|| UsageError::Other(format!("无法解析 {} 数据目录", kind.display_name())))
}

fn display_path(kind: TraePlatformKind) -> PathBuf {
    live_path(kind)
        .unwrap_or_else(|_| PathBuf::from(format!("{} storage.json", kind.display_name())))
}

fn require_file(kind: TraePlatformKind, path: &Path) -> UsageResult<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(UsageError::Other(format!(
            "未找到 {} storage.json：{}。请先启动 {} 并完成一次登录",
            kind.display_name(),
            path.display(),
            kind.display_name()
        )))
    }
}

fn read_root(kind: TraePlatformKind, path: &Path) -> UsageResult<Map<String, Value>> {
    let text = std::fs::read_to_string(path).map_err(|err| {
        UsageError::Other(format!(
            "读取 {} storage.json 失败：{err}",
            kind.display_name()
        ))
    })?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(UsageError::Other(format!(
            "{} storage.json 不是 JSON 对象，切换未生效",
            kind.display_name()
        ))),
    }
}

fn to_pretty(kind: TraePlatformKind, value: &Value) -> UsageResult<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|err| {
        UsageError::Other(format!(
            "序列化 {} storage.json 失败：{err}",
            kind.display_name()
        ))
    })
}

fn backup_file(kind: TraePlatformKind, path: &Path) -> UsageResult<PathBuf> {
    let backup = create_rolling_backup(path).map_err(|err| {
        UsageError::Other(format!(
            "备份 {} storage.json 失败：{err}",
            kind.display_name()
        ))
    })?;
    tighten(&backup);
    Ok(backup)
}

fn restore_backup(kind: TraePlatformKind, live: &Path, backup: &Path) -> UsageResult<()> {
    if let Some(parent) = live.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            UsageError::Other(format!(
                "回滚 {} storage.json 失败：{err}（备份 {}）",
                kind.display_name(),
                backup.display()
            ))
        })?;
    }
    std::fs::copy(backup, live).map_err(|err| {
        UsageError::Other(format!(
            "回滚 {} storage.json 失败：{err}（备份 {}）",
            kind.display_name(),
            backup.display()
        ))
    })?;
    tighten(live);
    Ok(())
}

fn restore_or_combine(
    kind: TraePlatformKind,
    live: &Path,
    backup: &Path,
    error: UsageError,
) -> UsageError {
    match restore_backup(kind, live, backup) {
        Ok(()) => error,
        Err(restore) => UsageError::Other(format!("{error}；{restore}")),
    }
}

fn success_outcome(kind: TraePlatformKind, path: &Path, backup: &Path) -> SwitchOutcome {
    SwitchOutcome {
        tool_id: kind.catalog_id().to_string(),
        config_path: path.display().to_string(),
        backup_path: Some(backup.display().to_string()),
        keychain_updated: false,
        link_mode: None,
        success: true,
        error: None,
    }
}

fn failed(kind: TraePlatformKind, path: &Path, error: UsageError) -> SwitchOutcome {
    SwitchOutcome::fail(kind.catalog_id(), path, error.to_string())
}

fn readback_error(kind: TraePlatformKind) -> UsageError {
    UsageError::Other(format!(
        "{} storage.json 回读校验失败，切换未生效",
        kind.display_name()
    ))
}

fn write_error(kind: TraePlatformKind, err: std::io::Error) -> UsageError {
    UsageError::Other(format!(
        "写入 {} storage.json 失败：{err}。文件可能正被 {} 占用，切换未生效",
        kind.display_name(),
        kind.display_name()
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

fn tighten(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn ensure_token(kind: TraePlatformKind, subscription: &Subscription) -> UsageResult<()> {
    if secret_text(&subscription.access_token_encrypted).is_none()
        && secret_text(&subscription.refresh_token_encrypted).is_none()
    {
        return Err(UsageError::Other(format!(
            "{} 账号缺少令牌，切换未生效",
            kind.display_name()
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "trae_tests.rs"]
mod tests;

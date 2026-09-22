//! Qoder account switching through `state.vscdb`.
//!
//! Backup, one ItemTable transaction, read-back, then pin. Only the three
//! `aicoding.auth` rows are written or cleared. `secret://` values use
//! [`skillstar_usage::tool_store::safe_storage`] and an injected password
//! (`SKILLSTAR_QODER_SAFE_STORAGE_PASSWORD`). This path does not read the
//! system keychain and does not create or copy a database. Restarting the
//! official Qoder app is not verified here.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use skillstar_models::tool_sync::create_rolling_backup;
use skillstar_usage::crypto;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::tool_store::safe_storage::{self, KeyMaterial};
use skillstar_usage::{UsageError, UsageResult, storage, tool_paths, vscdb};

use super::ide::IdeCredentialAdapter;
use super::{CliAccountState, SwitchOutcome};

pub(super) const CATALOG_ID: &str = "qoder";

const PRODUCT: &str = "Qoder";
pub(super) const USER_INFO_PLAIN: &str = "aicoding.auth.userInfo";
pub(super) const USER_PLAN_PLAIN: &str = "aicoding.auth.userPlan";
pub(super) const CREDIT_PLAIN: &str = "aicoding.auth.creditUsage";
pub(super) const USER_INFO_SECRET: &str = "secret://aicoding.auth.userInfo";
pub(super) const USER_PLAN_SECRET: &str = "secret://aicoding.auth.userPlan";
pub(super) const CREDIT_SECRET: &str = "secret://aicoding.auth.creditUsage";
pub(super) const SAFE_STORAGE_PASSWORD_ENV: &str = "SKILLSTAR_QODER_SAFE_STORAGE_PASSWORD";

#[cfg(test)]
pub(super) const READBACK_FAIL_ENV: &str = "SKILLSTAR_QODER_READBACK_FAIL";

const EMPTY_JSON: &str = "{}";
const COLUMNS: &[&str] = &[
    USER_INFO_PLAIN,
    USER_INFO_SECRET,
    USER_PLAN_PLAIN,
    USER_PLAN_SECRET,
    CREDIT_PLAIN,
    CREDIT_SECRET,
];
const MACHINE_KEYS: &[&str] = &[
    "machineToken",
    "machineId",
    "machineType",
    "machineCode",
    "hostname",
    "os",
    "cosy_version",
];

pub(super) struct Adapter;

impl IdeCredentialAdapter for Adapter {
    fn catalog_id(&self) -> &'static str {
        CATALOG_ID
    }

    fn available(&self) -> bool {
        // Addressable store, not "a database is present". A missing file stays
        // in the reconcile map as `Missing`.
        tool_paths::qoder_state_db_path().is_some()
    }

    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
        activate(sub_id)
    }

    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome> {
        sync(sub)
    }

    fn reconcile(&self) -> UsageResult<Option<CliAccountState>> {
        if !self.available() {
            return Ok(None);
        }
        reconcile().map(Some)
    }

    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()> {
        adopt_active_session(sub)
    }

    fn forget(&self, sub_id: &str) -> UsageResult<()> {
        forget_account(sub_id)
    }
}

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
    let path = live_path()?;
    let Some(live) = read_live(&path)? else {
        return Ok(CliAccountState::Missing);
    };
    if live.locked || (live.email.is_none() && live.token.is_some()) {
        return Ok(CliAccountState::Diverged);
    }
    let Some(subscription) = matching_subscription(&live)? else {
        return if live.email.is_some() {
            Ok(CliAccountState::Diverged)
        } else {
            Ok(CliAccountState::Missing)
        };
    };
    let updated = absorb(&subscription, &live);
    if credentials_changed(&updated, &subscription) {
        storage::patch_oauth_credentials(&updated)?;
    }
    Ok(CliAccountState::LinkedTo {
        subscription_id: subscription.id,
    })
}

pub(super) fn adopt_active_session(subscription: &mut Subscription) -> UsageResult<()> {
    if subscription.catalog_id != CATALOG_ID {
        return Ok(());
    }
    let Ok(path) = live_path() else {
        return Ok(());
    };
    let Some(live) = read_live(&path)? else {
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

fn forget_account(subscription_id: &str) -> UsageResult<()> {
    let Ok(subscription) = storage::get_subscription(subscription_id) else {
        return Ok(());
    };
    if subscription.catalog_id != CATALOG_ID {
        return Ok(());
    }
    let Ok(path) = live_path() else {
        return Ok(());
    };
    if !path.is_file() {
        return Ok(());
    }
    let Some(live) = read_live(&path)? else {
        return Ok(());
    };
    if !same_account(&subscription, &live) {
        return Ok(());
    }
    let backup = backup_db(&path)?;
    if let Err(error) = clear_auth(&path) {
        return Err(restore_or_combine(&path, &backup, error));
    }
    Ok(())
}

fn write_subscription(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    if subscription.catalog_id != CATALOG_ID {
        return Err(UsageError::Other(
            "Qoder 切换收到了其它 catalog 的订阅".into(),
        ));
    }
    let path = live_path()?;
    require_db(&path)?;
    let prepared = prepare(&path, subscription)?;
    let backup = commit(&path, &prepared)?;
    Ok(success_outcome(&path, &backup))
}

fn live_path() -> UsageResult<PathBuf> {
    tool_paths::qoder_state_db_path()
        .ok_or_else(|| UsageError::Other("无法解析 Qoder 数据目录".into()))
}

fn display_path() -> PathBuf {
    live_path().unwrap_or_else(|_| PathBuf::from("Qoder state.vscdb"))
}

fn require_db(path: &Path) -> UsageResult<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(UsageError::Other(format!(
            "未找到 Qoder state.vscdb：{}。请先启动 Qoder 并完成一次登录",
            path.display()
        )))
    }
}

#[derive(Debug, Clone)]
struct LiveAuth {
    email: Option<String>,
    token: Option<String>,
    refresh: Option<String>,
    locked: bool,
}

impl LiveAuth {
    fn locked() -> Self {
        Self {
            email: None,
            token: None,
            refresh: None,
            locked: true,
        }
    }
}

struct Material {
    token: String,
    refresh: Option<String>,
    email: String,
    user_id: Option<String>,
    name: Option<String>,
    plan: Option<String>,
    machine: Map<String, Value>,
}

enum FieldWrite {
    Replace {
        plain: String,
        stored: String,
    },
    Keep {
        secret: Option<String>,
        plain: Option<String>,
    },
}

struct Columns {
    user_info_plain: Option<String>,
    user_info_secret: Option<String>,
    plan_plain: Option<String>,
    plan_secret: Option<String>,
    credit_plain: Option<String>,
    credit_secret: Option<String>,
}

struct AuthWrite {
    user_info_plain: String,
    user_info_stored: String,
    plan: FieldWrite,
    credit: FieldWrite,
}

fn prepare(path: &Path, subscription: &Subscription) -> UsageResult<AuthWrite> {
    let material = material_of(subscription)?;
    let key = safe_storage_key()?;
    let columns = read_columns(path)?;
    let live = live_from_columns(&columns)?;
    let same = live.as_ref().is_some_and(|live| {
        !live.locked
            && live
                .email
                .as_deref()
                .is_some_and(|email| emails_eq(email, &material.email))
    });
    let user_info_plain = user_info_json(&material)?;
    let user_info_stored = encrypt_with(&key, &user_info_plain)?;
    let plan = field_write(
        &key,
        same,
        &plan_json(material.plan.as_deref())?,
        &columns.plan_secret,
        &columns.plan_plain,
    )?;
    // Quota blobs are not stored on the subscription. Rewriting them on every
    // same-account sync would wipe whatever the IDE last wrote.
    let credit = field_write(
        &key,
        same,
        EMPTY_JSON,
        &columns.credit_secret,
        &columns.credit_plain,
    )?;
    Ok(AuthWrite {
        user_info_plain,
        user_info_stored,
        plan,
        credit,
    })
}

fn field_write(
    key: &KeyMaterial,
    same_account: bool,
    projection: &str,
    existing_secret: &Option<String>,
    existing_plain: &Option<String>,
) -> UsageResult<FieldWrite> {
    let secret = present(existing_secret);
    let plain = present(existing_plain);
    if projection == EMPTY_JSON && same_account && (secret.is_some() || plain.is_some()) {
        return Ok(FieldWrite::Keep { secret, plain });
    }
    Ok(FieldWrite::Replace {
        plain: projection.to_string(),
        stored: encrypt_with(key, projection)?,
    })
}

fn material_of(subscription: &Subscription) -> UsageResult<Material> {
    let token = secret_text(&subscription.access_token_encrypted);
    let email = email_of(subscription);
    let (Some(token), Some(email)) = (token, email) else {
        return Err(UsageError::Other(
            "Qoder 账号缺少 access_token 或 email，切换未生效".into(),
        ));
    };
    let user_id = nonempty(subscription.oauth_account_id.clone())
        .filter(|id| !looks_like_email(id) && id != &token);
    Ok(Material {
        token,
        refresh: secret_text(&subscription.refresh_token_encrypted),
        email,
        user_id,
        name: name_of(subscription),
        plan: nonempty(subscription.plan_tier.clone()),
        machine: provider_map(subscription),
    })
}

fn user_info_json(material: &Material) -> UsageResult<String> {
    let mut map = Map::new();
    map.insert("token".into(), Value::String(material.token.clone()));
    insert_some(&mut map, "refreshToken", material.refresh.as_deref());
    insert_some(&mut map, "id", material.user_id.as_deref());
    map.insert("email".into(), Value::String(material.email.clone()));
    insert_some(&mut map, "name", material.name.as_deref());
    for key in MACHINE_KEYS {
        if let Some(value) = json_string(&material.machine, &[key]) {
            map.insert((*key).to_string(), Value::String(value));
        }
    }
    serde_json::to_string(&Value::Object(map))
        .map_err(|err| UsageError::Other(format!("序列化 Qoder userInfo 失败：{err}")))
}

fn plan_json(plan: Option<&str>) -> UsageResult<String> {
    let value = match plan.map(str::trim).filter(|plan| !plan.is_empty()) {
        Some(plan) => serde_json::json!({ "plan": plan, "tier": plan }),
        None => serde_json::json!({}),
    };
    serde_json::to_string(&value)
        .map_err(|err| UsageError::Other(format!("序列化 Qoder userPlan 失败：{err}")))
}

fn commit(path: &Path, prepared: &AuthWrite) -> UsageResult<PathBuf> {
    let backup = backup_db(path)?;
    if let Err(error) = apply_and_verify(path, prepared) {
        return Err(restore_or_combine(path, &backup, error));
    }
    Ok(backup)
}

fn apply_and_verify(path: &Path, prepared: &AuthWrite) -> UsageResult<()> {
    let (upserts, deletes) = mutation_rows(prepared);
    let upsert_refs: Vec<(&str, &str)> = upserts
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let delete_refs: Vec<&str> = deletes.iter().map(String::as_str).collect();
    vscdb::mutate_labeled_items(path, PRODUCT, &upsert_refs, &delete_refs)?;
    verify_write(path, prepared)
}

fn mutation_rows(prepared: &AuthWrite) -> (Vec<(String, String)>, Vec<String>) {
    let mut upserts = vec![(
        USER_INFO_SECRET.to_string(),
        prepared.user_info_stored.clone(),
    )];
    let mut deletes = vec![USER_INFO_PLAIN.to_string()];
    push_field(
        &mut upserts,
        &mut deletes,
        USER_PLAN_SECRET,
        USER_PLAN_PLAIN,
        &prepared.plan,
    );
    push_field(
        &mut upserts,
        &mut deletes,
        CREDIT_SECRET,
        CREDIT_PLAIN,
        &prepared.credit,
    );
    (upserts, deletes)
}

fn push_field(
    upserts: &mut Vec<(String, String)>,
    deletes: &mut Vec<String>,
    secret_key: &str,
    plain_key: &str,
    field: &FieldWrite,
) {
    if let FieldWrite::Replace { stored, .. } = field {
        upserts.push((secret_key.to_string(), stored.clone()));
        deletes.push(plain_key.to_string());
    }
}

fn verify_write(path: &Path, prepared: &AuthWrite) -> UsageResult<()> {
    if readback_forced_failure() {
        return Err(readback_error());
    }
    let columns = read_columns(path)?;
    if columns.user_info_plain.is_some()
        || decrypted(columns.user_info_secret.as_deref()).as_deref()
            != Some(prepared.user_info_plain.as_str())
    {
        return Err(readback_error());
    }
    verify_field(&columns.plan_secret, &columns.plan_plain, &prepared.plan)?;
    verify_field(
        &columns.credit_secret,
        &columns.credit_plain,
        &prepared.credit,
    )?;
    Ok(())
}

fn verify_field(
    secret: &Option<String>,
    plain: &Option<String>,
    field: &FieldWrite,
) -> UsageResult<()> {
    match field {
        FieldWrite::Replace {
            plain: expected, ..
        } => {
            if plain.is_some() || decrypted(secret.as_deref()).as_deref() != Some(expected.as_str())
            {
                return Err(readback_error());
            }
        }
        FieldWrite::Keep {
            secret: expected_secret,
            plain: expected_plain,
        } => {
            if present(secret) != *expected_secret || present(plain) != *expected_plain {
                return Err(readback_error());
            }
        }
    }
    Ok(())
}

fn clear_auth(path: &Path) -> UsageResult<()> {
    vscdb::mutate_labeled_items(path, PRODUCT, &[], COLUMNS)?;
    let columns = read_columns(path)?;
    if columns.user_info_plain.is_some()
        || columns.user_info_secret.is_some()
        || columns.plan_plain.is_some()
        || columns.plan_secret.is_some()
        || columns.credit_plain.is_some()
        || columns.credit_secret.is_some()
    {
        return Err(readback_error());
    }
    if path.is_file() {
        Ok(())
    } else {
        Err(UsageError::Other(
            "Qoder 清键后 state.vscdb 不见了，切换未生效".into(),
        ))
    }
}

fn backup_db(path: &Path) -> UsageResult<PathBuf> {
    let backup = create_rolling_backup(path)
        .map_err(|err| UsageError::Other(format!("备份 Qoder state.vscdb 失败：{err}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600));
    }
    Ok(backup)
}

fn restore_backup(live: &Path, backup: &Path) -> UsageResult<()> {
    std::fs::copy(backup, live).map_err(|err| {
        UsageError::Other(format!(
            "回滚 Qoder state.vscdb 失败：{err}（备份 {}）",
            backup.display()
        ))
    })?;
    Ok(())
}

fn restore_or_combine(live: &Path, backup: &Path, error: UsageError) -> UsageError {
    match restore_backup(live, backup) {
        Ok(()) => error,
        Err(restore) => UsageError::Other(format!("{error}；{restore}")),
    }
}

fn success_outcome(path: &Path, backup: &Path) -> SwitchOutcome {
    SwitchOutcome {
        tool_id: CATALOG_ID.to_string(),
        config_path: path.display().to_string(),
        backup_path: Some(backup.display().to_string()),
        keychain_updated: false,
        link_mode: None,
        success: true,
        error: None,
    }
}

fn readback_error() -> UsageError {
    UsageError::Other("Qoder state.vscdb 回读校验失败，切换未生效".into())
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

fn read_columns(path: &Path) -> UsageResult<Columns> {
    let [
        user_info_plain,
        user_info_secret,
        plan_plain,
        plan_secret,
        credit_plain,
        credit_secret,
    ] = read_six(path)?;
    Ok(Columns {
        user_info_plain,
        user_info_secret,
        plan_plain,
        plan_secret,
        credit_plain,
        credit_secret,
    })
}

fn read_six(path: &Path) -> UsageResult<[Option<String>; 6]> {
    vscdb::read_item_strings(path, COLUMNS)?
        .try_into()
        .map_err(|_| UsageError::Other("Qoder 状态字段数量不一致".into()))
}

fn read_live(path: &Path) -> UsageResult<Option<LiveAuth>> {
    if !path.is_file() {
        return Ok(None);
    }
    live_from_columns(&read_columns(path)?)
}

fn live_from_columns(columns: &Columns) -> UsageResult<Option<LiveAuth>> {
    match open_stored(columns.user_info_plain.as_deref()) {
        Opened::Plain(text) => Ok(Some(parse_user_info(&text)?)),
        Opened::Locked => Ok(Some(LiveAuth::locked())),
        Opened::Absent => match open_stored(columns.user_info_secret.as_deref()) {
            Opened::Plain(text) => Ok(Some(parse_user_info(&text)?)),
            Opened::Locked => Ok(Some(LiveAuth::locked())),
            Opened::Absent => Ok(None),
        },
    }
}

enum Opened {
    Absent,
    Plain(String),
    Locked,
}

fn open_stored(raw: Option<&str>) -> Opened {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Opened::Absent;
    };
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        if is_buffer(&value) {
            return decrypt_opened(&secret_base64(raw));
        }
        if let Some(text) = value.as_str() {
            if looks_like_safe_storage(text) {
                return decrypt_opened(text.trim());
            }
            return Opened::Plain(text.to_string());
        }
        return Opened::Plain(raw.to_string());
    }
    if looks_like_safe_storage(raw) {
        return decrypt_opened(raw);
    }
    Opened::Plain(raw.to_string())
}

fn decrypt_opened(encoded: &str) -> Opened {
    match decrypt_stored(encoded) {
        Ok(plain) => Opened::Plain(plain),
        Err(_) => Opened::Locked,
    }
}

fn parse_user_info(text: &str) -> UsageResult<LiveAuth> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(LiveAuth {
            email: None,
            token: None,
            refresh: None,
            locked: false,
        });
    }
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|_| UsageError::Other("Qoder userInfo 不是 JSON".into()))?;
    let Some(map) = value.as_object() else {
        return Err(UsageError::Other("Qoder userInfo 不是 JSON 对象".into()));
    };
    Ok(LiveAuth {
        email: json_string(map, &["email", "mail"]).filter(|value| looks_like_email(value)),
        token: json_string(
            map,
            &["token", "securityOauthToken", "accessToken", "access_token"],
        ),
        refresh: json_string(map, &["refreshToken", "refresh_token"]),
        locked: false,
    })
}

fn matching_subscription(live: &LiveAuth) -> UsageResult<Option<Subscription>> {
    if live.locked || live.email.is_none() {
        return Ok(None);
    }
    Ok(storage::list_subscriptions()?
        .into_iter()
        .find(|subscription| {
            subscription.catalog_id == CATALOG_ID && same_account(subscription, live)
        }))
}

fn same_account(subscription: &Subscription, live: &LiveAuth) -> bool {
    if live.locked {
        return false;
    }
    match (email_of(subscription), live.email.as_deref()) {
        (Some(stored), Some(live_email)) => emails_eq(&stored, live_email),
        _ => false,
    }
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
}

fn email_of(subscription: &Subscription) -> Option<String> {
    nonempty(Some(subscription.display_name.clone()))
        .filter(|value| looks_like_email(value))
        .or_else(|| {
            nonempty(subscription.oauth_account_id.clone()).filter(|value| looks_like_email(value))
        })
}

fn name_of(subscription: &Subscription) -> Option<String> {
    let name = subscription.display_name.trim();
    if name.is_empty() || looks_like_email(name) || name.eq_ignore_ascii_case("qoder") {
        None
    } else {
        Some(name.to_string())
    }
}

fn provider_map(subscription: &Subscription) -> Map<String, Value> {
    let plain = subscription
        .provider_state_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .unwrap_or_default();
    match serde_json::from_str::<Value>(plain.trim()) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

fn secret_text(slot: &Option<String>) -> Option<String> {
    nonempty(slot.as_deref().map(crypto::decrypt))
}

fn json_string(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        let Some(value) = map
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)
        else {
            continue;
        };
        if let Some(text) = value.as_str() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn insert_some(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|text| !text.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
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

fn present(value: &Option<String>) -> Option<String> {
    value.as_ref().and_then(|text| {
        if text.trim().is_empty() {
            None
        } else {
            Some(text.clone())
        }
    })
}

fn looks_like_email(value: &str) -> bool {
    let value = value.trim();
    value.len() > 3 && value.contains('@') && !value.contains(char::is_whitespace)
}

fn emails_eq(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn encrypt_with(key: &KeyMaterial, plain: &str) -> UsageResult<String> {
    safe_storage::encrypt_secret(key, plain.as_bytes())
        .map_err(|err| UsageError::Other(format!("Qoder secret:// 加密失败：{err}")))
}

fn safe_storage_key() -> UsageResult<KeyMaterial> {
    let password = std::env::var(SAFE_STORAGE_PASSWORD_ENV).map_err(|_| missing_password())?;
    if password.trim().is_empty() {
        return Err(missing_password());
    }
    Ok(host_key(&password))
}

fn missing_password() -> UsageError {
    UsageError::Other("Qoder Safe Storage 口令未注入（不会读取系统钥匙串），切换未生效".into())
}

pub(super) fn host_key(password: &str) -> KeyMaterial {
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

fn decrypted(stored: Option<&str>) -> Option<String> {
    decrypt_stored(stored?).ok()
}

pub(super) fn decrypt_stored(stored: &str) -> UsageResult<String> {
    let key = safe_storage_key()?;
    let encoded = secret_base64(stored);
    let bytes = safe_storage::decrypt_secret(&key, &encoded)
        .map_err(|err| UsageError::Other(format!("Qoder secret:// 解密失败：{err}")))?;
    String::from_utf8(bytes).map_err(|_| UsageError::Other("Qoder secret:// 不是 UTF-8".into()))
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
#[path = "qoder_tests.rs"]
mod tests;

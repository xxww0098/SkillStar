//! Windsurf account switching through `state.vscdb`.
//!
//! Backup, one ItemTable transaction, read-back, then pin. Only auth keys are
//! written or cleared — `windsurf_auth-*` usage-cache rows stay. `secret://`
//! values use [`skillstar_usage::tool_store::safe_storage`] and an injected
//! password (`SKILLSTAR_WINDSURF_SAFE_STORAGE_PASSWORD`). This path does not
//! read the system keychain. Restarting the official Windsurf app is not
//! verified here.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use skillstar_models::tool_sync::create_rolling_backup;
use skillstar_usage::crypto;
use skillstar_usage::fetchers::oauth::windsurf::{
    API_SERVER_SECRET_KEY, AUTH_STATUS_KEY, SESSIONS_SECRET_KEY,
};
use skillstar_usage::subscription::Subscription;
use skillstar_usage::tool_store::safe_storage::{self, KeyMaterial};
use skillstar_usage::{UsageError, UsageResult, storage, tool_paths, vscdb};

use super::ide::IdeCredentialAdapter;
use super::{CliAccountState, SwitchOutcome};

pub(super) const CATALOG_ID: &str = "windsurf";

const PRODUCT: &str = "Windsurf";
const SELECTED_AUTH_KEY: &str = "codeium.windsurf-windsurf_auth";
const EXTENSION_STATE_KEY: &str = "codeium.windsurf";
const API_SERVER_PLAIN_KEY: &str = "windsurf.apiServerUrl";
const SAFE_STORAGE_PASSWORD_ENV: &str = "SKILLSTAR_WINDSURF_SAFE_STORAGE_PASSWORD";

#[cfg(test)]
const READBACK_FAIL_ENV: &str = "SKILLSTAR_WINDSURF_READBACK_FAIL";

const AUTH_COLUMNS: &[&str] = &[
    AUTH_STATUS_KEY,
    SESSIONS_SECRET_KEY,
    API_SERVER_SECRET_KEY,
    API_SERVER_PLAIN_KEY,
];

const VERIFY_COLUMNS: &[&str] = &[
    AUTH_STATUS_KEY,
    SESSIONS_SECRET_KEY,
    API_SERVER_SECRET_KEY,
    SELECTED_AUTH_KEY,
    EXTENSION_STATE_KEY,
    API_SERVER_PLAIN_KEY,
];

pub(super) struct Adapter;

impl IdeCredentialAdapter for Adapter {
    fn catalog_id(&self) -> &'static str {
        CATALOG_ID
    }

    fn available(&self) -> bool {
        tool_paths::windsurf_state_db_path().is_some()
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
    let Some(live) = read_live(&live_path()?)? else {
        return Ok(CliAccountState::Missing);
    };
    let subscriptions = storage::list_subscriptions()?;
    let Some(subscription) = subscriptions.into_iter().find(|subscription| {
        subscription.catalog_id == CATALOG_ID && same_account(subscription, &live)
    }) else {
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
    let extension = plan_extension(&path, None)?;
    let backup = backup_db(&path)?;
    if let Err(error) = clear_auth(&path, &extension) {
        return Err(restore_or_combine(&path, &backup, error));
    }
    Ok(())
}

fn write_subscription(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    if subscription.catalog_id != CATALOG_ID {
        return Err(UsageError::Other(
            "Windsurf 切换收到了其它 catalog 的订阅".into(),
        ));
    }
    let path = live_path()?;
    require_db(&path)?;
    let prepared = prepare(&path, subscription)?;
    let backup = commit(&path, &prepared)?;
    Ok(success_outcome(&path, &backup))
}

fn live_path() -> UsageResult<PathBuf> {
    tool_paths::windsurf_state_db_path()
        .ok_or_else(|| UsageError::Other("无法解析 Windsurf 数据目录".into()))
}

fn display_path() -> PathBuf {
    live_path().unwrap_or_else(|_| PathBuf::from("Windsurf state.vscdb"))
}

fn require_db(path: &Path) -> UsageResult<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(UsageError::Other(format!(
            "未找到 Windsurf state.vscdb：{}。请先启动 Windsurf 并完成一次登录",
            path.display()
        )))
    }
}

#[derive(Debug, Clone, Default)]
struct LiveAuth {
    api_key: Option<String>,
    api_server_url: Option<String>,
    auth_token: Option<String>,
    refresh_token: Option<String>,
    auth1_token: Option<String>,
    proto: Option<String>,
    unreadable: bool,
}

impl LiveAuth {
    fn has_tokens(&self) -> bool {
        self.api_key.is_some()
            || self.auth_token.is_some()
            || self.refresh_token.is_some()
            || self.auth1_token.is_some()
    }
}

struct Material {
    api_key: Option<String>,
    api_server_url: Option<String>,
    auth_token: Option<String>,
    refresh_token: Option<String>,
    auth1_token: Option<String>,
    email: Option<String>,
    name: Option<String>,
    proto: Option<String>,
}

enum ExtensionPlan {
    Upsert(String),
    Delete,
    Unchanged(Option<String>),
}

struct AuthWrite {
    auth_status: String,
    sessions_plain: Option<String>,
    sessions_stored: Option<String>,
    api_server_url: Option<String>,
    api_server_stored: Option<String>,
    label: String,
    extension: ExtensionPlan,
}

fn prepare(path: &Path, subscription: &Subscription) -> UsageResult<AuthWrite> {
    let material = material_of(subscription);
    ensure_switchable(&material)?;
    let label = account_label(&material);
    let auth_status = auth_status_json(&material)?;
    let sessions_plain = sessions_plain(&material, &label);
    let api_server_url = material.api_server_url.clone();
    let (sessions_stored, api_server_stored) =
        encrypt_secrets(sessions_plain.as_deref(), api_server_url.as_deref())?;
    let extension = plan_extension(path, api_server_url.as_deref())?;
    Ok(AuthWrite {
        auth_status,
        sessions_plain,
        sessions_stored,
        api_server_url,
        api_server_stored,
        label,
        extension,
    })
}

fn ensure_switchable(material: &Material) -> UsageResult<()> {
    if material.api_key.is_none()
        && material.auth_token.is_none()
        && material.refresh_token.is_none()
        && material.auth1_token.is_none()
    {
        Err(UsageError::Other(
            "Windsurf 账号缺少 apiKey、authToken、refresh_token 或 auth1，切换未生效".into(),
        ))
    } else {
        Ok(())
    }
}

fn auth_status_json(material: &Material) -> UsageResult<String> {
    let mut map = Map::new();
    insert_some(&mut map, "apiKey", material.api_key.as_deref());
    insert_some(&mut map, "apiServerUrl", material.api_server_url.as_deref());
    insert_some(&mut map, "email", material.email.as_deref());
    insert_some(&mut map, "name", material.name.as_deref());
    insert_some(&mut map, "authToken", material.auth_token.as_deref());
    insert_some(&mut map, "refreshToken", material.refresh_token.as_deref());
    insert_some(&mut map, "auth1Token", material.auth1_token.as_deref());
    insert_some(
        &mut map,
        "userStatusProtoBinaryBase64",
        material.proto.as_deref(),
    );
    if material.auth1_token.is_some() {
        map.insert("authMethod".into(), Value::String("auth1".into()));
    }
    serde_json::to_string(&Value::Object(map))
        .map_err(|err| UsageError::Other(format!("序列化 windsurfAuthStatus 失败：{err}")))
}

fn sessions_plain(material: &Material, label: &str) -> Option<String> {
    let token = material
        .api_key
        .clone()
        .or_else(|| material.auth_token.clone())
        .or_else(|| material.auth1_token.clone())?;
    Some(
        serde_json::json!([{
            "id": uuid::Uuid::new_v4().to_string(),
            "accessToken": token,
            "account": { "label": label, "id": label },
            "scopes": []
        }])
        .to_string(),
    )
}

fn account_label(material: &Material) -> String {
    material
        .email
        .clone()
        .or_else(|| material.name.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "windsurf_user".to_string())
}

fn encrypt_secrets(
    sessions: Option<&str>,
    api_server: Option<&str>,
) -> UsageResult<(Option<String>, Option<String>)> {
    if sessions.is_none() && api_server.is_none() {
        return Ok((None, None));
    }
    let key = safe_storage_key()?;
    let sessions_stored = sessions
        .map(|plain| encrypt_with(&key, plain))
        .transpose()?;
    let server_stored = api_server
        .map(|plain| encrypt_with(&key, plain))
        .transpose()?;
    Ok((sessions_stored, server_stored))
}

fn encrypt_with(key: &KeyMaterial, plain: &str) -> UsageResult<String> {
    safe_storage::encrypt_secret(key, plain.as_bytes())
        .map_err(|err| UsageError::Other(format!("Windsurf secret:// 加密失败：{err}")))
}

fn plan_extension(path: &Path, api_server: Option<&str>) -> UsageResult<ExtensionPlan> {
    let existing = vscdb::read_item_string(path, EXTENSION_STATE_KEY)?;
    if existing.is_none() && api_server.is_none() {
        return Ok(ExtensionPlan::Unchanged(None));
    }
    let mut value = existing
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .filter(|value| value.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    let Value::Object(obj) = &mut value else {
        return Err(UsageError::Other("Windsurf 扩展状态不是对象".into()));
    };
    match api_server {
        Some(url) => {
            obj.insert("apiServerUrl".to_string(), Value::String(url.to_string()));
        }
        None => {
            obj.remove("apiServerUrl");
        }
    }
    if obj.is_empty() {
        return Ok(if existing.is_some() {
            ExtensionPlan::Delete
        } else {
            ExtensionPlan::Unchanged(None)
        });
    }
    let text = serde_json::to_string(&value)
        .map_err(|err| UsageError::Other(format!("序列化 codeium.windsurf 失败：{err}")))?;
    if existing.as_deref() == Some(text.as_str()) {
        Ok(ExtensionPlan::Unchanged(existing))
    } else {
        Ok(ExtensionPlan::Upsert(text))
    }
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
    let mut upserts = vec![
        (AUTH_STATUS_KEY.to_string(), prepared.auth_status.clone()),
        (SELECTED_AUTH_KEY.to_string(), prepared.label.clone()),
    ];
    let mut deletes = Vec::new();
    match &prepared.sessions_stored {
        Some(stored) => upserts.push((SESSIONS_SECRET_KEY.to_string(), stored.clone())),
        None => deletes.push(SESSIONS_SECRET_KEY.to_string()),
    }
    match &prepared.api_server_stored {
        Some(stored) => upserts.push((API_SERVER_SECRET_KEY.to_string(), stored.clone())),
        None => deletes.push(API_SERVER_SECRET_KEY.to_string()),
    }
    match &prepared.api_server_url {
        Some(url) => upserts.push((API_SERVER_PLAIN_KEY.to_string(), url.clone())),
        None => deletes.push(API_SERVER_PLAIN_KEY.to_string()),
    }
    push_extension(&mut upserts, &mut deletes, &prepared.extension);
    (upserts, deletes)
}

fn push_extension(
    upserts: &mut Vec<(String, String)>,
    deletes: &mut Vec<String>,
    plan: &ExtensionPlan,
) {
    match plan {
        ExtensionPlan::Upsert(value) => {
            upserts.push((EXTENSION_STATE_KEY.to_string(), value.clone()))
        }
        ExtensionPlan::Delete => deletes.push(EXTENSION_STATE_KEY.to_string()),
        ExtensionPlan::Unchanged(_) => {}
    }
}

fn verify_write(path: &Path, prepared: &AuthWrite) -> UsageResult<()> {
    if readback_forced_failure() {
        return Err(readback_error());
    }
    let [
        status,
        sessions,
        server_secret,
        selected,
        extension,
        server_plain,
    ] = read_columns(path, VERIFY_COLUMNS)?;
    if status.as_deref() != Some(prepared.auth_status.as_str())
        || selected.as_deref() != Some(prepared.label.as_str())
        || server_plain.as_deref() != prepared.api_server_url.as_deref()
    {
        return Err(readback_error());
    }
    match (&prepared.sessions_plain, sessions.as_deref()) {
        (Some(plain), Some(stored)) => {
            if decrypt_stored(stored).ok().as_deref() != Some(plain.as_str()) {
                return Err(readback_error());
            }
        }
        (None, None) => {}
        _ => return Err(readback_error()),
    }
    match (&prepared.api_server_stored, server_secret.as_deref()) {
        (Some(_), Some(stored)) => {
            if decrypt_stored(stored).ok().as_deref() != prepared.api_server_url.as_deref() {
                return Err(readback_error());
            }
        }
        (None, None) => {}
        _ => return Err(readback_error()),
    }
    if !extension_matches(&prepared.extension, extension.as_deref()) {
        return Err(readback_error());
    }
    Ok(())
}

fn extension_matches(plan: &ExtensionPlan, actual: Option<&str>) -> bool {
    match plan {
        ExtensionPlan::Upsert(expected) => actual == Some(expected.as_str()),
        ExtensionPlan::Delete => actual.is_none(),
        ExtensionPlan::Unchanged(expected) => actual == expected.as_deref(),
    }
}

fn clear_auth(path: &Path, extension: &ExtensionPlan) -> UsageResult<()> {
    let mut upserts = Vec::new();
    let mut deletes = vec![
        AUTH_STATUS_KEY.to_string(),
        SESSIONS_SECRET_KEY.to_string(),
        API_SERVER_SECRET_KEY.to_string(),
        SELECTED_AUTH_KEY.to_string(),
        API_SERVER_PLAIN_KEY.to_string(),
    ];
    push_extension(&mut upserts, &mut deletes, extension);
    let upsert_refs: Vec<(&str, &str)> = upserts
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let delete_refs: Vec<&str> = deletes.iter().map(String::as_str).collect();
    vscdb::mutate_labeled_items(path, PRODUCT, &upsert_refs, &delete_refs)?;
    let [
        status,
        sessions,
        server_secret,
        selected,
        extension_value,
        server_plain,
    ] = read_columns(path, VERIFY_COLUMNS)?;
    if status.is_some()
        || sessions.is_some()
        || server_secret.is_some()
        || selected.is_some()
        || server_plain.is_some()
        || !extension_matches(extension, extension_value.as_deref())
    {
        return Err(readback_error());
    }
    if path.is_file() {
        Ok(())
    } else {
        Err(UsageError::Other(
            "Windsurf 清键后 state.vscdb 不见了，切换未生效".into(),
        ))
    }
}

fn backup_db(path: &Path) -> UsageResult<PathBuf> {
    let backup = create_rolling_backup(path)
        .map_err(|err| UsageError::Other(format!("备份 Windsurf state.vscdb 失败：{err}")))?;
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
            "回滚 Windsurf state.vscdb 失败：{err}（备份 {}）",
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
    UsageError::Other("Windsurf state.vscdb 回读校验失败，切换未生效".into())
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

fn read_columns<const N: usize>(path: &Path, keys: &[&str]) -> UsageResult<[Option<String>; N]> {
    vscdb::read_item_strings(path, keys)?
        .try_into()
        .map_err(|_| UsageError::Other("Windsurf 状态字段数量不一致".into()))
}

fn read_live(path: &Path) -> UsageResult<Option<LiveAuth>> {
    if !path.is_file() {
        return Ok(None);
    }
    let [status, sessions, server_secret, server_plain] = read_columns(path, AUTH_COLUMNS)?;
    let mut live = match status.as_deref() {
        Some(raw) => parse_auth_status(raw)?,
        None => LiveAuth::default(),
    };
    if live.unreadable {
        return Ok(Some(live));
    }
    let sessions_read = open_secret(sessions.as_deref());
    let server_read = open_secret(server_secret.as_deref());
    if live.api_key.is_none()
        && let SecretRead::Plain(plain) = &sessions_read
        && let Some(token) = session_access_token(plain)
    {
        if looks_like_api_key(&token) {
            live.api_key = Some(token);
        } else if live.auth_token.is_none() {
            live.auth_token = Some(token);
        }
    }
    if live.api_server_url.is_none() {
        live.api_server_url = nonempty_ref(server_plain.as_deref());
    }
    if live.api_server_url.is_none()
        && let SecretRead::Plain(plain) = &server_read
    {
        live.api_server_url = nonempty(Some(plain.clone()));
    }
    if live.has_tokens() {
        return Ok(Some(live));
    }
    let locked =
        matches!(sessions_read, SecretRead::Locked) || matches!(server_read, SecretRead::Locked);
    if locked {
        live.unreadable = true;
        return Ok(Some(live));
    }
    Ok(None)
}

enum SecretRead {
    Absent,
    Plain(String),
    Locked,
}

fn open_secret(stored: Option<&str>) -> SecretRead {
    let Some(stored) = stored.map(str::trim).filter(|value| !value.is_empty()) else {
        return SecretRead::Absent;
    };
    match decrypt_stored(stored) {
        Ok(plain) => SecretRead::Plain(plain),
        Err(_) => SecretRead::Locked,
    }
}

fn parse_auth_status(raw: &str) -> UsageResult<LiveAuth> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(LiveAuth::default());
    }
    let json_text = if trimmed.starts_with('{') {
        trimmed.to_string()
    } else {
        match decrypt_stored(trimmed) {
            Ok(plain) => plain,
            Err(_) => {
                return Ok(LiveAuth {
                    unreadable: true,
                    ..LiveAuth::default()
                });
            }
        }
    };
    let value: Value = serde_json::from_str(&json_text)
        .map_err(|err| UsageError::Other(format!("解析 windsurfAuthStatus 失败：{err}")))?;
    let Some(map) = value.as_object() else {
        return Err(UsageError::Other(
            "windsurfAuthStatus 不是 JSON 对象".into(),
        ));
    };
    Ok(LiveAuth {
        api_key: json_string(map, &["apiKey", "api_key"]),
        api_server_url: json_string(map, &["apiServerUrl", "api_server_url"]),
        auth_token: json_string(
            map,
            &["authToken", "accessToken", "sessionToken", "auth_token"],
        ),
        refresh_token: json_string(
            map,
            &["refreshToken", "refresh_token", "firebaseRefreshToken"],
        ),
        auth1_token: json_string(map, &["auth1Token", "auth1_token"]),
        proto: json_string(map, &["userStatusProtoBinaryBase64"]),
        unreadable: false,
    })
}

fn session_access_token(plain: &str) -> Option<String> {
    let value: Value = serde_json::from_str(plain).ok()?;
    let entry = value
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(&value);
    entry
        .as_object()
        .and_then(|map| json_string(map, &["accessToken", "access_token", "apiKey", "api_key"]))
}

fn looks_like_api_key(token: &str) -> bool {
    token.starts_with("sk-ws-") || token.starts_with("devin-session-token$")
}

fn same_account(subscription: &Subscription, live: &LiveAuth) -> bool {
    if live.unreadable {
        return false;
    }
    let stored = material_of(subscription);
    eq_token(
        stored.refresh_token.as_deref(),
        live.refresh_token.as_deref(),
    ) || eq_token(stored.auth1_token.as_deref(), live.auth1_token.as_deref())
        || eq_token(stored.api_key.as_deref(), live.api_key.as_deref())
        || eq_token(stored.auth_token.as_deref(), live.auth_token.as_deref())
}

fn eq_token(stored: Option<&str>, live: Option<&str>) -> bool {
    match (
        stored.filter(|value| !value.is_empty()),
        live.filter(|value| !value.is_empty()),
    ) {
        (Some(stored), Some(live)) => stored == live,
        _ => false,
    }
}

fn absorb(subscription: &Subscription, live: &LiveAuth) -> Subscription {
    if live.unreadable {
        return subscription.clone();
    }
    let mut updated = subscription.clone();
    assign_secret(
        &mut updated.access_token_encrypted,
        live.auth_token.as_deref(),
    );
    assign_secret(
        &mut updated.refresh_token_encrypted,
        live.refresh_token.as_deref(),
    );
    assign_provider(
        &mut updated.provider_state_encrypted,
        projected_provider(subscription, live),
    );
    updated
}

fn projected_provider(subscription: &Subscription, live: &LiveAuth) -> Option<String> {
    let mut map = provider_map(subscription);
    insert_some(&mut map, "apiKey", live.api_key.as_deref());
    insert_some(&mut map, "apiServerUrl", live.api_server_url.as_deref());
    insert_some(&mut map, "auth1Token", live.auth1_token.as_deref());
    insert_some(
        &mut map,
        "userStatusProtoBinaryBase64",
        live.proto.as_deref(),
    );
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map).to_string())
    }
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

fn assign_provider(slot: &mut Option<String>, next: Option<String>) {
    let Some(next) = next.filter(|value| !value.is_empty()) else {
        return;
    };
    let current = slot.as_deref().map(crypto::decrypt).unwrap_or_default();
    if current != next {
        *slot = Some(crypto::encrypt(&next));
    }
}

fn credentials_changed(updated: &Subscription, original: &Subscription) -> bool {
    updated.access_token_encrypted != original.access_token_encrypted
        || updated.refresh_token_encrypted != original.refresh_token_encrypted
        || updated.provider_state_encrypted != original.provider_state_encrypted
}

fn material_of(subscription: &Subscription) -> Material {
    let map = provider_map(subscription);
    let access = nonempty(
        subscription
            .access_token_encrypted
            .as_deref()
            .map(crypto::decrypt),
    );
    let api_key = json_string(&map, &["apiKey", "api_key"])
        .or_else(|| access.clone().filter(|token| looks_like_api_key(token)));
    let auth_token = access.filter(|token| Some(token) != api_key.as_ref());
    Material {
        api_key,
        api_server_url: json_string(&map, &["apiServerUrl", "api_server_url"]),
        auth_token,
        refresh_token: nonempty(
            subscription
                .refresh_token_encrypted
                .as_deref()
                .map(crypto::decrypt),
        ),
        auth1_token: json_string(&map, &["auth1Token", "auth1_token"]),
        email: email_of(subscription),
        name: name_of(subscription, &map),
        proto: json_string(&map, &["userStatusProtoBinaryBase64"]),
    }
}

fn provider_map(subscription: &Subscription) -> Map<String, Value> {
    let plain = subscription
        .provider_state_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .unwrap_or_default();
    let trimmed = plain.trim();
    if trimmed.is_empty() {
        return Map::new();
    }
    if let Ok(Value::Object(map)) = serde_json::from_str(trimmed) {
        return map;
    }
    let mut map = Map::new();
    if !trimmed.contains(char::is_whitespace) && trimmed.starts_with("auth1_") {
        map.insert("auth1Token".to_string(), Value::String(trimmed.to_string()));
    } else if !trimmed.contains(char::is_whitespace) && looks_like_api_key(trimmed) {
        map.insert("apiKey".to_string(), Value::String(trimmed.to_string()));
    }
    map
}

fn email_of(subscription: &Subscription) -> Option<String> {
    nonempty(subscription.oauth_account_id.clone())
        .filter(|value| value.contains('@'))
        .or_else(|| {
            let name = subscription.display_name.trim();
            name.contains('@').then(|| name.to_string())
        })
}

fn name_of(subscription: &Subscription, map: &Map<String, Value>) -> Option<String> {
    json_string(map, &["name"]).or_else(|| {
        let name = subscription.display_name.trim();
        if name.is_empty() || name.contains('@') || name.eq_ignore_ascii_case("windsurf") {
            None
        } else {
            Some(name.to_string())
        }
    })
}

fn json_string(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = map.get(*key).and_then(Value::as_str) {
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

fn nonempty_ref(value: Option<&str>) -> Option<String> {
    nonempty(value.map(str::to_string))
}

fn safe_storage_key() -> UsageResult<KeyMaterial> {
    let password = std::env::var(SAFE_STORAGE_PASSWORD_ENV).map_err(|_| missing_password())?;
    if password.trim().is_empty() {
        return Err(missing_password());
    }
    Ok(host_key(&password))
}

fn missing_password() -> UsageError {
    UsageError::Other("Windsurf Safe Storage 口令未注入（不会读取系统钥匙串），切换未生效".into())
}

fn host_key(password: &str) -> KeyMaterial {
    #[cfg(target_os = "macos")]
    {
        return KeyMaterial::macos_v10(password);
    }
    #[cfg(target_os = "linux")]
    {
        return KeyMaterial::linux_v10(password);
    }
    #[cfg(target_os = "windows")]
    {
        return KeyMaterial::OsCryptKey(windows_os_crypt_key(password));
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

fn decrypt_stored(stored: &str) -> UsageResult<String> {
    let key = safe_storage_key()?;
    let encoded = secret_base64(stored);
    let bytes = safe_storage::decrypt_secret(&key, &encoded)
        .map_err(|err| UsageError::Other(format!("Windsurf secret:// 解密失败：{err}")))?;
    String::from_utf8(bytes).map_err(|_| UsageError::Other("Windsurf secret:// 不是 UTF-8".into()))
}

/// Official rows are standard base64. Cockpit also writes
/// `{"type":"Buffer","data":[...]}`.
fn secret_base64(stored: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(stored.trim()) else {
        return stored.trim().to_string();
    };
    if value.get("type").and_then(Value::as_str) != Some("Buffer") {
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
#[path = "windsurf_tests.rs"]
mod tests;

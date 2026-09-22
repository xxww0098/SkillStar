//! Pasted refresh token / credential JSON, and `storage.json` iCube values.
//!
//! `storage.json` values are standard base64 of a `byte_crypto` blob, or
//! already-plaintext JSON. Tests point `SKILLSTAR_TOOL_SYNC_HOME` at a temp
//! directory. This module does not read the real home on its own.

use std::fs;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Map, Value};

use super::{TraeAuthState, nonempty, pick_string};
use crate::catalog::AuthMode;
use crate::fetchers::trae::device;
use crate::subscription::{BillingCycle, Subscription};
use crate::token_import::ImportedToken;
use crate::tool_store::byte_crypto;
use crate::trae_platform::TraePlatformKind;
use crate::{UsageError, UsageResult};

const AUTH_KEY: &str = "iCubeAuthInfo://icube.cloudide";
const SERVER_KEY: &str = "iCubeServerData://icube.cloudide";
const DEVICE_PREFIX: &str = "iCubeAuthInfo://icube-dc:";
const MIN_BARE_LEN: usize = 20;

pub(crate) fn import_from_token(raw: &str) -> UsageResult<ImportedToken> {
    parse_token(TraePlatformKind::Trae, raw)
}

pub(crate) fn import_from_token_solo(raw: &str) -> UsageResult<ImportedToken> {
    parse_token(TraePlatformKind::TraeSolo, raw)
}

pub(crate) fn import_from_token_cn(raw: &str) -> UsageResult<ImportedToken> {
    parse_token(TraePlatformKind::TraeCn, raw)
}

pub(crate) fn import_from_token_solo_cn(raw: &str) -> UsageResult<ImportedToken> {
    parse_token(TraePlatformKind::TraeSoloCn, raw)
}

pub(crate) fn import_from_local() -> UsageResult<ImportedToken> {
    read_local(TraePlatformKind::Trae)
}

pub(crate) fn import_from_local_solo() -> UsageResult<ImportedToken> {
    read_local(TraePlatformKind::TraeSolo)
}

pub(crate) fn import_from_local_cn() -> UsageResult<ImportedToken> {
    read_local(TraePlatformKind::TraeCn)
}

pub(crate) fn import_from_local_solo_cn() -> UsageResult<ImportedToken> {
    read_local(TraePlatformKind::TraeSoloCn)
}

pub(crate) fn oauth_row_from_imported(imported: ImportedToken) -> UsageResult<Subscription> {
    oauth_row(TraePlatformKind::Trae, imported)
}

pub(crate) fn oauth_row_from_imported_solo(imported: ImportedToken) -> UsageResult<Subscription> {
    oauth_row(TraePlatformKind::TraeSolo, imported)
}

pub(crate) fn oauth_row_from_imported_cn(imported: ImportedToken) -> UsageResult<Subscription> {
    oauth_row(TraePlatformKind::TraeCn, imported)
}

pub(crate) fn oauth_row_from_imported_solo_cn(
    imported: ImportedToken,
) -> UsageResult<Subscription> {
    oauth_row(TraePlatformKind::TraeSoloCn, imported)
}

pub(crate) fn parse_token(kind: TraePlatformKind, raw: &str) -> UsageResult<ImportedToken> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(UsageError::Other(format!(
            "{} 令牌为空",
            kind.display_name()
        )));
    }
    if trimmed.starts_with(['{', '[', '"']) {
        let value: Value = serde_json::from_str(trimmed).map_err(|_| {
            UsageError::Other(format!("{} 凭据 JSON 无法解析", kind.display_name()))
        })?;
        return imported_from_value(kind, &value);
    }
    if trimmed.len() >= MIN_BARE_LEN && !trimmed.chars().any(char::is_whitespace) {
        return imported_from_secrets(
            kind,
            None,
            Some(trimmed.to_string()),
            None,
            SecretsMeta::default(),
        );
    }
    Err(UsageError::Other(format!(
        "{} 粘贴的不是凭据 JSON，也不是 refresh token。裸 token 可能因设备绑定失败，请优先本机导入。",
        kind.display_name()
    )))
}

pub(crate) fn read_local(kind: TraePlatformKind) -> UsageResult<ImportedToken> {
    let path = crate::tool_paths::trae_storage_path_for(kind)
        .ok_or_else(|| UsageError::Other(format!("无法解析 {} 的数据目录", kind.display_name())))?;
    if !path.is_file() {
        return Err(UsageError::Other(format!(
            "未找到 {} 的 storage.json（{}）。请先在 {} 登录，再用本机导入。",
            kind.display_name(),
            path.display(),
            kind.display_name()
        )));
    }
    let text = fs::read_to_string(&path).map_err(|err| {
        UsageError::Other(format!(
            "读取 {} storage.json 失败：{err}",
            kind.display_name()
        ))
    })?;
    let value: Value = serde_json::from_str(&text).map_err(|_| {
        UsageError::Other(format!("{} storage.json 不是 JSON", kind.display_name()))
    })?;
    imported_from_value(kind, &value)
}

pub(crate) fn oauth_row(
    kind: TraePlatformKind,
    imported: ImportedToken,
) -> UsageResult<Subscription> {
    if imported.access_token.trim().is_empty()
        && imported
            .refresh_token
            .as_deref()
            .is_none_or(|token| token.trim().is_empty())
    {
        return Err(UsageError::Other(format!(
            "{} 导入没有可用令牌",
            kind.display_name()
        )));
    }
    let now = chrono::Utc::now().timestamp();
    let provider_state = imported
        .provider_state
        .filter(|value| !value.trim().is_empty());
    Ok(Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: kind.catalog_id().to_string(),
        display_name: imported.display_name,
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: imported.currency.unwrap_or_else(|| "USD".to_string()),
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        platform_token_encrypted: None,
        access_token_encrypted: (!imported.access_token.trim().is_empty())
            .then(|| crate::crypto::encrypt(&imported.access_token)),
        refresh_token_encrypted: imported
            .refresh_token
            .filter(|value| !value.trim().is_empty())
            .map(|value| crate::crypto::encrypt(&value)),
        access_token_expires_at: imported.expires_at,
        id_token_encrypted: None,
        oauth_account_id: imported.oauth_account_id,
        oauth_region: imported.oauth_region,
        requires_reauth: false,
        provider_state_encrypted: provider_state.as_deref().map(crate::crypto::encrypt),
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    })
}

fn imported_from_value(kind: TraePlatformKind, value: &Value) -> UsageResult<ImportedToken> {
    match value {
        Value::Array(items) => first_account(kind, items),
        Value::String(text) => parse_token(kind, text),
        Value::Object(map) if is_storage_root(map) => imported_from_storage(kind, map),
        Value::Object(map) => {
            if let Some(list) = map
                .get("accounts")
                .or_else(|| map.get("items"))
                .and_then(Value::as_array)
            {
                return first_account(kind, list);
            }
            imported_from_credential(kind, value)
        }
        _ => Err(UsageError::Other(format!(
            "{} 凭据 JSON 没有令牌",
            kind.display_name()
        ))),
    }
}

fn first_account(kind: TraePlatformKind, items: &[Value]) -> UsageResult<ImportedToken> {
    let mut last = None;
    for item in items {
        match imported_from_value(kind, item) {
            Ok(imported) => return Ok(imported),
            Err(err) => last = Some(err),
        }
    }
    Err(last.unwrap_or_else(|| UsageError::Other(format!("{} 导入数组为空", kind.display_name()))))
}

fn is_storage_root(map: &Map<String, Value>) -> bool {
    map.keys().any(|key| {
        key.starts_with("iCubeAuthInfo://")
            || key.starts_with("iCubeServerData://")
            || key.starts_with("iCubeEntitlementInfo://")
    })
}

fn imported_from_storage(
    kind: TraePlatformKind,
    map: &Map<String, Value>,
) -> UsageResult<ImportedToken> {
    let auth = open_named(map, AUTH_KEY)?;
    let server = open_named(map, SERVER_KEY)?;
    let device = open_device_key(map)?;
    let access = first_string(
        &[auth.as_ref(), server.as_ref()],
        &[
            &["accessToken"],
            &["access_token"],
            &["token"],
            &["data", "accessToken"],
        ],
    );
    let refresh = first_string(
        &[auth.as_ref(), server.as_ref()],
        &[
            &["refreshToken"],
            &["refresh_token"],
            &["RefreshToken"],
            &["exchangeResponse", "Result", "RefreshToken"],
        ],
    );
    let user_id = first_string(
        &[auth.as_ref(), server.as_ref()],
        &[&["userId"], &["user_id"], &["uid"], &["UserID"], &["id"]],
    );
    let email = first_string(
        &[auth.as_ref(), server.as_ref()],
        &[
            &["email"],
            &["NonPlainTextEmail"],
            &["account", "email"],
            &["user", "email"],
        ],
    );
    let client_id = first_string(
        &[auth.as_ref(), server.as_ref()],
        &[&["authClientId"], &["clientId"], &["ClientID"]],
    );
    let login_host = first_string(
        &[auth.as_ref(), server.as_ref()],
        &[&["loginHost"], &["host"], &["account", "host"]],
    );
    let auth_domain = first_string(&[auth.as_ref()], &[&["authDomain"]]);
    let login_region = first_string(&[auth.as_ref(), server.as_ref()], &[&["loginRegion"]]);
    let key = device.or_else(|| key_from_value(auth.as_ref()));
    imported_from_secrets(
        kind,
        access,
        refresh,
        key,
        SecretsMeta {
            user_id,
            email,
            client_id,
            login_host,
            auth_domain,
            login_region,
        },
    )
}

#[derive(Default)]
struct SecretsMeta {
    user_id: Option<String>,
    email: Option<String>,
    client_id: Option<String>,
    login_host: Option<String>,
    auth_domain: Option<String>,
    login_region: Option<String>,
}

fn imported_from_credential(kind: TraePlatformKind, value: &Value) -> UsageResult<ImportedToken> {
    let auth = value
        .get("trae_auth_raw")
        .or_else(|| value.get("auth_raw"))
        .or_else(|| value.get("auth"));
    let access = pick_string(
        value,
        &[
            &["access_token"],
            &["accessToken"],
            &["token"],
            &["trae_access_token"],
        ],
    )
    .or_else(|| {
        auth.and_then(|auth| pick_string(auth, &[&["accessToken"], &["access_token"], &["token"]]))
    });
    let refresh = pick_string(
        value,
        &[&["refresh_token"], &["refreshToken"], &["RefreshToken"]],
    )
    .or_else(|| {
        auth.and_then(|auth| {
            pick_string(
                auth,
                &[&["refreshToken"], &["refresh_token"], &["RefreshToken"]],
            )
        })
    });
    let key = key_from_value(Some(value)).or_else(|| key_from_value(auth));
    imported_from_secrets(
        kind,
        access,
        refresh,
        key,
        SecretsMeta {
            user_id: pick_string(value, &[&["user_id"], &["userId"], &["uid"], &["UserID"]]),
            email: pick_string(value, &[&["email"], &["NonPlainTextEmail"]]),
            client_id: pick_string(value, &[&["clientId"], &["ClientID"], &["authClientId"]]),
            login_host: pick_string(value, &[&["loginHost"], &["host"]]),
            auth_domain: pick_string(value, &[&["authDomain"]]),
            login_region: pick_string(value, &[&["loginRegion"], &["oauth_region"]]),
        },
    )
}

fn imported_from_secrets(
    kind: TraePlatformKind,
    access: Option<String>,
    refresh: Option<String>,
    key: Option<(String, String)>,
    meta: SecretsMeta,
) -> UsageResult<ImportedToken> {
    let access = nonempty(access.as_deref()).unwrap_or_default();
    let refresh = nonempty(refresh.as_deref());
    if access.is_empty() && refresh.is_none() {
        return Err(UsageError::Other(format!(
            "{} 凭据里没有 access token 或 refresh token。请优先本机导入。",
            kind.display_name()
        )));
    }
    let (private_pem, public_pem) = match key {
        Some((private_pem, public_pem)) => (private_pem, public_pem),
        None if refresh.is_some() => {
            let generated = device::generate_device_keypair().map_err(|err| {
                UsageError::Fetcher(format!("{} 设备密钥生成失败：{err}", kind.display_name()))
            })?;
            (generated.private_pem, generated.public_pem)
        }
        None => (String::new(), String::new()),
    };
    let state = TraeAuthState {
        private_pem,
        public_pem,
        client_id: nonempty(meta.client_id.as_deref())
            .unwrap_or_else(|| kind.auth_client_id().to_string()),
        login_host: nonempty(meta.login_host.as_deref())
            .and_then(|host| super::normalize_origin(&host))
            .unwrap_or_else(|| kind.default_login_host().to_string()),
        auth_domain: nonempty(meta.auth_domain.as_deref())
            .unwrap_or_else(|| kind.auth_domain().to_string()),
    };
    let email = nonempty(meta.email.as_deref()).filter(|value| value.contains('@'));
    let region = nonempty(meta.login_region.as_deref())
        .as_deref()
        .and_then(super::quota::normalize_login_region)
        .or_else(|| Some(infer_region(kind, &state.login_host)));
    Ok(ImportedToken {
        display_name: email
            .clone()
            .unwrap_or_else(|| kind.display_name().to_string()),
        access_token: access,
        refresh_token: refresh,
        expires_at: None,
        oauth_account_id: nonempty(meta.user_id.as_deref()),
        provider_state: state.has_key().then(|| state.to_json()),
        currency: Some("USD".to_string()),
        oauth_region: region,
    })
}

fn infer_region(kind: TraePlatformKind, login_host: &str) -> String {
    let host = login_host.to_ascii_lowercase();
    if host.contains(".cn") || kind.is_cn() {
        "cn".to_string()
    } else if host.contains(".us") {
        "us".to_string()
    } else {
        "sg".to_string()
    }
}

fn first_string(roots: &[Option<&Value>], paths: &[&[&str]]) -> Option<String> {
    for root in roots.iter().copied().flatten() {
        if let Some(value) = pick_string(root, paths) {
            return Some(value);
        }
    }
    None
}

fn key_from_value(value: Option<&Value>) -> Option<(String, String)> {
    let value = value?;
    let nested = value.get("deviceKeyPair").unwrap_or(value);
    let private_pem = pick_string(nested, &[&["privateKeyPEM"], &["private_key_pem"]])?;
    let public_pem = pick_string(nested, &[&["publicKeyPEM"], &["public_key_pem"]])?;
    Some((private_pem, public_pem))
}

fn open_named(map: &Map<String, Value>, key: &str) -> UsageResult<Option<Value>> {
    match map.get(key) {
        Some(value) => open_storage_value(value),
        None => Ok(None),
    }
}

fn open_device_key(map: &Map<String, Value>) -> UsageResult<Option<(String, String)>> {
    let mut keys: Vec<&String> = map
        .keys()
        .filter(|key| key.starts_with(DEVICE_PREFIX))
        .collect();
    keys.sort();
    for key in keys {
        if let Some(value) = open_storage_value(&map[key])? {
            if let Some(pair) = key_from_value(Some(&value)) {
                return Ok(Some(pair));
            }
        }
    }
    Ok(None)
}

fn open_storage_value(value: &Value) -> UsageResult<Option<Value>> {
    if value.is_object() || value.is_array() {
        return Ok(Some(value.clone()));
    }
    let Some(text) = value.as_str() else {
        return Ok(None);
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        if parsed.is_string() {
            return open_storage_value(&parsed);
        }
        return Ok(Some(parsed));
    }
    let bytes = BASE64.decode(trimmed).map_err(|_| {
        UsageError::Other("Trae storage.json 的 iCube 值不是合法 base64，也不是 JSON".into())
    })?;
    let plain = byte_crypto::decode(&bytes)
        .map_err(|err| UsageError::Other(format!("Trae storage.json 解密失败：{err}")))?;
    let text = String::from_utf8(plain)
        .map_err(|_| UsageError::Other("Trae storage.json 解密结果不是 UTF-8".into()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|_| UsageError::Other("Trae storage.json 解密结果不是 JSON".into()))
}

/// Test helper: cockpit-shaped `storage.json` object. `auth` and `device` are
/// encrypted; callers pass the plaintext JSON values.
#[cfg(test)]
pub(super) fn cockpit_storage(auth: &Value, device_pair: Option<&Value>) -> Value {
    let mut map = Map::new();
    map.insert(AUTH_KEY.to_string(), cipher_string(auth));
    if let Some(pair) = device_pair {
        map.insert(format!("{DEVICE_PREFIX}device-1"), cipher_string(pair));
    }
    Value::Object(map)
}

#[cfg(test)]
fn cipher_string(value: &Value) -> Value {
    let plain = serde_json::to_vec(value).expect("json");
    let blob = byte_crypto::encode(&plain).expect("byte_crypto");
    Value::String(BASE64.encode(blob))
}

#[cfg(test)]
pub(super) fn write_storage(path: &std::path::Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(path, serde_json::to_vec_pretty(value).expect("storage")).expect("write");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    fn guard(path: &Path) -> impl Drop {
        struct Env {
            _lock: std::sync::MutexGuard<'static, ()>,
            tool: Option<std::ffi::OsString>,
        }
        impl Drop for Env {
            fn drop(&mut self) {
                unsafe {
                    match self.tool.as_deref() {
                        Some(value) => std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", value),
                        None => std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME"),
                    }
                }
            }
        }
        let lock = crate::test_env_lock()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let tool = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
        unsafe { std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", path) }
        Env { _lock: lock, tool }
    }

    #[test]
    fn bare_refresh_and_json_keep_kind_and_do_not_share_a_generated_key() {
        let bare = parse_token(TraePlatformKind::Trae, "refresh-token-value-0001").unwrap();
        assert!(bare.access_token.is_empty());
        assert_eq!(
            bare.refresh_token.as_deref(),
            Some("refresh-token-value-0001")
        );
        let state: Value = serde_json::from_str(bare.provider_state.as_deref().unwrap()).unwrap();
        assert_eq!(state["clientId"], "ono9krqynydwx5");
        assert_eq!(state["loginHost"], "https://grow-normal.trae.ai");
        assert_eq!(state["authDomain"], "www.trae.ai");
        assert!(
            state["deviceKeyPair"]["privateKeyPEM"]
                .as_str()
                .unwrap()
                .contains("PRIVATE KEY")
        );
        assert!(
            bare.provider_state
                .as_deref()
                .unwrap()
                .contains("deviceKeyPair")
        );
        assert!(
            !bare
                .provider_state
                .as_deref()
                .unwrap()
                .contains("platform_token")
        );

        let solo = parse_token(
            TraePlatformKind::TraeSoloCn,
            r#"{"refreshToken":"rt-solo-cn-0000000001","clientId":"from-json","loginHost":"https://api.trae.cn/ignored","authDomain":"www.trae.cn","loginRegion":"china-north"}"#,
        )
        .unwrap();
        let solo_state: Value =
            serde_json::from_str(solo.provider_state.as_deref().unwrap()).unwrap();
        assert_eq!(solo_state["clientId"], "from-json");
        assert_eq!(solo_state["loginHost"], "https://api.trae.cn");
        assert_eq!(solo.oauth_region.as_deref(), Some("cn"));
        let row = oauth_row(TraePlatformKind::TraeSoloCn, solo).unwrap();
        assert_eq!(row.catalog_id, "trae-solo-cn");
        assert!(row.platform_token_encrypted.is_none());
        assert!(row.access_token_encrypted.is_none());
        assert!(row.refresh_token_encrypted.is_some());

        for raw in ["", "short", "has space token-but-not-json", "{", "[]"] {
            assert!(parse_token(TraePlatformKind::Trae, raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn storage_json_byte_crypto_round_trips_and_a_flipped_byte_fails() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = guard(dir.path());
        let pair = device::generate_device_keypair().unwrap();
        let auth = json!({
            "accessToken": "access-from-disk",
            "refreshToken": "refresh-from-disk",
            "userId": "user-7",
            "email": "trae@example.com",
            "loginHost": "https://grow-normal.trae.ai",
            "loginRegion": "sg",
            "authClientId": "ono9krqynydwx5",
            "authDomain": "www.trae.ai",
            "nickname": "套餐"
        });
        let device_json = json!({
            "privateKeyPEM": pair.private_pem,
            "publicKeyPEM": pair.public_pem
        });
        let path = crate::tool_paths::trae_storage_path_for(TraePlatformKind::Trae).unwrap();
        assert!(path.starts_with(dir.path()), "{path:?}");
        assert!(path.ends_with("storage.json"));
        write_storage(&path, &cockpit_storage(&auth, Some(&device_json)));

        let imported = read_local(TraePlatformKind::Trae).unwrap();
        assert_eq!(imported.access_token, "access-from-disk");
        assert_eq!(imported.refresh_token.as_deref(), Some("refresh-from-disk"));
        assert_eq!(imported.oauth_account_id.as_deref(), Some("user-7"));
        assert_eq!(imported.display_name, "trae@example.com");
        let state: Value =
            serde_json::from_str(imported.provider_state.as_deref().unwrap()).unwrap();
        assert_eq!(
            state["deviceKeyPair"]["privateKeyPEM"]
                .as_str()
                .unwrap()
                .trim(),
            pair.private_pem.trim()
        );
        assert_eq!(
            state["deviceKeyPair"]["publicKeyPEM"]
                .as_str()
                .unwrap()
                .trim(),
            pair.public_pem.trim()
        );
        assert_eq!(state["clientId"], "ono9krqynydwx5");
        assert_eq!(state["loginHost"], "https://grow-normal.trae.ai");
        assert_eq!(state["authDomain"], "www.trae.ai");

        let Err(solo) = read_local(TraePlatformKind::TraeSolo) else {
            panic!("solo storage should be missing");
        };
        assert!(solo.to_string().contains("storage.json"), "{solo}");
        assert!(solo.to_string().contains("TRAE SOLO"), "{solo}");

        let mut tampered = cockpit_storage(&auth, Some(&device_json));
        let cipher = tampered[AUTH_KEY].as_str().unwrap().to_string();
        let mut bytes = BASE64.decode(cipher).unwrap();
        let index = bytes.len() - 1;
        bytes[index] ^= 0x01;
        tampered[AUTH_KEY] = Value::String(BASE64.encode(bytes));
        write_storage(&path, &tampered);
        let Err(err) = read_local(TraePlatformKind::Trae) else {
            panic!("tampered storage should fail");
        };
        assert!(
            err.to_string().contains("解密") || err.to_string().contains("完整性"),
            "{err}"
        );
    }
}

//! On-disk ZCode credential files: enc:v1 fields, API-key config, and backups.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use skillstar_models::tool_sync::create_rolling_backup;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::tool_store::enc_v1::{
    decrypt_enc_v1, encrypt_enc_v1, zcode_credential_key, zcode_credentials_path,
};
use skillstar_usage::{UsageError, UsageResult, crypto, tool_paths};

use super::{
    ACTIVE, ApiLive, BUILTIN_BIGMODEL, BUILTIN_ZAI, FAMILY_MODES, JWT, OauthLive, OauthMaterial,
    OauthState, Write,
};

pub(super) fn apply_oauth(
    map: &mut Map<String, Value>,
    material: &OauthMaterial,
    key: &[u8; 32],
) -> UsageResult<()> {
    map.insert(
        ACTIVE.to_string(),
        Value::String(seal(key, &material.provider)?),
    );
    map.insert(
        access_key(&material.provider),
        Value::String(seal(key, &material.access)?),
    );
    let refresh_name = refresh_key(&material.provider);
    if let Some(refresh) = &material.refresh {
        map.insert(refresh_name, Value::String(seal(key, refresh)?));
    } else {
        map.remove(&refresh_name);
    }
    map.insert(JWT.to_string(), Value::String(seal(key, &material.jwt)?));
    map.insert(
        user_key(&material.provider),
        Value::String(seal(key, &material.user_info)?),
    );
    Ok(())
}

pub(super) fn apply_api(
    map: &mut Map<String, Value>,
    provider: &str,
    api_key: &str,
) -> UsageResult<()> {
    let providers = object_slot(
        map,
        "providers",
        "ZCode config.json 的 providers 不是对象，切换未生效",
    )?;
    let slot = object_slot(
        providers,
        builtin_id(provider),
        "ZCode config.json 的 provider 不是对象，切换未生效",
    )?;
    slot.insert("enabled".to_string(), Value::Bool(true));
    let options = object_slot(
        slot,
        "options",
        "ZCode config.json 的 options 不是对象，切换未生效",
    )?;
    options.insert("apiKey".to_string(), Value::String(api_key.to_string()));
    Ok(())
}

pub(super) fn with_mode(
    mut root: Map<String, Value>,
    provider: &str,
    mode: &str,
) -> UsageResult<Map<String, Value>> {
    let modes = object_slot(
        &mut root,
        FAMILY_MODES,
        "ZCode setting.json 的 modelProviderFamilyModes 不是对象，切换未生效",
    )?;
    modes.insert(provider.to_string(), Value::String(mode.to_string()));
    Ok(root)
}

fn object_slot<'a>(
    map: &'a mut Map<String, Value>,
    key: &str,
    invalid: &str,
) -> UsageResult<&'a mut Map<String, Value>> {
    if map.get(key).is_some_and(|value| !value.is_object()) {
        return Err(UsageError::Other(invalid.into()));
    }
    let value = map
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    value
        .as_object_mut()
        .ok_or_else(|| UsageError::Other(invalid.into()))
}

pub(super) fn strip_oauth(
    map: &mut Map<String, Value>,
    subscription: &Subscription,
    key: &[u8; 32],
) -> UsageResult<bool> {
    let provider = provider_of(subscription)?;
    let Some(access) = plain(&subscription.access_token_encrypted) else {
        return Ok(false);
    };
    let Some(live_access) = decrypt_optional(map, &access_key(&provider), key)? else {
        return Ok(false);
    };
    if live_access != access {
        return Ok(false);
    }
    map.remove(&access_key(&provider));
    map.remove(&refresh_key(&provider));
    map.remove(&user_key(&provider));
    if plain(&subscription.id_token_encrypted).as_deref()
        == decrypt_optional(map, JWT, key)?.as_deref()
    {
        map.remove(JWT);
    }
    if decrypt_optional(map, ACTIVE, key)?.as_deref() == Some(provider.as_str()) {
        map.remove(ACTIVE);
    }
    Ok(true)
}

pub(super) fn strip_api(
    map: &mut Map<String, Value>,
    subscription: &Subscription,
) -> UsageResult<bool> {
    let provider = provider_of(subscription)?;
    let Some(api_key) = plain(&subscription.api_key_encrypted) else {
        return Ok(false);
    };
    let id = builtin_id(&provider);
    let drop_slot = {
        let Some(providers) = map.get_mut("providers").and_then(Value::as_object_mut) else {
            return Ok(false);
        };
        let Some(slot) = providers.get_mut(id).and_then(Value::as_object_mut) else {
            return Ok(false);
        };
        let Some(options) = slot.get_mut("options").and_then(Value::as_object_mut) else {
            return Ok(false);
        };
        if options.get("apiKey").and_then(Value::as_str) != Some(api_key.as_str()) {
            return Ok(false);
        }
        options.remove("apiKey");
        if options.is_empty() {
            slot.remove("options");
        }
        slot.is_empty()
            || (slot.len() == 1 && slot.get("enabled").and_then(Value::as_bool) == Some(true))
    };
    if drop_slot && let Some(providers) = map.get_mut("providers").and_then(Value::as_object_mut) {
        providers.remove(id);
        if providers.is_empty() {
            map.remove("providers");
        }
    }
    Ok(true)
}

pub(super) fn verify_oauth(
    path: &Path,
    material: &OauthMaterial,
    key: &[u8; 32],
) -> UsageResult<()> {
    let map = read_object(path)?;
    let provider = required_field(&map, ACTIVE, key)?;
    let access = required_field(&map, &access_key(&material.provider), key)?;
    let jwt = required_field(&map, JWT, key)?;
    let refresh = decrypt_optional(&map, &refresh_key(&material.provider), key)?;
    let user_info = required_field(&map, &user_key(&material.provider), key)?;
    if provider != material.provider
        || access != material.access
        || jwt != material.jwt
        || refresh != material.refresh
        || user_info != material.user_info
    {
        return Err(readback_error());
    }
    Ok(())
}

pub(super) fn verify_api(path: &Path, provider: &str, api_key: &str) -> UsageResult<()> {
    let root = read_object(path)?;
    let found = root
        .get("providers")
        .and_then(|value| value.get(builtin_id(provider)))
        .and_then(|value| value.get("options"))
        .and_then(|value| value.get("apiKey"))
        .and_then(Value::as_str);
    if found != Some(api_key) {
        Err(readback_error())
    } else {
        Ok(())
    }
}

pub(super) fn verify_mode(provider: &str, mode: &str) -> UsageResult<()> {
    if read_modes()?.get(provider).map(String::as_str) == Some(mode) {
        Ok(())
    } else {
        Err(readback_error())
    }
}

pub(super) fn read_oauth_state() -> UsageResult<OauthState> {
    let path = credentials_path();
    if !path.is_file() {
        return Ok(OauthState::Absent);
    }
    let map = read_object(&path)?;
    let key = credential_key();
    let Some(provider) = decrypt_optional(&map, ACTIVE, &key)? else {
        return Ok(OauthState::Absent);
    };
    let provider = provider.trim().to_ascii_lowercase();
    if provider != "zai" && provider != "bigmodel" {
        return Ok(OauthState::Incomplete { provider });
    }
    let Some(access) = decrypt_optional(&map, &access_key(&provider), &key)? else {
        return Ok(OauthState::Incomplete { provider });
    };
    if access.is_empty() {
        return Ok(OauthState::Incomplete { provider });
    }
    let refresh = decrypt_optional(&map, &refresh_key(&provider), &key)?;
    let jwt = decrypt_optional(&map, JWT, &key)?.unwrap_or_default();
    let user_id = decrypt_optional(&map, &user_key(&provider), &key)?
        .as_deref()
        .and_then(user_id_from_info);
    Ok(OauthState::Session(OauthLive {
        provider,
        access,
        refresh,
        jwt,
        user_id,
    }))
}

pub(super) fn read_api_keys() -> UsageResult<Vec<ApiLive>> {
    let path = config_path();
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let root = read_object(&path)?;
    let Some(providers) = root.get("providers").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    for (id, provider) in [(BUILTIN_ZAI, "zai"), (BUILTIN_BIGMODEL, "bigmodel")] {
        let Some(slot) = providers.get(id).and_then(Value::as_object) else {
            continue;
        };
        if slot.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let Some(api_key) = slot
            .get("options")
            .and_then(|value| value.get("apiKey"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        found.push(ApiLive {
            provider: provider.to_string(),
            api_key: api_key.to_string(),
        });
    }
    Ok(found)
}

type FamilyModes = BTreeMap<String, String>;

pub(super) fn read_modes() -> UsageResult<FamilyModes> {
    let path = settings_path();
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let root = read_object(&path)?;
    let Some(modes) = root.get(FAMILY_MODES) else {
        return Ok(BTreeMap::new());
    };
    let Some(modes) = modes.as_object() else {
        return Err(UsageError::Other(
            "ZCode setting.json 的 modelProviderFamilyModes 不是对象，切换未生效".into(),
        ));
    };
    let mut out = BTreeMap::new();
    for (family, value) in modes {
        let Some(mode) = value
            .as_str()
            .map(str::trim)
            .filter(|mode| !mode.is_empty())
        else {
            continue;
        };
        let family = family.trim().to_ascii_lowercase();
        if family == "zai" || family == "bigmodel" {
            out.insert(family, mode.to_string());
        }
    }
    Ok(out)
}

pub(super) fn commit(
    writes: &[Write],
    verify: impl Fn() -> UsageResult<()>,
) -> UsageResult<Option<PathBuf>> {
    let mut backups = Vec::with_capacity(writes.len());
    for write in writes {
        let backup = if write.path.is_file() {
            Some(backup_file(&write.path)?)
        } else {
            None
        };
        backups.push((write.path.clone(), backup));
    }
    let primary = backups.first().and_then(|(_, backup)| backup.clone());
    let written = (|| {
        for write in writes {
            atomic_replace(&write.path, &write.bytes)?;
            tighten(&write.path);
        }
        if readback_forced_failure() {
            return Err(readback_error());
        }
        verify()
    })();
    if let Err(error) = written {
        return Err(combine(error, restore_all(&backups)));
    }
    Ok(primary)
}

pub(super) fn remove_verified(path: &Path) -> UsageResult<()> {
    let backup = backup_file(path)?;
    let removed = (|| {
        std::fs::remove_file(path)
            .map_err(|err| UsageError::Other(format!("删除 ZCode 凭据失败：{err}")))?;
        if readback_forced_failure() || path.exists() {
            return Err(readback_error());
        }
        Ok(())
    })();
    if let Err(error) = removed {
        return Err(combine(error, restore_backup(path, &backup)));
    }
    Ok(())
}

fn restore_all(entries: &[(PathBuf, Option<PathBuf>)]) -> Result<(), UsageError> {
    let mut first = None;
    for (path, backup) in entries.iter().rev() {
        let result = match backup {
            Some(backup) => restore_backup(path, backup),
            None => {
                if path.exists() {
                    std::fs::remove_file(path).map_err(|err| {
                        UsageError::Other(format!("回滚 ZCode 凭据失败：{err}（删除新建文件）"))
                    })
                } else {
                    Ok(())
                }
            }
        };
        if let Err(error) = result
            && first.is_none()
        {
            first = Some(error);
        }
    }
    match first {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn backup_file(path: &Path) -> UsageResult<PathBuf> {
    let backup = create_rolling_backup(path)
        .map_err(|err| UsageError::Other(format!("备份 ZCode 凭据失败：{err}")))?;
    tighten(&backup);
    Ok(backup)
}

fn restore_backup(live: &Path, backup: &Path) -> UsageResult<()> {
    if let Some(parent) = live.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            UsageError::Other(format!(
                "回滚 ZCode 凭据失败：{err}（备份 {}）",
                backup.display()
            ))
        })?;
    }
    std::fs::copy(backup, live).map_err(|err| {
        UsageError::Other(format!(
            "回滚 ZCode 凭据失败：{err}（备份 {}）",
            backup.display()
        ))
    })?;
    tighten(live);
    Ok(())
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> UsageResult<()> {
    skillstar_core::infra::fs_ops::atomic_write(path, bytes)
        .map_err(|err| UsageError::Other(format!("写入 ZCode 凭据失败：{err}")))
}

fn combine(error: UsageError, restore: Result<(), UsageError>) -> UsageError {
    match restore {
        Ok(()) => error,
        Err(restore) => UsageError::Other(format!("{error}；{restore}")),
    }
}

pub(super) fn read_object(path: &Path) -> UsageResult<Map<String, Value>> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| UsageError::Other(format!("读取 ZCode 凭据失败：{err}")))?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(UsageError::Other(
            "ZCode 凭据文件不是 JSON 对象，切换未生效".into(),
        )),
        Err(_) => Err(UsageError::Other(
            "ZCode 凭据文件无法解析，切换未生效".into(),
        )),
    }
}

pub(super) fn to_bytes(value: &Value) -> UsageResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn required_field(map: &Map<String, Value>, name: &str, key: &[u8; 32]) -> UsageResult<String> {
    decrypt_optional(map, name, key)?
        .ok_or_else(|| UsageError::Other(format!("ZCode 凭据回读缺少 {name}，切换未生效")))
}

fn decrypt_optional(
    map: &Map<String, Value>,
    name: &str,
    key: &[u8; 32],
) -> UsageResult<Option<String>> {
    let Some(raw) = map.get(name).and_then(Value::as_str) else {
        return Ok(None);
    };
    decrypt_enc_v1(key, raw)
        .map(Some)
        .map_err(|err| UsageError::Other(err.to_string()))
}

fn seal(key: &[u8; 32], plaintext: &str) -> UsageResult<String> {
    encrypt_enc_v1(key, plaintext).map_err(|err| UsageError::Other(err.to_string()))
}

pub(super) fn credentials_path() -> PathBuf {
    zcode_credentials_path()
}

pub(super) fn config_path() -> PathBuf {
    tool_paths::zcode_home().join("v2").join("config.json")
}

pub(super) fn settings_path() -> PathBuf {
    tool_paths::zcode_home().join("v2").join("setting.json")
}

pub(super) fn display_path(subscription: &Subscription) -> PathBuf {
    if is_api_key(subscription) {
        config_path()
    } else {
        credentials_path()
    }
}

/// Same inputs as `fetchers/oauth/zcode/import.rs`: sandbox home replaces the
/// OS home, and `dataBaseDir` is not hashed.
pub(super) fn credential_key() -> [u8; 32] {
    zcode_credential_key(&credential_key_home(), &os_username())
}

fn credential_key_home() -> PathBuf {
    if tool_paths::is_tool_sync_sandboxed() {
        return std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(skillstar_core::infra::paths::home_dir);
    }
    skillstar_core::infra::paths::home_dir()
}

pub(super) fn os_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

pub(super) fn provider_of(subscription: &Subscription) -> UsageResult<String> {
    match subscription
        .oauth_region
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "zai" => Ok("zai".to_string()),
        "bigmodel" => Ok("bigmodel".to_string()),
        "" => Err(UsageError::Other("ZCode 账号缺少上游，切换未生效".into())),
        _ => Err(UsageError::Other("不支持的 ZCode 上游，切换未生效".into())),
    }
}

pub(super) fn is_api_key(subscription: &Subscription) -> bool {
    let plain = crypto::decrypt(
        subscription
            .provider_state_encrypted
            .as_deref()
            .unwrap_or(""),
    );
    serde_json::from_str::<Value>(&plain)
        .ok()
        .and_then(|value| {
            value
                .get("kind")
                .and_then(Value::as_str)
                .map(|kind| kind == "api_key")
        })
        .unwrap_or(false)
}

pub(super) fn user_info_json(subscription: &Subscription) -> String {
    let mut map = Map::new();
    if let Some(id) = nonempty(subscription.oauth_account_id.as_deref()) {
        map.insert("user_id".to_string(), Value::String(id.clone()));
        map.insert("id".to_string(), Value::String(id));
    }
    if let Some(name) = nonempty(Some(subscription.display_name.as_str())) {
        if name.contains('@') && name.len() > 3 && !name.contains(' ') {
            map.insert("email".to_string(), Value::String(name));
        } else if name != "ZCode" {
            map.insert("name".to_string(), Value::String(name));
        }
    }
    Value::Object(map).to_string()
}

fn user_id_from_info(raw: &str) -> Option<String> {
    let value: Value = serde_json::from_str(raw).ok()?;
    for key in ["user_id", "id", "customerNumber", "sub"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub(super) fn plain(value: &Option<String>) -> Option<String> {
    let text = crypto::decrypt(value.as_deref().unwrap_or(""));
    nonempty(Some(text.as_str()))
}

pub(super) fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

pub(super) fn access_key(provider: &str) -> String {
    format!("oauth:{provider}:access_token")
}

fn refresh_key(provider: &str) -> String {
    format!("oauth:{provider}:refresh_token")
}

fn user_key(provider: &str) -> String {
    format!("oauth:{provider}:user_info")
}

pub(super) fn builtin_id(provider: &str) -> &'static str {
    if provider == "bigmodel" {
        BUILTIN_BIGMODEL
    } else {
        BUILTIN_ZAI
    }
}

pub(super) fn readback_error() -> UsageError {
    UsageError::Other("ZCode 凭据回读与写入不一致，切换未生效".into())
}

fn readback_forced_failure() -> bool {
    #[cfg(test)]
    {
        std::env::var_os(super::READBACK_FAIL_ENV).is_some()
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

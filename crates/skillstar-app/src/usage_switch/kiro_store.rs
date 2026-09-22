//! On-disk Kiro credential files. The adapter in `kiro` decides when to
//! call these; this module only shapes JSON and rolls a backup back.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use sha1::{Digest, Sha1};
use skillstar_models::tool_sync::create_rolling_backup;
use skillstar_usage::crypto;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageError, UsageResult, storage, tool_paths, vscdb};

use super::{AUTH_FILE, BUILDER_ID_START_URL, CATALOG_ID, HASH_LEN, USAGE_DB_KEY};

pub(super) fn token_document(
    material: &Material,
    idc: bool,
    provider: Option<&str>,
    auth_method: Option<&str>,
) -> Value {
    let mut map = Map::new();
    insert(&mut map, "accessToken", material.access.as_deref());
    insert(&mut map, "refreshToken", material.refresh.as_deref());
    if let Some(expires) = material.expires_at.and_then(expires_rfc3339) {
        map.insert("expiresAt".to_string(), Value::String(expires));
    }
    insert(&mut map, "profileArn", material.profile_arn.as_deref());
    insert(&mut map, "provider", provider);
    insert(&mut map, "loginProvider", provider);
    insert(&mut map, "authMethod", auth_method);
    if idc {
        insert(&mut map, "region", material.region.as_deref());
        insert(&mut map, "idcRegion", material.region.as_deref());
        insert(&mut map, "idc_region", material.region.as_deref());
        insert(&mut map, "issuerUrl", material.start_url.as_deref());
        insert(&mut map, "issuer_url", material.start_url.as_deref());
        insert(&mut map, "clientId", material.client_id.as_deref());
        insert(&mut map, "client_id", material.client_id.as_deref());
        if let Some(start) = material.start_url.as_deref() {
            map.insert(
                "clientIdHash".to_string(),
                Value::String(client_id_hash(start)),
            );
        }
    } else {
        insert(&mut map, "region", material.region.as_deref());
    }
    insert(&mut map, "tokenType", material.token_type.as_deref());
    insert(&mut map, "scopes", material.scopes.as_deref());
    insert(&mut map, "scope", material.scopes.as_deref());
    insert(&mut map, "loginHint", material.login_hint.as_deref());
    insert(&mut map, "login_hint", material.login_hint.as_deref());
    insert(&mut map, "email", material.email.as_deref());
    insert(&mut map, "userId", material.user_id.as_deref());
    insert(&mut map, "user_id", material.user_id.as_deref());
    map.remove("clientSecret");
    map.remove("client_secret");
    map.remove("clientRegistration");
    map.remove("client_registration");
    Value::Object(map)
}

pub(super) fn registration_document(path: &Path, client_id: &str, client_secret: &str) -> Value {
    let mut map = read_object(path).unwrap_or_default();
    let same = field(&map, &["clientId", "client_id"]).as_deref() == Some(client_id);
    if !same {
        map.clear();
    }
    map.insert("clientId".to_string(), Value::String(client_id.to_string()));
    map.insert(
        "clientSecret".to_string(),
        Value::String(client_secret.to_string()),
    );
    if map.contains_key("client_id") {
        map.insert(
            "client_id".to_string(),
            Value::String(client_id.to_string()),
        );
    }
    map.remove("client_secret");
    Value::Object(map)
}

pub(super) fn profile_document(material: &Material, provider: Option<&str>) -> Option<Value> {
    let mut map = Map::new();
    let arn = material
        .profile_arn
        .clone()
        .or_else(|| material.user_id.clone())
        .or_else(|| material.email.clone());
    insert(&mut map, "arn", arn.as_deref());
    insert(
        &mut map,
        "name",
        profile_name(material, provider).as_deref(),
    );
    insert(&mut map, "email", material.email.as_deref());
    insert(&mut map, "userId", material.user_id.as_deref());
    insert(&mut map, "loginProvider", provider);
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map))
    }
}

pub(super) fn profile_name(material: &Material, provider: Option<&str>) -> Option<String> {
    if let Some(email) = material.email.clone() {
        return Some(email);
    }
    if let Some(provider) = provider.map(str::trim).filter(|value| !value.is_empty()) {
        return Some(provider.to_string());
    }
    let name = material.display_name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("kiro") || looks_like_email(name) {
        None
    } else {
        Some(name.to_string())
    }
}

pub(super) fn usage_document(material: &Material) -> Option<String> {
    let mut user = Map::new();
    insert(&mut user, "email", material.email.as_deref());
    insert(&mut user, "userId", material.user_id.as_deref());
    let mut root = Map::new();
    if !user.is_empty() {
        root.insert("userInfo".to_string(), Value::Object(user));
    }
    insert(&mut root, "profileArn", material.profile_arn.as_deref());
    if root.is_empty() {
        None
    } else {
        Some(Value::Object(root).to_string())
    }
}

pub(super) fn login_labels(material: &Material, idc: bool) -> (Option<String>, Option<String>) {
    if idc {
        let provider = material
            .provider
            .as_deref()
            .map(normalize_provider)
            .filter(|value| !is_social(value))
            .or_else(|| {
                Some(
                    if material.start_url.as_deref() == Some(BUILDER_ID_START_URL) {
                        "BuilderId".to_string()
                    } else {
                        "Enterprise".to_string()
                    },
                )
            });
        return (provider, Some("IdC".to_string()));
    }
    let provider = material.provider.as_deref().map(normalize_provider);
    let auth_method = if provider.as_deref().is_some_and(is_social) {
        Some("social".to_string())
    } else {
        material.auth_method.clone()
    };
    (provider, auth_method)
}
pub(super) struct FileBackup {
    path: PathBuf,
    backup: Option<PathBuf>,
}

impl FileBackup {
    fn capture(path: &Path) -> UsageResult<Self> {
        if !path.is_file() {
            return Ok(Self {
                path: path.to_path_buf(),
                backup: None,
            });
        }
        let backup = create_rolling_backup(path)
            .map_err(|err| UsageError::Other(format!("备份 {} 失败：{err}", path.display())))?;
        tighten(&backup);
        Ok(Self {
            path: path.to_path_buf(),
            backup: Some(backup),
        })
    }

    fn restore(&self) -> UsageResult<()> {
        match &self.backup {
            Some(backup) => {
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent).map_err(|err| {
                        UsageError::Other(format!("恢复 {} 失败：{err}", self.path.display()))
                    })?;
                }
                std::fs::copy(backup, &self.path).map_err(|err| {
                    UsageError::Other(format!(
                        "回滚 {} 失败：{err}（备份 {}）",
                        self.path.display(),
                        backup.display()
                    ))
                })?;
                tighten(&self.path);
                Ok(())
            }
            None => {
                if self.path.exists() {
                    std::fs::remove_file(&self.path).map_err(|err| {
                        UsageError::Other(format!("回滚删除 {} 失败：{err}", self.path.display()))
                    })?;
                }
                Ok(())
            }
        }
    }
}

pub(super) fn capture_plan(plan: &super::Plan) -> UsageResult<Vec<FileBackup>> {
    let mut paths = Vec::new();
    if let Some((path, _)) = &plan.registration {
        paths.push(path.as_path());
    }
    paths.push(plan.token_path.as_path());
    if let Some((path, _)) = &plan.profile {
        paths.push(path.as_path());
    }
    if let Some((path, _)) = &plan.database {
        paths.push(path.as_path());
    }
    capture_paths(paths)
}

pub(super) fn capture_paths<'a>(
    paths: impl IntoIterator<Item = &'a Path>,
) -> UsageResult<Vec<FileBackup>> {
    let mut backups = Vec::new();
    for path in paths {
        backups.push(FileBackup::capture(path)?);
    }
    Ok(backups)
}

pub(super) fn restore_all(backups: &[FileBackup], error: UsageError) -> UsageError {
    let mut failed = Vec::new();
    for item in backups.iter().rev() {
        if let Err(restore) = item.restore() {
            failed.push(restore.to_string());
        }
    }
    if failed.is_empty() {
        error
    } else {
        UsageError::Other(format!("{error}；{}", failed.join("；")))
    }
}

pub(super) fn success_outcome(
    plan: &super::Plan,
    backups: &[FileBackup],
) -> crate::usage_switch::SwitchOutcome {
    let backup = backups
        .iter()
        .filter(|item| item.path == plan.token_path)
        .find_map(|item| item.backup.clone())
        .or_else(|| backups.iter().find_map(|item| item.backup.clone()));
    crate::usage_switch::SwitchOutcome {
        tool_id: CATALOG_ID.to_string(),
        config_path: plan.token_path.display().to_string(),
        backup_path: backup.map(|path| path.display().to_string()),
        keychain_updated: false,
        link_mode: None,
        success: true,
        error: None,
    }
}

pub(super) fn write_json(path: &Path, value: &Value) -> UsageResult<()> {
    let raw = serde_json::to_string_pretty(value)
        .map_err(|err| UsageError::Other(format!("序列化 Kiro 授权文件失败：{err}")))?;
    skillstar_core::infra::fs_ops::atomic_write(path, raw.as_bytes())
        .map_err(|err| UsageError::Other(format!("写入 {} 失败：{err}", path.display())))?;
    tighten(path);
    Ok(())
}

pub(super) fn read_value(path: &Path) -> UsageResult<Value> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| UsageError::Other(format!("读取 {} 失败：{err}", path.display())))?;
    serde_json::from_str(&raw)
        .map_err(|_| UsageError::Other(format!("无法解析 {}", path.display())))
}

pub(super) fn read_object(path: &Path) -> Option<Map<String, Value>> {
    read_value(path).ok()?.as_object().cloned()
}

pub(super) fn tighten(path: &Path) {
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

pub(super) fn readback_error() -> UsageError {
    UsageError::Other("Kiro 授权文件回读校验失败，切换未生效".into())
}

pub(super) fn readback_forced_failure() -> bool {
    #[cfg(test)]
    {
        std::env::var_os(super::READBACK_FAIL_ENV).is_some()
    }
    #[cfg(not(test))]
    {
        false
    }
}

pub(super) struct Material {
    pub(super) access: Option<String>,
    pub(super) refresh: Option<String>,
    pub(super) expires_at: Option<i64>,
    pub(super) email: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) display_name: String,
    pub(super) client_id: Option<String>,
    pub(super) client_secret: Option<String>,
    pub(super) region: Option<String>,
    pub(super) start_url: Option<String>,
    pub(super) profile_arn: Option<String>,
    pub(super) provider: Option<String>,
    pub(super) auth_method: Option<String>,
    pub(super) token_type: Option<String>,
    pub(super) scopes: Option<String>,
    pub(super) login_hint: Option<String>,
}

pub(super) fn material_of(subscription: &Subscription) -> Material {
    let map = provider_map(subscription);
    Material {
        access: secret(subscription.access_token_encrypted.as_deref()),
        refresh: secret(subscription.refresh_token_encrypted.as_deref()),
        expires_at: subscription.access_token_expires_at,
        email: email_of(subscription),
        user_id: user_id_of(subscription),
        display_name: subscription.display_name.clone(),
        client_id: field(&map, &["clientId", "client_id"]),
        client_secret: field(&map, &["clientSecret", "client_secret"]),
        region: field(&map, &["region", "idcRegion", "idc_region"]).or_else(|| {
            subscription
                .oauth_region
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        }),
        start_url: field(
            &map,
            &["startUrl", "start_url", "issuerUrl", "issuer_url", "issuer"],
        ),
        profile_arn: field(&map, &["profileArn", "profile_arn"]),
        provider: field(&map, &["provider", "loginProvider"]),
        auth_method: field(&map, &["authMethod", "auth_method"]),
        token_type: field(&map, &["tokenType", "token_type"]),
        scopes: field(&map, &["scopes", "scope"]),
        login_hint: field(&map, &["loginHint", "login_hint"]),
    }
}

pub(super) struct LiveAuth {
    pub(super) access: Option<String>,
    pub(super) refresh: Option<String>,
    pub(super) expires_at: Option<i64>,
    pub(super) client_id_hash: Option<String>,
    pub(super) profile_arn: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) email: Option<String>,
    pub(super) region: Option<String>,
    pub(super) start_url: Option<String>,
    pub(super) client_id: Option<String>,
    pub(super) client_secret: Option<String>,
}

pub(super) fn read_live() -> UsageResult<Option<LiveAuth>> {
    let path = auth_token_path();
    if !path.is_file() {
        return Ok(None);
    }
    let value = read_value(&path)?;
    let Some(map) = value.as_object() else {
        return Err(UsageError::Other("Kiro 本地授权文件须是 JSON 对象".into()));
    };
    let access = field(map, &["accessToken", "access_token", "token"]);
    let refresh = field(map, &["refreshToken", "refresh_token"]);
    if access.is_none() && refresh.is_none() {
        return Ok(None);
    }
    let mut live = LiveAuth {
        access,
        refresh,
        expires_at: parse_expires(&value),
        client_id_hash: field(map, &["clientIdHash", "client_id_hash"]),
        profile_arn: field(map, &["profileArn", "profile_arn", "arn"]),
        user_id: field(map, &["userId", "user_id", "sub"]),
        email: field(map, &["email", "userEmail"]),
        region: field(map, &["region", "idcRegion", "idc_region"]),
        start_url: field(
            map,
            &["issuerUrl", "issuer_url", "issuer", "startUrl", "start_url"],
        ),
        client_id: field(map, &["clientId", "client_id"]),
        client_secret: field(map, &["clientSecret", "client_secret"]),
    };
    if let Some(hash) = live.client_id_hash.clone()
        && let Some(path) = registration_path(&cache_dir(), &hash)
        && let Some(registration) = read_object(&path)
    {
        if let Some(client_id) = field(&registration, &["clientId", "client_id"]) {
            live.client_id = Some(client_id);
        }
        if let Some(secret) = field(&registration, &["clientSecret", "client_secret"]) {
            live.client_secret = Some(secret);
        }
    }
    enrich_identity(&mut live);
    Ok(Some(live))
}

pub(super) fn enrich_identity(live: &mut LiveAuth) {
    if live.user_id.is_some() && live.email.is_some() && live.profile_arn.is_some() {
        return;
    }
    if let Some(path) = profile_path()
        && let Some(map) = read_object(&path)
    {
        if live.email.is_none() {
            live.email = field(&map, &["email"]);
        }
        if live.user_id.is_none() {
            live.user_id = field(&map, &["userId", "user_id", "id", "sub"]);
        }
        if live.profile_arn.is_none() {
            live.profile_arn = field(&map, &["arn", "profileArn", "profile_arn"]);
        }
    }
    if live.user_id.is_some() && live.email.is_some() && live.profile_arn.is_some() {
        return;
    }
    let Some(path) = state_db_path() else {
        return;
    };
    let Ok(Some(raw)) = vscdb::read_item_string(&path, USAGE_DB_KEY) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return;
    };
    let user = value.get("userInfo");
    if live.email.is_none() {
        live.email =
            field_value(user, &["email"]).or_else(|| field_value(Some(&value), &["email"]));
    }
    if live.user_id.is_none() {
        live.user_id = field_value(user, &["userId", "user_id", "sub"])
            .or_else(|| field_value(Some(&value), &["userId", "user_id", "sub"]));
    }
    if live.profile_arn.is_none() {
        live.profile_arn = field_value(Some(&value), &["profileArn", "profile_arn", "arn"]);
    }
}

pub(super) fn matching_subscription(live: &LiveAuth) -> UsageResult<Option<Subscription>> {
    let subscriptions = storage::list_subscriptions()?;
    let rows: Vec<_> = subscriptions
        .into_iter()
        .filter(|subscription| subscription.catalog_id == CATALOG_ID)
        .collect();
    if let Some(found) = rows
        .iter()
        .find(|subscription| token_match(subscription, live))
    {
        return Ok(Some(found.clone()));
    }
    Ok(rows
        .into_iter()
        .find(|subscription| identity_match(subscription, live)))
}

pub(super) fn same_account(subscription: &Subscription, live: &LiveAuth) -> bool {
    token_match(subscription, live) || identity_match(subscription, live)
}

pub(super) fn token_match(subscription: &Subscription, live: &LiveAuth) -> bool {
    eq_opt(
        secret(subscription.refresh_token_encrypted.as_deref()).as_deref(),
        live.refresh.as_deref(),
    ) || eq_opt(
        secret(subscription.access_token_encrypted.as_deref()).as_deref(),
        live.access.as_deref(),
    )
}

pub(super) fn identity_match(subscription: &Subscription, live: &LiveAuth) -> bool {
    let material = material_of(subscription);
    eq_opt(material.profile_arn.as_deref(), live.profile_arn.as_deref())
        || eq_opt(material.user_id.as_deref(), live.user_id.as_deref())
}

pub(super) fn absorb(subscription: &Subscription, live: &LiveAuth) -> Subscription {
    let mut updated = subscription.clone();
    assign_secret(&mut updated.access_token_encrypted, live.access.as_deref());
    assign_secret(
        &mut updated.refresh_token_encrypted,
        live.refresh.as_deref(),
    );
    if let Some(expires_at) = live.expires_at {
        updated.access_token_expires_at = Some(expires_at);
    }
    if let Some(user_id) = live.user_id.clone().filter(|value| !value.is_empty()) {
        updated.oauth_account_id = Some(user_id);
    }
    if let Some(json) = projected_provider(subscription, live) {
        assign_provider(&mut updated.provider_state_encrypted, Some(json));
    }
    updated
}

pub(super) fn projected_provider(subscription: &Subscription, live: &LiveAuth) -> Option<String> {
    let existing = provider_map(subscription);
    let mut map = Map::new();
    let client_id = live
        .client_id
        .clone()
        .or_else(|| field(&existing, &["clientId", "client_id"]));
    let client_secret = live
        .client_secret
        .clone()
        .or_else(|| field(&existing, &["clientSecret", "client_secret"]));
    let region = live
        .region
        .clone()
        .or_else(|| field(&existing, &["region", "idcRegion", "idc_region"]));
    let start_url = live.start_url.clone().or_else(|| {
        field(
            &existing,
            &["startUrl", "start_url", "issuerUrl", "issuer_url", "issuer"],
        )
    });
    let profile_arn = live
        .profile_arn
        .clone()
        .or_else(|| field(&existing, &["profileArn", "profile_arn"]));
    insert(&mut map, "clientId", client_id.as_deref());
    insert(&mut map, "clientSecret", client_secret.as_deref());
    insert(&mut map, "region", region.as_deref());
    insert(&mut map, "startUrl", start_url.as_deref());
    insert(&mut map, "profileArn", profile_arn.as_deref());
    if let Some(provider) = field(&existing, &["provider"]) {
        insert(&mut map, "provider", Some(provider.as_str()));
    }
    if let Some(method) = field(&existing, &["authMethod", "auth_method"]) {
        insert(&mut map, "authMethod", Some(method.as_str()));
    }
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map).to_string())
    }
}

pub(super) fn credentials_changed(updated: &Subscription, original: &Subscription) -> bool {
    updated.access_token_encrypted != original.access_token_encrypted
        || updated.refresh_token_encrypted != original.refresh_token_encrypted
        || updated.access_token_expires_at != original.access_token_expires_at
        || updated.oauth_account_id != original.oauth_account_id
        || updated.provider_state_encrypted != original.provider_state_encrypted
}

pub(super) fn assign_secret(slot: &mut Option<String>, plain: Option<&str>) {
    let Some(plain) = plain.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let current = slot.as_deref().map(crypto::decrypt).unwrap_or_default();
    if current != plain {
        *slot = Some(crypto::encrypt(plain));
    }
}

pub(super) fn assign_provider(slot: &mut Option<String>, next: Option<String>) {
    let Some(next) = next.filter(|value| !value.is_empty()) else {
        return;
    };
    let current = slot.as_deref().map(crypto::decrypt).unwrap_or_default();
    if !provider_equivalent(&current, &next) {
        *slot = Some(crypto::encrypt(&next));
    }
}

pub(super) fn provider_equivalent(current: &str, next: &str) -> bool {
    if current == next {
        return true;
    }
    match (
        serde_json::from_str::<Value>(current.trim()),
        serde_json::from_str::<Value>(next.trim()),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub(super) fn provider_map(subscription: &Subscription) -> Map<String, Value> {
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

pub(super) fn secret(value: Option<&str>) -> Option<String> {
    value
        .map(crypto::decrypt)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

pub(super) fn email_of(subscription: &Subscription) -> Option<String> {
    [
        Some(subscription.display_name.as_str()),
        subscription.oauth_account_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| looks_like_email(value))
    .map(str::to_string)
}

pub(super) fn user_id_of(subscription: &Subscription) -> Option<String> {
    subscription
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !looks_like_email(value))
        .map(str::to_string)
}

pub(super) fn auth_token_path() -> PathBuf {
    cache_dir().join(AUTH_FILE)
}

pub(super) fn cache_dir() -> PathBuf {
    tool_paths::aws_sso_cache_dir()
}

pub(super) fn profile_path() -> Option<PathBuf> {
    tool_paths::kiro_data_dir().map(|root| {
        root.join("User")
            .join("globalStorage")
            .join("kiro.kiroagent")
            .join("profile.json")
    })
}

pub(super) fn state_db_path() -> Option<PathBuf> {
    tool_paths::kiro_data_dir()
        .map(|root| root.join("User").join("globalStorage").join("state.vscdb"))
}

pub(super) fn hashed_registration_path(start_url: &str) -> PathBuf {
    cache_dir().join(format!("{}.json", client_id_hash(start_url)))
}

pub(super) fn registration_path(cache: &Path, hash: &str) -> Option<PathBuf> {
    if !is_client_hash(hash) {
        return None;
    }
    Some(cache.join(format!("{hash}.json")))
}

pub(super) fn client_id_hash(start_url: &str) -> String {
    Sha1::digest(start_url.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn is_client_hash(hash: &str) -> bool {
    hash.len() == HASH_LEN
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn expires_rfc3339(seconds: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|stamp| stamp.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

pub(super) fn parse_expires(value: &Value) -> Option<i64> {
    let raw = value.get("expiresAt").or_else(|| value.get("expires_at"))?;
    if let Some(seconds) = raw.as_i64() {
        return normalize_epoch(seconds);
    }
    if let Some(text) = raw.as_str() {
        let text = text.trim();
        if let Ok(seconds) = text.parse::<i64>() {
            return normalize_epoch(seconds);
        }
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(text) {
            return Some(parsed.timestamp());
        }
    }
    None
}

pub(super) fn normalize_epoch(raw: i64) -> Option<i64> {
    if raw <= 0 {
        None
    } else if raw > 10_000_000_000 {
        Some(raw / 1000)
    } else {
        Some(raw)
    }
}

pub(super) fn field(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
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

pub(super) fn field_value(value: Option<&Value>, keys: &[&str]) -> Option<String> {
    field(value?.as_object()?, keys)
}

pub(super) fn insert(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|text| !text.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

pub(super) fn eq_opt(stored: Option<&str>, live: Option<&str>) -> bool {
    match (
        stored.map(str::trim).filter(|value| !value.is_empty()),
        live.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(stored), Some(live)) => stored == live,
        _ => false,
    }
}

pub(super) fn looks_like_email(value: &str) -> bool {
    let value = value.trim();
    value.contains('@') && value.len() > 3 && !value.contains(' ')
}

fn normalize_provider(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "google" => "Google".to_string(),
        "github" => "Github".to_string(),
        "builderid" | "builder_id" | "builder-id" => "BuilderId".to_string(),
        "enterprise" => "Enterprise".to_string(),
        "internal" => "Internal".to_string(),
        _ => value.trim().to_string(),
    }
}

pub(super) fn is_social(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "google" | "github"
    )
}

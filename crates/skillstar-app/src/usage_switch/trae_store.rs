//! Trae `storage.json` auth documents, ciphertext, and live-session matching.
//!
//! The adapter in `trae.rs` owns backup, atomic replace, and the pin. This
//! module owns which iCube keys those bytes belong to.

use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value};
use skillstar_usage::crypto;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::tool_store::byte_crypto;
use skillstar_usage::trae_platform::TraePlatformKind;
use skillstar_usage::{UsageError, UsageResult, storage};

pub(super) fn user_auth_key(root: &Map<String, Value>) -> String {
    if root.contains_key(super::DEFAULT_AUTH_KEY) {
        return super::DEFAULT_AUTH_KEY.to_string();
    }
    root.keys()
        .filter(|key| is_user_auth_key(key))
        .min()
        .cloned()
        .unwrap_or_else(|| super::DEFAULT_AUTH_KEY.to_string())
}

fn is_user_auth_key(key: &str) -> bool {
    key.starts_with(super::AUTH_PREFIX)
        && key != super::USERTAG_KEY
        && !key.starts_with(super::DEVICE_PREFIX)
}

pub(super) fn device_key_name(
    root: &Map<String, Value>,
    explicit_id: Option<&str>,
) -> Option<String> {
    if let Some(id) = explicit_id.map(str::trim).filter(|id| !id.is_empty()) {
        return Some(format!("{}{id}", super::DEVICE_PREFIX));
    }
    root.keys()
        .filter(|key| key.starts_with(super::DEVICE_PREFIX))
        .min()
        .cloned()
}

pub(super) fn removal_keys(root: &Map<String, Value>, subscription: &Subscription) -> Vec<String> {
    let canonical = user_auth_key(root);
    let mut keys = Vec::new();
    if root
        .get(&canonical)
        .and_then(opened_value)
        .is_some_and(|value| same_account(subscription, &live_from_value(&value)))
    {
        keys.push(canonical);
    }
    if keys.is_empty() {
        return keys;
    }
    for key in root.keys() {
        if !is_user_auth_key(key) || keys.iter().any(|have| have == key) {
            continue;
        }
        if root
            .get(key)
            .and_then(opened_value)
            .is_some_and(|value| same_account(subscription, &live_from_value(&value)))
        {
            keys.push(key.clone());
        }
    }
    if let Some(pair) = provider_of(subscription).device {
        for key in root.keys() {
            if !key.starts_with(super::DEVICE_PREFIX) {
                continue;
            }
            if root
                .get(key)
                .and_then(opened_value)
                .is_some_and(|value| key_pair(&value).is_some_and(|found| same_pair(&found, &pair)))
            {
                keys.push(key.clone());
            }
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

#[derive(Debug)]
enum Opened {
    Absent,
    Value(Value),
    Locked,
}

fn open_value(value: &Value) -> Opened {
    if value.is_object() || value.is_array() {
        return Opened::Value(value.clone());
    }
    let Some(text) = value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    else {
        return Opened::Absent;
    };
    if let Ok(parsed) = serde_json::from_str::<Value>(text) {
        if parsed.is_string() {
            return open_value(&parsed);
        }
        return Opened::Value(parsed);
    }
    match decode_cipher(text) {
        Ok(parsed) => Opened::Value(parsed),
        Err(()) => Opened::Locked,
    }
}

pub(super) fn opened_value(value: &Value) -> Option<Value> {
    match open_value(value) {
        Opened::Value(value) => Some(value),
        Opened::Absent | Opened::Locked => None,
    }
}

fn decode_cipher(text: &str) -> Result<Value, ()> {
    let bytes = STANDARD.decode(text.trim()).map_err(|_| ())?;
    let plain = byte_crypto::decode(&bytes).map_err(|_| ())?;
    let text = String::from_utf8(plain).map_err(|_| ())?;
    serde_json::from_str(&text).map_err(|_| ())
}

pub(super) fn encode_value(kind: TraePlatformKind, value: &Value) -> UsageResult<Value> {
    let plain = serde_json::to_string(value).map_err(|err| {
        UsageError::Other(format!("序列化 {} 登录态失败：{err}", kind.display_name()))
    })?;
    let blob = byte_crypto::encode(plain.as_bytes()).map_err(|err| {
        UsageError::Other(format!(
            "{} storage.json 加密失败：{err}",
            kind.display_name()
        ))
    })?;
    Ok(Value::String(STANDARD.encode(blob)))
}

pub(super) struct LiveAuth {
    access: Option<String>,
    refresh: Option<String>,
    user_id: Option<String>,
    expires_at: Option<i64>,
    device: Option<(String, String)>,
    pub(super) locked: bool,
}

impl LiveAuth {
    fn locked() -> Self {
        Self {
            access: None,
            refresh: None,
            user_id: None,
            expires_at: None,
            device: None,
            locked: true,
        }
    }

    fn is_empty(&self) -> bool {
        self.access.is_none() && self.refresh.is_none() && self.user_id.is_none()
    }
}

pub(super) fn read_live(kind: TraePlatformKind, path: &Path) -> UsageResult<Option<LiveAuth>> {
    if !path.is_file() {
        return Ok(None);
    }
    let root = super::read_root(kind, path)?;
    Ok(live_from_root(&root))
}

pub(super) fn live_from_root(root: &Map<String, Value>) -> Option<LiveAuth> {
    let key = user_auth_key(root);
    let raw = root.get(&key)?;
    match open_value(raw) {
        Opened::Absent => None,
        Opened::Locked => Some(LiveAuth::locked()),
        Opened::Value(value) => {
            let mut live = live_from_value(&value);
            if live.device.is_none() {
                live.device = device_from_root(root);
            }
            if live.is_empty() { None } else { Some(live) }
        }
    }
}

fn device_from_root(root: &Map<String, Value>) -> Option<(String, String)> {
    let key = device_key_name(root, None)?;
    let raw = root.get(&key)?;
    match open_value(raw) {
        Opened::Value(value) => key_pair(&value),
        Opened::Absent | Opened::Locked => None,
    }
}

fn live_from_value(value: &Value) -> LiveAuth {
    if !value.is_object() {
        return match value.as_str() {
            Some(text) => LiveAuth {
                access: nonempty(Some(text.to_string())),
                refresh: None,
                user_id: None,
                expires_at: None,
                device: None,
                locked: false,
            },
            None => LiveAuth {
                access: None,
                refresh: None,
                user_id: None,
                expires_at: None,
                device: None,
                locked: false,
            },
        };
    }
    LiveAuth {
        access: pick_string(value, &[&["accessToken"], &["token"], &["access_token"]]),
        refresh: pick_string(
            value,
            &[&["refreshToken"], &["refresh_token"], &["RefreshToken"]],
        ),
        user_id: pick_string(
            value,
            &[
                &["userId"],
                &["user_id"],
                &["uid"],
                &["account", "uid"],
                &["account", "userId"],
            ],
        ),
        expires_at: live_expiry(value),
        device: key_pair(value),
        locked: false,
    }
}

pub(super) struct Provider {
    pub(super) client_id: Option<String>,
    pub(super) login_host: Option<String>,
    pub(super) auth_domain: Option<String>,
    pub(super) device: Option<(String, String)>,
    pub(super) device_id: Option<String>,
}

pub(super) fn provider_of(subscription: &Subscription) -> Provider {
    let mut provider = Provider {
        client_id: None,
        login_host: None,
        auth_domain: None,
        device: None,
        device_id: None,
    };
    let Some(raw) = secret_text(&subscription.provider_state_encrypted) else {
        return provider;
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return provider;
    };
    provider.client_id = pick_string(&value, &[&["clientId"], &["ClientID"], &["authClientId"]]);
    provider.login_host = pick_string(&value, &[&["loginHost"], &["host"]])
        .as_deref()
        .and_then(normalize_origin);
    provider.auth_domain = pick_string(&value, &[&["authDomain"]]);
    provider.device = key_pair(&value);
    provider.device_id = pick_string(
        &value,
        &[
            &["deviceId"],
            &["DeviceID"],
            &["device_id"],
            &["deviceInfo", "DeviceID"],
            &["deviceInfo", "deviceId"],
        ],
    );
    provider
}

pub(super) fn choose_host(
    kind: TraePlatformKind,
    provider: &Provider,
    existing: Option<&Value>,
) -> String {
    if let Some(host) = provider.login_host.clone() {
        return host;
    }
    if let Some(existing) = existing
        && let Some(host) = pick_string(existing, &[&["loginHost"], &["host"]])
            .as_deref()
            .and_then(normalize_origin)
    {
        return host;
    }
    kind.default_login_host().to_string()
}

pub(super) fn choose_text(
    explicit: Option<&str>,
    existing: Option<&Value>,
    paths: &[&[&str]],
    fallback: &str,
) -> String {
    if let Some(value) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return value.to_string();
    }
    if let Some(existing) = existing
        && let Some(value) = pick_string(existing, paths)
    {
        return value;
    }
    fallback.to_string()
}

pub(super) fn matching_subscription(
    kind: TraePlatformKind,
    live: &LiveAuth,
) -> UsageResult<Option<Subscription>> {
    if live.locked || live.is_empty() {
        return Ok(None);
    }
    let mine: Vec<Subscription> = storage::list_subscriptions()?
        .into_iter()
        .filter(|subscription| subscription.catalog_id == kind.catalog_id())
        .collect();
    if let Some(found) = mine
        .iter()
        .find(|subscription| token_matches(subscription, live))
    {
        return Ok(Some(found.clone()));
    }
    if let Some(found) = mine
        .iter()
        .find(|subscription| uid_matches(subscription, live))
    {
        return Ok(Some(found.clone()));
    }
    if let Some(found) = mine
        .iter()
        .find(|subscription| refresh_matches(subscription, live))
    {
        return Ok(Some(found.clone()));
    }
    Ok(None)
}

pub(super) fn same_account(subscription: &Subscription, live: &LiveAuth) -> bool {
    !live.locked
        && (token_matches(subscription, live)
            || uid_matches(subscription, live)
            || refresh_matches(subscription, live))
}

fn token_matches(subscription: &Subscription, live: &LiveAuth) -> bool {
    match (
        secret_text(&subscription.access_token_encrypted).as_deref(),
        live.access.as_deref(),
    ) {
        (Some(stored), Some(live_token)) => stored == live_token,
        _ => false,
    }
}

fn refresh_matches(subscription: &Subscription, live: &LiveAuth) -> bool {
    match (
        secret_text(&subscription.refresh_token_encrypted).as_deref(),
        live.refresh.as_deref(),
    ) {
        (Some(stored), Some(live_token)) => stored == live_token,
        _ => false,
    }
}

fn uid_matches(subscription: &Subscription, live: &LiveAuth) -> bool {
    match (
        nonempty(subscription.oauth_account_id.clone()).as_deref(),
        live.user_id.as_deref(),
    ) {
        (Some(stored), Some(live_uid)) => stored == live_uid,
        _ => false,
    }
}

pub(super) fn absorb(
    kind: TraePlatformKind,
    subscription: &Subscription,
    live: &LiveAuth,
) -> Subscription {
    if live.locked {
        return subscription.clone();
    }
    let mut updated = subscription.clone();
    assign_secret(&mut updated.access_token_encrypted, live.access.as_deref());
    assign_secret(
        &mut updated.refresh_token_encrypted,
        live.refresh.as_deref(),
    );
    if let Some(expires) = live.expires_at
        && updated.access_token_expires_at != Some(expires)
    {
        updated.access_token_expires_at = Some(expires);
    }
    if let Some(uid) = live.user_id.clone()
        && updated.oauth_account_id.as_deref() != Some(uid.as_str())
    {
        updated.oauth_account_id = Some(uid);
    }
    if let Some(pair) = live.device.clone() {
        assign_device(kind, &mut updated, &pair);
    }
    updated
}

fn assign_secret(slot: &mut Option<String>, plain: Option<&str>) {
    let Some(plain) = plain.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let current = slot.as_deref().map(crypto::decrypt).unwrap_or_default();
    if current != plain {
        *slot = Some(crypto::encrypt(plain));
    }
}

fn assign_device(kind: TraePlatformKind, subscription: &mut Subscription, pair: &(String, String)) {
    let current = secret_text(&subscription.provider_state_encrypted);
    let mut value = current
        .as_deref()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    obj.entry("clientId".to_string())
        .or_insert_with(|| Value::String(kind.auth_client_id().to_string()));
    obj.entry("loginHost".to_string())
        .or_insert_with(|| Value::String(kind.default_login_host().to_string()));
    obj.entry("authDomain".to_string())
        .or_insert_with(|| Value::String(kind.auth_domain().to_string()));
    let next = serde_json::json!({
        "privateKeyPEM": pair.0.trim(),
        "publicKeyPEM": pair.1.trim(),
    });
    if obj.get("deviceKeyPair") == Some(&next) {
        return;
    }
    obj.insert("deviceKeyPair".to_string(), next);
    subscription.provider_state_encrypted = Some(crypto::encrypt(&value.to_string()));
}

pub(super) fn credentials_changed(updated: &Subscription, original: &Subscription) -> bool {
    updated.access_token_encrypted != original.access_token_encrypted
        || updated.refresh_token_encrypted != original.refresh_token_encrypted
        || updated.access_token_expires_at != original.access_token_expires_at
        || updated.oauth_account_id != original.oauth_account_id
        || updated.provider_state_encrypted != original.provider_state_encrypted
}

pub(super) fn identity_names(
    kind: TraePlatformKind,
    subscription: &Subscription,
) -> (Option<String>, Option<String>) {
    let name = subscription.display_name.trim();
    if name.is_empty()
        || name.eq_ignore_ascii_case(kind.display_name())
        || name == kind.catalog_id()
    {
        return (None, None);
    }
    if looks_like_email(name) {
        let email = name.to_string();
        return (Some(email.clone()), Some(email));
    }
    (None, Some(name.to_string()))
}

fn key_pair(value: &Value) -> Option<(String, String)> {
    let nested = value.get("deviceKeyPair").unwrap_or(value);
    let private_pem = pick_string(nested, &[&["privateKeyPEM"], &["private_key_pem"]])?;
    let public_pem = pick_string(nested, &[&["publicKeyPEM"], &["public_key_pem"]])?;
    Some((private_pem, public_pem))
}

fn same_pair(left: &(String, String), right: &(String, String)) -> bool {
    left.0.trim() == right.0.trim() && left.1.trim() == right.1.trim()
}

pub(super) fn put_access(obj: &mut Map<String, Value>, access: &str) {
    obj.remove("access_token");
    let access = access.trim();
    put_text(
        obj,
        "accessToken",
        Some(access).filter(|value| !value.is_empty()),
    );
    put_text(obj, "token", Some(access).filter(|value| !value.is_empty()));
}

pub(super) fn put_refresh(obj: &mut Map<String, Value>, refresh: Option<&str>) {
    obj.remove("refresh_token");
    obj.remove("RefreshToken");
    put_text(obj, "refreshToken", refresh);
}

pub(super) fn put_user(obj: &mut Map<String, Value>, uid: Option<&str>) {
    obj.remove("user_id");
    obj.remove("uid");
    put_text(obj, "userId", uid);
}

pub(super) fn put_text(obj: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    match value.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => {
            obj.insert(key.to_string(), Value::String(text.to_string()));
        }
        None => {
            obj.remove(key);
        }
    }
}

fn live_expiry(value: &Value) -> Option<i64> {
    if let Some(number) = value.get("expiresAt").and_then(json_i64)
        && let Some(seconds) = epoch_seconds(number)
    {
        return Some(seconds);
    }
    for key in ["expiredAt", "expiresAt"] {
        if let Some(text) = value.get(key).and_then(Value::as_str)
            && let Some(seconds) = parse_time(text)
        {
            return Some(seconds);
        }
    }
    None
}

fn parse_time(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    if let Ok(number) = trimmed.parse::<i64>() {
        return epoch_seconds(number);
    }
    chrono::DateTime::parse_from_rfc3339(trimmed)
        .ok()
        .map(|time| time.timestamp())
}

pub(super) fn expiry_iso(seconds: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

pub(super) fn epoch_seconds(raw: i64) -> Option<i64> {
    if raw <= 0 {
        None
    } else if raw >= 100_000_000_000 {
        Some(raw / 1000)
    } else if raw >= 1_000_000_000 {
        Some(raw)
    } else {
        None
    }
}

fn normalize_origin(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let (scheme, rest) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        ("http", rest)
    } else {
        ("https", trimmed)
    };
    let host = rest.split('/').next().filter(|host| !host.is_empty())?;
    Some(format!("{scheme}://{host}"))
}

fn pick_string(value: &Value, paths: &[&[&str]]) -> Option<String> {
    for path in paths {
        if let Some(found) = dig(value, path)
            && let Some(text) = scalar_string(found)
        {
            return Some(text);
        }
    }
    None
}

fn dig<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.as_object()?.get(*key)?;
    }
    Some(current)
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
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok())),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

pub(super) fn nonempty(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub(super) fn secret_text(slot: &Option<String>) -> Option<String> {
    nonempty(slot.as_deref().map(crypto::decrypt))
}

fn looks_like_email(value: &str) -> bool {
    let value = value.trim();
    value.len() > 3 && value.contains('@') && !value.contains(char::is_whitespace)
}

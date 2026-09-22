//! Trae, TRAE SOLO, Trae CN, and TRAE SOLO CN.
//!
//! One implementation. Product differences live on [`TraePlatformKind`].
//! There is no browser login: a refresh token is exchanged once at
//! `/trae/api/v3/oauth/ExchangeToken` with a device proof. Quota is
//! `GetUserInfo` plus entitlement. `provider_state` is plaintext JSON
//! `{deviceKeyPair, clientId, loginHost, authDomain}` until the import
//! pipeline encrypts it. `platform_token_encrypted` is not used.

mod exchange;
mod http;
mod import;
mod login;
mod quota;

use serde_json::{Value, json};

use crate::subscription::{Subscription, SubscriptionUsage};
use crate::trae_platform::TraePlatformKind;
use crate::{UsageError, UsageResult};

pub(crate) use import::{
    import_from_local, import_from_local_cn, import_from_local_solo, import_from_local_solo_cn,
    import_from_token, import_from_token_cn, import_from_token_solo, import_from_token_solo_cn,
    oauth_row_from_imported, oauth_row_from_imported_cn, oauth_row_from_imported_solo,
    oauth_row_from_imported_solo_cn,
};
pub(crate) use login::start_login;

const TITLE_PLACEHOLDERS: &[&str] = &["Trae", "TRAE SOLO", "Trae CN", "TRAE SOLO CN"];

pub(super) struct TraeAuthState {
    pub private_pem: String,
    pub public_pem: String,
    pub client_id: String,
    pub login_host: String,
    pub auth_domain: String,
}

impl TraeAuthState {
    fn from_kind(kind: TraePlatformKind) -> Self {
        Self {
            private_pem: String::new(),
            public_pem: String::new(),
            client_id: kind.auth_client_id().to_string(),
            login_host: kind.default_login_host().to_string(),
            auth_domain: kind.auth_domain().to_string(),
        }
    }

    pub(super) fn has_key(&self) -> bool {
        !self.private_pem.trim().is_empty() && !self.public_pem.trim().is_empty()
    }

    pub(super) fn to_json(&self) -> String {
        json!({
            "deviceKeyPair": {
                "privateKeyPEM": self.private_pem,
                "publicKeyPEM": self.public_pem,
            },
            "clientId": self.client_id,
            "loginHost": self.login_host,
            "authDomain": self.auth_domain,
        })
        .to_string()
    }

    fn parse(kind: TraePlatformKind, raw: &str) -> Self {
        let mut state = Self::from_kind(kind);
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return state;
        };
        if let Some(client_id) =
            pick_string(&value, &[&["clientId"], &["ClientID"], &["authClientId"]])
        {
            state.client_id = client_id;
        }
        if let Some(host) = pick_string(&value, &[&["loginHost"], &["host"]])
            .as_deref()
            .and_then(normalize_origin)
        {
            state.login_host = host;
        }
        if let Some(domain) = pick_string(&value, &[&["authDomain"]]) {
            state.auth_domain = domain;
        }
        if let Some((private_pem, public_pem)) = key_pair(&value) {
            state.private_pem = private_pem;
            state.public_pem = public_pem;
        }
        state
    }
}

fn key_pair(value: &Value) -> Option<(String, String)> {
    let nested = value.get("deviceKeyPair").unwrap_or(value);
    let private_pem = pick_string(nested, &[&["privateKeyPEM"], &["private_key_pem"]])?;
    let public_pem = pick_string(nested, &[&["publicKeyPEM"], &["public_key_pem"]])?;
    Some((private_pem, public_pem))
}

pub(crate) async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let kind = kind_of(&subscription.catalog_id)?;
    let client = crate::fetchers::http_client()?;
    fetch_with(&client, kind, subscription, None).await
}

pub(super) async fn fetch_with(
    client: &reqwest::Client,
    kind: TraePlatformKind,
    subscription: &mut Subscription,
    scripted_origin: Option<&str>,
) -> UsageResult<SubscriptionUsage> {
    let mut state = state_from(kind, subscription);
    let mut access = decrypt_optional(&subscription.access_token_encrypted).unwrap_or_default();
    let refresh = decrypt_optional(&subscription.refresh_token_encrypted);
    let mut exchanged = false;
    if let Some(refresh_token) = refresh
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let origin = scripted_origin
            .map(str::to_string)
            .unwrap_or_else(|| exchange_origin(kind, &state.login_host));
        let issued = exchange::exchange_refresh(
            client,
            &origin,
            &state,
            refresh_token,
            nonempty(Some(access.as_str())).as_deref(),
        )
        .await?;
        exchanged = true;
        access = issued.access_token;
        if let Some(host) = issued.login_host.as_deref().and_then(normalize_origin) {
            state.login_host = host;
        }
        apply_tokens(
            subscription,
            &access,
            issued.refresh_token.as_deref().or(Some(refresh_token)),
            issued.expires_at.or(subscription.access_token_expires_at),
            &state,
        );
        if let Some(user_id) = issued.user_id
            && subscription
                .oauth_account_id
                .as_deref()
                .map(str::trim)
                .is_none_or(str::is_empty)
            {
                subscription.oauth_account_id = Some(user_id);
            }
        if let Some(region) = issued
            .login_region
            .as_deref()
            .and_then(quota::normalize_login_region)
        {
            subscription.oauth_region = Some(region);
        }
    }
    if access.trim().is_empty() {
        return Err(UsageError::AuthRequired);
    }
    let origins = match scripted_origin {
        Some(origin) => vec![origin.to_string()],
        None => quota_origins(kind, &state.login_host),
    };
    match quota::read_quota(client, kind, &origins, access.trim()).await {
        Ok(snapshot) => Ok(usage_from_snapshot(subscription, snapshot)),
        Err(err) if exchanged => Ok(usage_error(subscription, &err)),
        Err(err) => Err(err),
    }
}

fn usage_from_snapshot(
    subscription: &mut Subscription,
    snapshot: quota::QuotaSnapshot,
) -> SubscriptionUsage {
    if let Some(user_id) = snapshot.user_id.clone()
        && subscription
            .oauth_account_id
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
        {
            subscription.oauth_account_id = Some(user_id);
        }
    if let Some(region) = snapshot.login_region.clone() {
        subscription.oauth_region = Some(region);
    }
    crate::fetchers::oauth::common::apply_email_title(
        subscription,
        snapshot.email.as_deref(),
        TITLE_PLACEHOLDERS,
    );
    SubscriptionUsage {
        subscription_id: subscription.id.clone(),
        fetched_at: chrono::Utc::now().timestamp(),
        plan_name: snapshot.plan_name,
        monthly: snapshot.monthly,
        credits: snapshot.credits,
        ..Default::default()
    }
}

fn usage_error(subscription: &Subscription, err: &UsageError) -> SubscriptionUsage {
    SubscriptionUsage {
        subscription_id: subscription.id.clone(),
        fetched_at: chrono::Utc::now().timestamp(),
        error: Some(err.to_string()),
        ..Default::default()
    }
}

fn apply_tokens(
    subscription: &mut Subscription,
    access: &str,
    refresh: Option<&str>,
    expires_at: Option<i64>,
    state: &TraeAuthState,
) {
    if !access.trim().is_empty() {
        subscription.access_token_encrypted = Some(crate::crypto::encrypt(access));
    }
    if let Some(refresh) = refresh.map(str::trim).filter(|token| !token.is_empty()) {
        subscription.refresh_token_encrypted = Some(crate::crypto::encrypt(refresh));
    }
    if expires_at.is_some() {
        subscription.access_token_expires_at = expires_at;
    }
    if state.has_key() {
        subscription.provider_state_encrypted = Some(crate::crypto::encrypt(&state.to_json()));
    }
}

fn state_from(kind: TraePlatformKind, subscription: &Subscription) -> TraeAuthState {
    match decrypt_optional(&subscription.provider_state_encrypted) {
        Some(raw) => TraeAuthState::parse(kind, &raw),
        None => TraeAuthState::from_kind(kind),
    }
}

pub(super) fn exchange_origin(kind: TraePlatformKind, login_host: &str) -> String {
    normalize_origin(login_host).unwrap_or_else(|| kind.default_login_host().to_string())
}

pub(super) fn quota_origins(kind: TraePlatformKind, login_host: &str) -> Vec<String> {
    let mut origins = Vec::new();
    if let Some(origin) = normalize_origin(login_host) {
        origins.push(origin);
    }
    for host in kind.region_hosts() {
        if !origins.iter().any(|item| item == host) {
            origins.push((*host).to_string());
        }
    }
    if origins.is_empty() {
        origins.push(kind.default_login_host().to_string());
    }
    origins
}

fn kind_of(catalog_id: &str) -> UsageResult<TraePlatformKind> {
    TraePlatformKind::from_catalog_id(catalog_id)
        .ok_or_else(|| crate::fetchers::unsupported(catalog_id))
}

pub(super) fn oauth_flow_home(kind: TraePlatformKind) -> &'static str {
    kind.product_home()
}

pub(super) fn normalize_origin(raw: &str) -> Option<String> {
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

pub(super) fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

pub(super) fn decrypt_optional(cipher: &Option<String>) -> Option<String> {
    let plain = crate::crypto::decrypt(cipher.as_deref().unwrap_or(""));
    nonempty(Some(plain.as_str()))
}

pub(super) fn payload_root(value: &Value) -> &Value {
    for key in ["Result", "result", "data", "Data"] {
        if let Some(child) = value.get(key)
            && child.is_object()
        {
            return child;
        }
    }
    value
}

pub(super) fn pick_string(value: &Value, paths: &[&[&str]]) -> Option<String> {
    pick(value, paths, scalar_string)
}

pub(super) fn pick_i64(value: &Value, paths: &[&[&str]]) -> Option<i64> {
    pick(value, paths, json_i64)
}

pub(super) fn pick_f64(value: &Value, paths: &[&[&str]]) -> Option<f64> {
    pick(value, paths, json_f64)
}

fn pick<T>(value: &Value, paths: &[&[&str]], cast: fn(&Value) -> Option<T>) -> Option<T> {
    let roots = [value, payload_root(value)];
    for root in roots {
        for path in paths {
            if let Some(found) = dig(root, path).and_then(&cast) {
                return Some(found);
            }
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
        Value::String(text) => nonempty(Some(text)),
        Value::Number(number) => nonempty(Some(&number.to_string())),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

pub(super) fn json_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| number.as_f64().and_then(f64_to_i64)),
        Value::String(text) => text
            .trim()
            .parse::<i64>()
            .ok()
            .or_else(|| text.trim().parse::<f64>().ok().and_then(f64_to_i64)),
        _ => None,
    }
}

fn f64_to_i64(value: f64) -> Option<i64> {
    if !value.is_finite() {
        return None;
    }
    Some(value as i64)
}

pub(super) fn json_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|value| value.is_finite()),
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite()),
        _ => None,
    }
}

pub(super) fn epoch_seconds(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }
    if raw >= 100_000_000_000 {
        return Some(raw / 1000);
    }
    if raw >= 1_000_000_000 {
        return Some(raw);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_stays_on_one_origin_and_quota_walks_region_hosts() {
        for kind in TraePlatformKind::ALL {
            let origin = exchange_origin(kind, "");
            assert_eq!(origin, kind.default_login_host());
            let stored = exchange_origin(kind, "https://api.trae.ai/cloudide/api");
            assert_eq!(stored, "https://api.trae.ai");
            let origins = quota_origins(kind, kind.default_login_host());
            assert_eq!(origins[0], kind.default_login_host());
            assert_eq!(origins.len(), kind.region_hosts().len());
            if kind.is_cn() {
                assert!(origins.iter().all(|host| !host.contains("trae.ai")));
            } else {
                assert!(origins.iter().all(|host| !host.contains("trae.cn")));
            }
        }
        let state = TraeAuthState::parse(
            TraePlatformKind::Trae,
            r#"{"deviceKeyPair":{"privateKeyPEM":"priv","publicKeyPEM":"pub"},"clientId":"custom","loginHost":"https://growsg-normal.trae.ai/x","authDomain":"www.trae.ai"}"#,
        );
        assert!(state.has_key());
        assert_eq!(state.client_id, "custom");
        assert_eq!(state.login_host, "https://growsg-normal.trae.ai");
        let parsed: Value = serde_json::from_str(&state.to_json()).unwrap();
        assert_eq!(parsed["deviceKeyPair"]["privateKeyPEM"], "priv");
        assert!(parsed.get("platform_token").is_none());
    }
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod refresh_tests;

//! Qoder quota, device-poll login, and local/token import.
//!
//! Machine token and id live in plaintext `provider_state` JSON. Login and
//! import encrypt that blob. It is not a `platform_token`.

mod http;
mod import;
mod login;
mod quota;

use serde_json::{Map, Value};

use crate::UsageResult;
use crate::subscription::{Subscription, SubscriptionUsage};

pub(crate) const CATALOG_ID: &str = "qoder";
pub(crate) const LOGIN_URL: &str = "https://qoder.com/device/selectAccounts";
pub(crate) const OPENAPI_BASE: &str = "https://openapi.qoder.sh";
pub(crate) const REDIRECT_URI: &str = "qoder://aicoding.aicoding-agent/login-success";
pub(crate) const CHALLENGE_METHOD: &str = "S256";

pub(crate) use import::{import_from_local, import_from_token, oauth_row_from_imported};
pub(crate) use login::start_login;

/// Device blob quota and poll headers are built from. Empty fields are omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct QoderMachine {
    pub machine_token: Option<String>,
    pub machine_id: Option<String>,
    pub machine_type: Option<String>,
    pub machine_code: Option<String>,
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub cosy_version: Option<String>,
}

impl QoderMachine {
    pub(crate) fn parse(raw: &str) -> Self {
        serde_json::from_str::<Value>(raw.trim())
            .ok()
            .as_ref()
            .map(Self::from_value)
            .unwrap_or_default()
    }

    pub(crate) fn from_value(value: &Value) -> Self {
        Self {
            machine_token: pick_string(value, &["machineToken", "machine_token"]),
            machine_id: pick_string(value, &["machineId", "machine_id"]),
            machine_type: pick_string(value, &["machineType", "machine_type"]),
            machine_code: pick_string(value, &["machineCode", "machine_code"]),
            hostname: pick_string(value, &["hostname", "machineHostname", "machine_hostname"]),
            os: pick_string(value, &["os", "machineOs", "machine_os"]),
            cosy_version: pick_string(value, &["cosy_version", "cosyVersion"]),
        }
    }

    /// Cockpit `machine_token.json`. Top-level keys only: `token` there is the
    /// machine token, not a user access token.
    pub(crate) fn from_cache_value(value: &Value) -> Self {
        let mut machine = Self::from_value(value);
        machine.fill_missing(&Self {
            machine_token: object_string(value, &["token"]),
            machine_id: object_string(value, &["id"]),
            machine_type: object_string(value, &["type"]),
            machine_code: object_string(value, &["code"]),
            hostname: object_string(value, &["hostname"]),
            os: object_string(value, &["os"]),
            cosy_version: object_string(value, &["version"]),
        });
        machine
    }

    pub(crate) fn from_subscription(subscription: &Subscription) -> Self {
        Self::parse(&decrypt_optional(&subscription.provider_state_encrypted).unwrap_or_default())
    }

    /// Canonical keys from slice 14. `machineCode` is extra because the Cosy
    /// header set cockpit sends includes it.
    pub(crate) fn to_json(&self) -> Option<String> {
        let mut map = Map::new();
        insert(&mut map, "machineToken", self.machine_token.as_deref());
        insert(&mut map, "machineId", self.machine_id.as_deref());
        insert(&mut map, "machineType", self.machine_type.as_deref());
        insert(&mut map, "machineCode", self.machine_code.as_deref());
        insert(&mut map, "hostname", self.hostname.as_deref());
        insert(&mut map, "os", self.os.as_deref());
        insert(&mut map, "cosy_version", self.cosy_version.as_deref());
        if map.is_empty() {
            None
        } else {
            Some(Value::Object(map).to_string())
        }
    }

    pub(crate) fn fill_missing(&mut self, extra: &Self) {
        fill(&mut self.machine_token, extra.machine_token.as_deref());
        fill(&mut self.machine_id, extra.machine_id.as_deref());
        fill(&mut self.machine_type, extra.machine_type.as_deref());
        fill(&mut self.machine_code, extra.machine_code.as_deref());
        fill(&mut self.hostname, extra.hostname.as_deref());
        fill(&mut self.os, extra.os.as_deref());
        fill(&mut self.cosy_version, extra.cosy_version.as_deref());
    }
}

pub(crate) async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    quota::fetch_quota(subscription).await
}

pub(super) fn decrypt_optional(cipher: &Option<String>) -> Option<String> {
    let plain = crate::crypto::decrypt(cipher.as_deref().unwrap_or(""));
    if plain.is_empty() { None } else { Some(plain) }
}

pub(super) fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

pub(super) fn object_string(value: &Value, keys: &[&str]) -> Option<String> {
    let map = value.as_object()?;
    for key in keys {
        if let Some(text) = map
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .and_then(|(_, item)| scalar_string(item))
        {
            return Some(text);
        }
    }
    None
}

/// Earlier keys win, top-level before nested. A generic `id` must not beat
/// `machineId` just because it appears first in the object.
pub(super) fn pick_string(value: &Value, keys: &[&str]) -> Option<String> {
    if let Some(text) = object_string(value, keys) {
        return Some(text);
    }
    for key in keys {
        let mut found = None;
        walk(value, &mut |name, item| {
            if found.is_none()
                && name.eq_ignore_ascii_case(key)
                && let Some(text) = scalar_string(item)
            {
                found = Some(text);
            }
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => nonempty(Some(text)),
        Value::Number(number) => nonempty(Some(&number.to_string())),
        _ => None,
    }
}

fn walk(value: &Value, visit: &mut dyn FnMut(&str, &Value)) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                visit(key, item);
                walk(item, visit);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, visit);
            }
        }
        _ => {}
    }
}

fn insert(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = nonempty(value) {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn fill(slot: &mut Option<String>, value: Option<&str>) {
    if slot.as_deref().is_none_or(str::is_empty)
        && let Some(value) = nonempty(value)
    {
        *slot = Some(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qoder_provider_state_round_trips_and_reads_the_cockpit_cache() {
        let machine = QoderMachine {
            machine_token: Some("mt".into()),
            machine_id: Some("mid".into()),
            machine_type: Some("desktop".into()),
            machine_code: Some("code".into()),
            hostname: Some("host".into()),
            os: Some("aarch64_darwin".into()),
            cosy_version: Some("1.2.3".into()),
        };
        let parsed = QoderMachine::parse(&machine.to_json().expect("json"));
        assert_eq!(parsed, machine);
        assert!(QoderMachine::default().to_json().is_none());

        let cache = QoderMachine::from_cache_value(&serde_json::json!({
            "token": "cache-token",
            "type": "ide",
            "code": "mc",
            "id": "cache-id",
            "hostname": "box",
            "os": "x86_64_linux",
            "version": "9.9.9"
        }));
        assert_eq!(cache.machine_token.as_deref(), Some("cache-token"));
        assert_eq!(cache.machine_id.as_deref(), Some("cache-id"));
        assert_eq!(cache.machine_type.as_deref(), Some("ide"));
        assert_eq!(cache.machine_code.as_deref(), Some("mc"));
        assert_eq!(cache.hostname.as_deref(), Some("box"));
        assert_eq!(cache.os.as_deref(), Some("x86_64_linux"));
        assert_eq!(cache.cosy_version.as_deref(), Some("9.9.9"));
    }
}

//! CodeBuddy and CodeBuddy CN quota, remote-poll login, and import.
//!
//! One implementation. The only differences live on [`Host`]: origin,
//! `oauth_region`, the safe-storage item key, and the `state.vscdb` path.
//! CN is not a second copy of the module.

mod http;
mod import;
mod login;
mod quota;

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Map, Value};

use crate::UsageResult;
use crate::subscription::{Subscription, SubscriptionUsage};

pub(crate) const PLATFORM: &str = "ide";
pub(crate) const AUTH_STATE_PATH: &str = "/v2/plugin/auth/state";
pub(crate) const AUTH_TOKEN_PATH: &str = "/v2/plugin/auth/token";
pub(crate) const AUTH_REFRESH_PATH: &str = "/v2/plugin/auth/token/refresh";
pub(crate) const ACCOUNT_PATH: &str = "/v2/plugin/login/account";
pub(crate) const DOSAGE_PATH: &str = "/v2/billing/meter/get-dosage-notify";
pub(crate) const PAYMENT_PATH: &str = "/v2/billing/meter/get-payment-type";
pub(crate) const USER_RESOURCE_PATH: &str = "/v2/billing/meter/get-user-resource";
pub(crate) const ENTERPRISE_USAGE_PATH: &str = "/v2/billing/meter/get-enterprise-user-usage";
/// Browser UA. CodeBuddy's gateway answers a missing UA with HTTP 403 / `code=10085`.
pub(crate) const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
pub(crate) const REFRESH_SOURCE: &str = "ide-main";
pub(crate) const UA_REJECT_CODE: i64 = 10_085;
pub(crate) const SECRET_EXTENSION: &str = "tencent-cloud.coding-copilot";
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(1_500);
pub(crate) const POLL_INTERVAL_SECS: u32 = 2;
pub(crate) const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

const ROUTES: &[&str] = &[
    AUTH_STATE_PATH,
    AUTH_TOKEN_PATH,
    AUTH_REFRESH_PATH,
    ACCOUNT_PATH,
    DOSAGE_PATH,
    PAYMENT_PATH,
    USER_RESOURCE_PATH,
    ENTERPRISE_USAGE_PATH,
];

/// Fixed per catalog id. Callers do not pick a region at login time.
pub(crate) struct Host {
    pub catalog_id: &'static str,
    pub display_name: &'static str,
    pub origin: &'static str,
    pub oauth_region: &'static str,
    pub secret_key: &'static str,
    pub state_db: fn() -> Option<PathBuf>,
}

pub(crate) const GLOBAL: Host = Host {
    catalog_id: "codebuddy",
    display_name: "CodeBuddy",
    origin: "https://www.codebuddy.ai",
    oauth_region: "global",
    secret_key: "planning-genie.new.accessToken",
    state_db: crate::tool_paths::codebuddy_state_db_path,
};

pub(crate) const CN: Host = Host {
    catalog_id: "codebuddy-cn",
    display_name: "CodeBuddy CN",
    origin: "https://www.codebuddy.cn",
    oauth_region: "cn",
    secret_key: "planning-genie.new.accessTokencn",
    state_db: crate::tool_paths::codebuddy_cn_state_db_path,
};

pub(crate) const HOSTS: &[&Host] = &[&GLOBAL, &CN];

pub(crate) fn host_for(catalog_id: &str) -> Option<&'static Host> {
    HOSTS
        .iter()
        .copied()
        .find(|host| host.catalog_id == catalog_id)
}

impl Host {
    pub(crate) fn secret_item_key(&self) -> String {
        format!(
            r#"secret://{{"extensionId":"{SECRET_EXTENSION}","key":"{}"}}"#,
            self.secret_key
        )
    }
}

pub(crate) fn endpoint(origin: &str, path: &str) -> String {
    format!("{}{path}", origin.trim_end_matches('/'))
}

/// Enterprise headers and `X-Domain`. `oauth_region` is not this blob.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Enterprise {
    pub enterprise_id: Option<String>,
    pub enterprise_name: Option<String>,
    pub domain: Option<String>,
}

impl Enterprise {
    pub(crate) fn to_json(&self) -> Option<String> {
        let mut map = Map::new();
        insert(&mut map, "enterpriseId", self.enterprise_id.as_deref());
        insert(&mut map, "enterpriseName", self.enterprise_name.as_deref());
        insert(&mut map, "domain", self.domain.as_deref());
        if map.is_empty() {
            None
        } else {
            Some(Value::Object(map).to_string())
        }
    }

    pub(crate) fn parse(raw: &str) -> Self {
        serde_json::from_str::<Value>(raw)
            .ok()
            .as_ref()
            .map(Self::from_value)
            .unwrap_or_default()
    }

    pub(crate) fn from_value(value: &Value) -> Self {
        Self {
            enterprise_id: pick_string(value, &["enterpriseId", "enterprise_id"]),
            enterprise_name: pick_string(value, &["enterpriseName", "enterprise_name"]),
            domain: pick_string(value, &["domain"]),
        }
    }

    pub(crate) fn from_subscription(subscription: &Subscription) -> Self {
        decrypt_optional(&subscription.provider_state_encrypted)
            .map(|plain| Self::parse(&plain))
            .unwrap_or_default()
    }
}

/// Identity collected while logging in, before it is stored on a row.
pub(crate) struct LoginIdentity {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub uid: Option<String>,
    pub email: Option<String>,
    pub nickname: Option<String>,
    pub enterprise: Enterprise,
}

pub(crate) async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let host = host_for(&subscription.catalog_id)
        .ok_or_else(|| crate::fetchers::unsupported(&subscription.catalog_id))?;
    quota::fetch_quota(host, subscription, true).await
}

pub(crate) use import::{
    import_from_local, import_from_local_cn, import_from_token, import_from_token_cn,
    oauth_row_from_imported, oauth_row_from_imported_cn,
};
pub(crate) use login::start_login;

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
        if let Some((_, item)) = map.iter().find(|(name, _)| name.eq_ignore_ascii_case(key))
            && let Some(text) = scalar_string(item)
        {
            return Some(text);
        }
    }
    None
}

/// Earlier keys win. Top-level is checked before `data` / `auth` / `account`.
pub(super) fn pick_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = object_string(value, &[key]) {
            return Some(text);
        }
        for nest in ["data", "auth", "account", "user"] {
            if let Some(child) = value.get(nest)
                && let Some(text) = object_string(child, &[key])
            {
                return Some(text);
            }
        }
    }
    None
}

pub(super) fn json_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

pub(super) fn json_f64(value: Option<&Value>) -> Option<f64> {
    let number = match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }?;
    number.is_finite().then_some(number)
}

fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => nonempty(Some(text)),
        Value::Number(number) => nonempty(Some(&number.to_string())),
        _ => None,
    }
}

fn insert(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = nonempty(value) {
        map.insert(key.to_string(), Value::String(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_share_every_route_and_differ_only_in_the_table() {
        assert!(USER_AGENT.starts_with("Mozilla/5.0"));
        assert_eq!(PLATFORM, "ide");
        assert_eq!(REFRESH_SOURCE, "ide-main");
        for path in ROUTES {
            let global = endpoint(GLOBAL.origin, path);
            let cn = endpoint(CN.origin, path);
            assert_eq!(
                global.trim_start_matches(GLOBAL.origin),
                cn.trim_start_matches(CN.origin),
                "{path}"
            );
            assert!(global.contains("https://www.codebuddy.ai"), "{global}");
            assert!(cn.contains("https://www.codebuddy.cn"), "{cn}");
            assert!(!global.contains("codebuddy.cn"), "{global}");
            assert!(!cn.contains("codebuddy.ai"), "{cn}");
        }
        assert_eq!(GLOBAL.catalog_id, "codebuddy");
        assert_eq!(CN.catalog_id, "codebuddy-cn");
        assert_eq!(GLOBAL.oauth_region, "global");
        assert_eq!(CN.oauth_region, "cn");
        assert_eq!(GLOBAL.display_name, "CodeBuddy");
        assert_eq!(CN.display_name, "CodeBuddy CN");
        assert_ne!(GLOBAL.secret_key, CN.secret_key);
        assert_eq!(
            GLOBAL.secret_item_key().replace(GLOBAL.secret_key, ""),
            CN.secret_item_key().replace(CN.secret_key, "")
        );
        assert!(GLOBAL.secret_item_key().contains(SECRET_EXTENSION));
        assert!(
            GLOBAL
                .secret_item_key()
                .contains("planning-genie.new.accessToken\"")
        );
        assert!(
            CN.secret_item_key()
                .contains("planning-genie.new.accessTokencn\"")
        );
        assert!(host_for("codebuddy").is_some());
        assert!(host_for("codebuddy-cn").is_some());
        assert!(host_for("workbuddy").is_none());
    }

    #[test]
    fn enterprise_context_round_trips_and_omits_blanks() {
        let enterprise = Enterprise {
            enterprise_id: Some("ent".into()),
            enterprise_name: Some("Acme".into()),
            domain: Some("acme.example".into()),
        };
        let parsed = Enterprise::parse(&enterprise.to_json().expect("json"));
        assert_eq!(parsed, enterprise);
        assert!(Enterprise::default().to_json().is_none());
        let domain_only = Enterprise {
            domain: Some("d".into()),
            ..Enterprise::default()
        };
        assert_eq!(domain_only.to_json().as_deref(), Some(r#"{"domain":"d"}"#));
    }
}

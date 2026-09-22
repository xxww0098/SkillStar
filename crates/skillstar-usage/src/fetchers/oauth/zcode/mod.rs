//! ZCode quota, SchemePaste login, and credential import.
//!
//! The browser is sent to z.ai or BigModel. The callback is a `zcode://` URL
//! the user pastes; it is parsed in-process and never fetched. Token exchange
//! is `POST https://zcode.z.ai/api/v1/oauth/token`. Quota reads the provider
//! profile (Bearer access token, or raw `Authorization` for BigModel) and the
//! billing balance (Bearer zcode JWT).

mod http;
mod import;
mod login;
mod quota;

use serde_json::Value;

use crate::UsageResult;
use crate::subscription::{Subscription, SubscriptionUsage};

pub(crate) const CATALOG_ID: &str = "zcode";
pub(super) const SCHEME_PREFIX: &str = "zcode://";
pub(super) const PLACEHOLDER_NAME: &str = "ZCode";
pub(super) const APP_VERSION: &str = "3.10.2";

/// Cockpit `is_zcode_callback_url`. Host + path, not a path under an empty host.
pub(super) const CALLBACK_ROUTES: &[(&str, &str)] =
    &[("oauth", "/callback"), ("zai-auth", "/callback")];

pub(super) const ZAI_AUTHORIZE: &str = "https://chat.z.ai/api/oauth/authorize";
pub(super) const BIGMODEL_AUTHORIZE: &str = "https://bigmodel.cn/login";
pub(super) const ZAI_REDIRECT: &str = "zcode://zai-auth/callback";
pub(super) const BIGMODEL_REDIRECT: &str = "zcode://oauth/callback";
pub(super) const ZAI_CLIENT_ID: &str = "client_P8X5CMWmlaRO9gyO-KSqtg";

pub(crate) use import::{import_from_local, import_from_token, oauth_row_from_imported};
pub(crate) use login::start_login;

pub(crate) async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    quota::fetch_quota(subscription).await
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Endpoints {
    pub token: String,
    pub zai_business: String,
    pub zai_userinfo: String,
    pub bigmodel_customer: String,
    pub billing: String,
}

impl Endpoints {
    pub(super) fn production() -> Self {
        Self::from_bases(
            "https://zcode.z.ai",
            "https://api.z.ai",
            "https://chat.z.ai",
            "https://open.bigmodel.cn",
        )
    }

    pub(super) fn from_base(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self::from_bases(base, base, base, base)
    }

    fn from_bases(zcode: &str, zai_api: &str, chat: &str, bigmodel: &str) -> Self {
        Self {
            token: format!("{zcode}/api/v1/oauth/token"),
            zai_business: format!("{zai_api}/api/auth/z/login"),
            zai_userinfo: format!("{chat}/api/oauth/userinfo"),
            bigmodel_customer: format!("{bigmodel}/api/biz/customer/getCustomerInfo"),
            billing: format!("{zcode}/api/v1/zcode-plan/billing/balance"),
        }
    }

    pub(super) fn billing_url(&self) -> String {
        format!("{}?app_version={APP_VERSION}", self.billing)
    }
}

pub(super) fn normalize_provider(region: Option<&str>) -> UsageResult<&'static str> {
    match region.unwrap_or("zai").trim().to_ascii_lowercase().as_str() {
        "" | "zai" => Ok("zai"),
        "bigmodel" => Ok("bigmodel"),
        _ => Err(crate::UsageError::Other(
            "不支持的 ZCode 上游，请选择 zai 或 bigmodel".into(),
        )),
    }
}

pub(super) fn callback_host(provider: &str) -> &'static str {
    if provider == "zai" {
        "zai-auth"
    } else {
        "oauth"
    }
}

pub(super) fn provider_state_json(kind: &str) -> String {
    serde_json::json!({ "kind": kind }).to_string()
}

pub(super) fn provider_kind(subscription: &Subscription) -> &'static str {
    let Some(raw) = decrypt_optional(&subscription.provider_state_encrypted) else {
        return "oauth";
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return "oauth";
    };
    match value.get("kind").and_then(Value::as_str) {
        Some("api_key") => "api_key",
        _ => "oauth",
    }
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

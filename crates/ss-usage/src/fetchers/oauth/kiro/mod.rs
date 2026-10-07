//! Kiro quota, portal PKCE, AWS IDC device login, and local/token import.
//!
//! IDC `clientId` / `clientSecret` live in plaintext `provider_state` JSON.
//! The token-import and login paths encrypt that blob. It is not a
//! `platform_token`.

mod http;
mod idc;
mod import;
mod portal;
mod quota;

use serde_json::{Map, Value};

use crate::subscription::{Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

pub(crate) const CATALOG_ID: &str = "kiro";
pub(crate) const PORTAL_ORIGIN: &str = "https://app.kiro.dev/signin";
pub(crate) const PORTAL_TOKEN_URL: &str =
    "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/token";
pub(crate) const PORTAL_REFRESH_URL: &str =
    "https://prod.us-east-1.auth.desktop.kiro.dev/refreshToken";
pub(crate) const DEFAULT_REGION: &str = "us-east-1";
pub(crate) const BUILDER_ID_START_URL: &str = "https://view.awsapps.com/start";

pub(crate) use import::{import_from_local, import_from_token, oauth_row_from_imported};

/// Which browser leg `start_login` opens. Portal is the default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoginLeg {
    Portal,
    /// `region` is an AWS region when the caller passed one, otherwise default.
    Idc {
        region: Option<String>,
    },
}

/// Plaintext provider blob. Writers omit empty fields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct KiroState {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub region: Option<String>,
    pub start_url: Option<String>,
    pub profile_arn: Option<String>,
}

impl KiroState {
    pub(crate) fn parse(raw: &str) -> Self {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw.trim()) else {
            return Self::default();
        };
        Self {
            client_id: owned_string(&map, &["clientId", "client_id"]),
            client_secret: owned_string(&map, &["clientSecret", "client_secret"]),
            region: owned_string(&map, &["region", "idcRegion", "idc_region"]),
            start_url: owned_string(&map, &["startUrl", "start_url", "issuerUrl", "issuer_url"]),
            profile_arn: owned_string(&map, &["profileArn", "profile_arn"]),
        }
    }

    pub(crate) fn from_subscription(subscription: &Subscription) -> Self {
        let mut state = Self::parse(
            &decrypt_optional(&subscription.provider_state_encrypted).unwrap_or_default(),
        );
        if state
            .region
            .as_deref()
            .is_none_or(|region| !is_aws_region(region))
            && let Some(region) = subscription
                .oauth_region
                .as_deref()
                .filter(|region| is_aws_region(region))
        {
            state.region = Some(region.to_string());
        }
        state
    }

    pub(crate) fn to_json(&self) -> Option<String> {
        let mut map = Map::new();
        insert_string(&mut map, "clientId", self.client_id.as_deref());
        insert_string(&mut map, "clientSecret", self.client_secret.as_deref());
        insert_string(&mut map, "region", self.region.as_deref());
        insert_string(&mut map, "startUrl", self.start_url.as_deref());
        insert_string(&mut map, "profileArn", self.profile_arn.as_deref());
        if map.is_empty() {
            None
        } else {
            Some(Value::Object(map).to_string())
        }
    }

    pub(crate) fn has_idc_client(&self) -> bool {
        self.client_id
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            && self
                .client_secret
                .as_deref()
                .is_some_and(|value| !value.is_empty())
    }

    /// Fill gaps from a token JSON object. Existing secrets stay when the
    /// payload omits them — an IDC refresh response often has neither.
    pub(crate) fn overlay_token(&mut self, token: &Value) {
        if let Some(value) = string_field(Some(token), &["clientId", "client_id"]) {
            self.client_id = Some(value);
        }
        if let Some(value) = string_field(Some(token), &["clientSecret", "client_secret"]) {
            self.client_secret = Some(value);
        }
        if let Some(value) = string_field(Some(token), &["region", "idcRegion", "idc_region"])
            .filter(|region| is_aws_region(region))
        {
            self.region = Some(value);
        }
        if let Some(value) = string_field(
            Some(token),
            &["startUrl", "start_url", "issuerUrl", "issuer_url", "issuer"],
        ) {
            self.start_url = Some(value);
        }
        if let Some(value) = string_field(Some(token), &["profileArn", "profile_arn", "arn"]) {
            self.profile_arn = Some(value);
        }
    }
}

pub(crate) async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    quota::fetch_quota(subscription).await
}

pub(crate) async fn start_login(
    region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::OAuthStartInfo> {
    match login_leg(region) {
        LoginLeg::Portal => portal::start_portal(target_subscription_id).await,
        LoginLeg::Idc { region } => idc::start_idc(region.as_deref(), target_subscription_id).await,
    }
}

/// `None`, `portal`, and `social` stay on the PKCE leg. `idc` / `aws-idc`
/// and a real AWS region open the device flow.
pub(crate) fn login_leg(region: Option<&str>) -> LoginLeg {
    let Some(raw) = region.map(str::trim).filter(|value| !value.is_empty()) else {
        return LoginLeg::Portal;
    };
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "idc" | "aws-idc" | "awsidc" => LoginLeg::Idc { region: None },
        other if is_aws_region(other) => LoginLeg::Idc {
            region: Some(other.to_string()),
        },
        _ => LoginLeg::Portal,
    }
}

/// `us-east-1`, `eu-central-1`, `us-gov-west-1`, `us-isof-south-1`.
/// A hyphen alone is not enough — `builder-id` is a login leg, not a region.
pub(crate) fn is_aws_region(value: &str) -> bool {
    let value = value.trim();
    if value.len() > 32 {
        return false;
    }
    let mut parts = value.split('-');
    let Some(geo) = parts.next() else {
        return false;
    };
    if geo.len() != 2 || !geo.chars().all(|ch| ch.is_ascii_lowercase()) {
        return false;
    }
    let mut last = "";
    let mut named = false;
    for part in parts {
        if part.is_empty()
            || !part
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        {
            return false;
        }
        last = part;
        named = true;
    }
    named && !last.is_empty() && last.chars().all(|ch| ch.is_ascii_digit())
}

/// Callback region, then the profile ARN's region, then `us-east-1`.
pub(crate) fn resolve_region(callback: Option<&str>, profile_arn: Option<&str>) -> String {
    if let Some(region) = callback.map(str::trim).filter(|value| is_aws_region(value)) {
        return region.to_string();
    }
    if let Some(region) = profile_arn.and_then(profile_arn_region)
        && is_aws_region(&region)
    {
        return region;
    }
    DEFAULT_REGION.to_string()
}

pub(crate) fn profile_arn_region(profile_arn: &str) -> Option<String> {
    let mut segments = profile_arn.split(':');
    let prefix = segments.next()?.trim();
    if !prefix.eq_ignore_ascii_case("arn") {
        return None;
    }
    let _partition = segments.next()?;
    let _service = segments.next()?;
    let region = segments.next()?.trim();
    if region.is_empty() {
        None
    } else {
        Some(region.to_string())
    }
}

/// CodeWhisperer runtime host for `region`. Unknown regions use us-east-1,
/// matching cockpit `runtime_endpoint_for_region`.
pub(crate) fn runtime_endpoint(region: &str) -> String {
    match region.trim() {
        "eu-central-1" => "https://q.eu-central-1.amazonaws.com".to_string(),
        "us-gov-east-1" => "https://q-fips.us-gov-east-1.amazonaws.com".to_string(),
        "us-gov-west-1" => "https://q-fips.us-gov-west-1.amazonaws.com".to_string(),
        "us-iso-east-1" => "https://q.us-iso-east-1.c2s.ic.gov".to_string(),
        "us-isob-east-1" => "https://q.us-isob-east-1.sc2s.sgov.gov".to_string(),
        "us-isof-south-1" => "https://q.us-isof-south-1.csp.hci.ic.gov".to_string(),
        "us-isof-east-1" => "https://q.us-isof-east-1.csp.hci.ic.gov".to_string(),
        _ => "https://q.us-east-1.amazonaws.com".to_string(),
    }
}

pub(super) fn string_field(value: Option<&Value>, keys: &[&str]) -> Option<String> {
    let map = value?.as_object()?;
    owned_string(map, keys)
}

pub(super) fn decrypt_optional(cipher: &Option<String>) -> Option<String> {
    let plain = crate::crypto::decrypt(cipher.as_deref().unwrap_or(""));
    if plain.is_empty() { None } else { Some(plain) }
}

fn owned_string(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
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

fn insert_string(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|text| !text.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

pub(super) fn invalid_region() -> UsageError {
    UsageError::Other("AWS Region 格式无效".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiro_login_leg_defaults_to_portal_and_selects_idc() {
        assert_eq!(login_leg(None), LoginLeg::Portal);
        assert_eq!(login_leg(Some("")), LoginLeg::Portal);
        assert_eq!(login_leg(Some("portal")), LoginLeg::Portal);
        assert_eq!(login_leg(Some("social")), LoginLeg::Portal);
        assert_eq!(login_leg(Some("builder-id")), LoginLeg::Portal);
        assert_eq!(login_leg(Some("idc")), LoginLeg::Idc { region: None });
        assert_eq!(login_leg(Some("AWS-IDC")), LoginLeg::Idc { region: None });
        assert_eq!(
            login_leg(Some("eu-central-1")),
            LoginLeg::Idc {
                region: Some("eu-central-1".into())
            }
        );
        assert_eq!(
            login_leg(Some("us-gov-west-1")),
            LoginLeg::Idc {
                region: Some("us-gov-west-1".into())
            }
        );
        assert_eq!(
            login_leg(Some("us-isof-south-1")),
            LoginLeg::Idc {
                region: Some("us-isof-south-1".into())
            }
        );
        assert_eq!(login_leg(Some("not a region")), LoginLeg::Portal);
    }

    #[test]
    fn kiro_region_falls_back_callback_then_arn_then_default() {
        let arn = "arn:aws:codewhisperer:us-west-2:111122223333:profile/abc";
        assert_eq!(
            resolve_region(Some("eu-central-1"), Some(arn)),
            "eu-central-1"
        );
        assert_eq!(resolve_region(Some("NOPE"), Some(arn)), "us-west-2");
        assert_eq!(resolve_region(None, Some(arn)), "us-west-2");
        assert_eq!(resolve_region(None, Some("not-an-arn")), DEFAULT_REGION);
        assert_eq!(resolve_region(None, None), DEFAULT_REGION);
    }

    #[test]
    fn kiro_runtime_endpoint_keeps_gov_and_iso_hosts() {
        assert_eq!(
            runtime_endpoint("eu-central-1"),
            "https://q.eu-central-1.amazonaws.com"
        );
        assert_eq!(
            runtime_endpoint("us-gov-west-1"),
            "https://q-fips.us-gov-west-1.amazonaws.com"
        );
        assert_eq!(
            runtime_endpoint("us-iso-east-1"),
            "https://q.us-iso-east-1.c2s.ic.gov"
        );
        assert_eq!(
            runtime_endpoint("mars-1"),
            "https://q.us-east-1.amazonaws.com"
        );
    }

    #[test]
    fn kiro_provider_state_omits_empty_fields() {
        let state = KiroState {
            client_id: Some("cid".into()),
            client_secret: Some("sec".into()),
            region: Some("us-east-1".into()),
            start_url: Some(BUILDER_ID_START_URL.into()),
            profile_arn: None,
        };
        let parsed = KiroState::parse(&state.to_json().unwrap());
        assert_eq!(parsed, state);
        assert!(KiroState::default().to_json().is_none());
    }
}

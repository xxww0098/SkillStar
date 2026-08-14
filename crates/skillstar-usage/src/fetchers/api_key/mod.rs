//! API-key fetchers (DeepSeek, GLM, MiniMax, Kimi).
//!
//! The request path is identical across all four providers (build the
//! client, attach the key per the provider's auth scheme, GET, map transport
//! errors). That shared boilerplate lives in [`fetch_spec`] + [`map_err`],
//! driven by the [`BalanceSpec`] table in `skillstar-providers`. Only the
//! response *parsing* differs per provider and stays in each module.

pub mod deepseek;
pub mod deepseek_platform;
pub mod glm;
pub mod kimi;
pub mod minimax;

use serde::de::DeserializeOwned;
use skillstar_providers::balance::{AuthScheme, BalanceSpec};

use crate::crypto;
use crate::http_client::usage_http_client;
use crate::request::{Req, RequestError};
use crate::subscription::{Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

/// Dispatch an API-key refresh based on `subscription.catalog_id`.
pub async fn dispatch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let key_cipher = subscription
        .api_key_encrypted
        .as_deref()
        .ok_or_else(|| UsageError::Other("订阅缺少 API Key".into()))?;
    let api_key = crypto::decrypt(key_cipher);
    if api_key.is_empty() {
        return Err(UsageError::Other(
            "API Key 解密失败（已损坏或机器变化）".into(),
        ));
    }

    match subscription.catalog_id.as_str() {
        "deepseek" => deepseek::fetch(subscription, &api_key).await,
        "glm" => glm::fetch(&subscription.id, &api_key).await,
        "minimax" => minimax::fetch(&subscription.id, &api_key).await,
        "kimi" => kimi::fetch(&subscription.id, &api_key).await,
        other => Err(super::unsupported(other)),
    }
}

/// Shared request path for every API-key balance fetcher.
///
/// Builds the client, attaches the key per the spec's [`AuthScheme`], issues
/// the GET, and decodes the JSON body into `T`. Transport errors are mapped
/// uniformly via [`map_err`]. Each caller picks the concrete `T` matching that
/// provider's response shape.
pub(super) async fn fetch_spec<T: DeserializeOwned>(
    spec: &BalanceSpec,
    api_key: &str,
) -> UsageResult<T> {
    let client = usage_http_client()
        .map_err(|e| UsageError::Fetcher(format!("{} client: {e}", spec.display_name)))?;

    let req = Req::get(&client, spec.endpoint).header("Accept", "application/json");
    let req = match spec.auth {
        AuthScheme::Bearer => req.bearer(api_key),
        AuthScheme::RawHeader(name) => req.header(name, api_key),
    };

    req.send_json::<T>().await.map_err(|e| map_err(spec, e))
}

/// Uniform transport-error mapping for API-key fetchers.
///
/// Only a 401 means "this key is not accepted". 403 (Cloudflare / WAF /
/// regional block) and 429 / 5xx are the provider's state, not the key's — an
/// API-key account has no re-authorization flow, so mapping them to
/// [`UsageError::AuthRequired`] used to show the user an action that does not
/// exist while wiping the card's last known balance.
fn map_err(spec: &BalanceSpec, e: RequestError) -> UsageError {
    // A provider-specific 401 hint takes precedence over the generic auth error
    // (MiniMax wants the user to know it expects a Token Plan Key).
    if let (Some(hint), RequestError::HttpStatus { status: 401, .. }) = (spec.auth_error_hint, &e) {
        return UsageError::Fetcher(hint.to_string());
    }
    if e.is_auth_error() {
        return UsageError::AuthRequired;
    }
    match e {
        RequestError::HttpStatus { status, body } => {
            UsageError::http_status(spec.display_name, status, &body)
        }
        RequestError::JsonDecode { source, .. } => {
            UsageError::Fetcher(format!("{} 响应解析失败：{source}", spec.display_name))
        }
        RequestError::Transport(error) => UsageError::transport(spec.display_name, error),
        other => UsageError::Fetcher(format!("{} 请求失败：{other}", spec.display_name)),
    }
}

#[cfg(test)]
mod tests {
    use super::map_err;
    use crate::UsageError;
    use crate::catalog::{CatalogTier, catalog};
    use crate::request::RequestError;
    use skillstar_providers::balance::API_KEY_BALANCE_SPECS;

    fn spec_without_hint() -> &'static skillstar_providers::balance::BalanceSpec {
        API_KEY_BALANCE_SPECS
            .iter()
            .find(|spec| spec.auth_error_hint.is_none())
            .expect("at least one balance spec has no provider-specific 401 hint")
    }

    fn http(status: u16) -> RequestError {
        RequestError::HttpStatus {
            status,
            body: "blocked".into(),
        }
    }

    /// An API key has no re-authorization flow, so only the provider actually
    /// rejecting the key (401) may latch `requires_reauth`.
    #[test]
    fn only_401_maps_to_auth_required() {
        let spec = spec_without_hint();

        assert!(matches!(
            map_err(spec, http(401)),
            UsageError::AuthRequired
        ));

        for status in [403, 404, 429, 500, 503] {
            let mapped = map_err(spec, http(status));
            assert!(
                !matches!(mapped, UsageError::AuthRequired),
                "{status} must not be an auth verdict, got {mapped:?}"
            );
        }
    }

    #[test]
    fn throttling_and_outages_are_retryable_but_403_is_not() {
        let spec = spec_without_hint();

        assert!(map_err(spec, http(429)).is_transient());
        assert!(map_err(spec, http(503)).is_transient());
        assert!(
            !map_err(spec, http(403)).is_transient(),
            "a 403 block will not clear by retrying"
        );
    }

    /// Every API-key-tier catalog entry must have a balance spec in
    /// `skillstar-providers`, and vice versa — this pins the two tables together
    /// so they can no longer drift apart.
    #[test]
    fn api_key_catalog_and_balance_specs_stay_in_sync() {
        use std::collections::BTreeSet;

        let catalog_api_key_ids: BTreeSet<&str> = catalog()
            .iter()
            .filter(|e| e.tier == CatalogTier::ApiKey)
            .map(|e| e.id)
            .collect();
        let spec_ids: BTreeSet<&str> = skillstar_providers::balance::API_KEY_BALANCE_SPECS
            .iter()
            .map(|s| s.catalog_id)
            .collect();

        assert_eq!(
            catalog_api_key_ids, spec_ids,
            "ApiKey catalog entries and balance specs must match exactly"
        );
    }
}

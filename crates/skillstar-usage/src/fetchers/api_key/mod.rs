//! API-key fetchers (DeepSeek, GLM, MiniMax, Kimi).
//!
//! All read `Subscription.fingerprint_id`; when set, the request goes
//! through a [`FingerprintAwareClient`] built from the stored fingerprint
//! (TLS/HTTP-2 emulation via wreq). When unset, the legacy reqwest path is
//! used — behaviour is unchanged for existing subscriptions.
//!
//! The request path itself is identical across all four providers (build the
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
use skillstar_fingerprint::{DeviceFingerprint, Req, RequestError};
use skillstar_providers::balance::{AuthScheme, BalanceSpec};

use crate::crypto;
use crate::http_client::{load_fingerprint, usage_client_with_fingerprint};
use crate::subscription::{Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

/// Dispatch an API-key refresh based on `subscription.catalog_id`.
///
/// Resolves the optional fingerprint *once* up front and threads it through
/// to the per-provider fetcher so the client builder cache stays warm.
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

    let fingerprint = load_fingerprint(subscription.fingerprint_id.as_deref())?;
    let fp = fingerprint.as_ref();

    match subscription.catalog_id.as_str() {
        "deepseek" => deepseek::fetch(subscription, &api_key, fp).await,
        "glm" => glm::fetch(&subscription.id, &api_key, fp).await,
        "minimax" => minimax::fetch(&subscription.id, &api_key, fp).await,
        "kimi" => kimi::fetch(&subscription.id, &api_key, fp).await,
        other => Err(super::unsupported(other)),
    }
}

/// Shared request path for every API-key balance fetcher.
///
/// Builds the (optionally fingerprinted) client, attaches the key per the
/// spec's [`AuthScheme`], issues the GET, and decodes the JSON body into `T`.
/// Transport errors are mapped uniformly via [`map_err`]. Each caller picks the
/// concrete `T` matching that provider's response shape.
pub(super) async fn fetch_spec<T: DeserializeOwned>(
    spec: &BalanceSpec,
    api_key: &str,
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<T> {
    let client = usage_client_with_fingerprint(fingerprint)
        .map_err(|e| UsageError::Fetcher(format!("{} client: {e}", spec.display_name)))?;

    let req = Req::get(&client, spec.endpoint).header("Accept", "application/json");
    let req = match spec.auth {
        AuthScheme::Bearer => req.bearer(api_key),
        AuthScheme::RawHeader(name) => req.header(name, api_key),
    };

    req.send_json::<T>().await.map_err(|e| map_err(spec, e))
}

/// Uniform transport-error mapping for API-key fetchers.
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
        RequestError::HttpStatus { status, body } => UsageError::Fetcher(format!(
            "{} 返回 {status}: {}",
            spec.display_name,
            body.chars().take(200).collect::<String>()
        )),
        RequestError::JsonDecode { source, .. } => {
            UsageError::Fetcher(format!("{} 响应解析失败：{source}", spec.display_name))
        }
        other => UsageError::Fetcher(format!("{} 请求失败：{other}", spec.display_name)),
    }
}

#[cfg(test)]
mod tests {
    use crate::catalog::{CatalogTier, catalog};

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

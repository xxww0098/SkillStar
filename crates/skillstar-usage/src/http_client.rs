//! Proxy- and fingerprint-aware HTTP clients used by usage fetchers and
//! OAuth flows.
//!
//! Two flavours:
//!
//! * [`usage_http_client`] returns a plain `reqwest::Client` with proxy
//!   settings from `~/.skillstar/config/proxy.json`. Used by legacy fetchers
//!   that haven't been migrated to fingerprint-aware mode yet.
//!
//! * [`usage_client_with_fingerprint`] returns a [`FingerprintAwareClient`]
//!   wrapped around either `reqwest` (when the fingerprint requests
//!   [`TlsProfile::Default`]) or `wreq` (when it requests browser
//!   emulation).  Use this for any fetcher that wants TLS / HTTP-2 / header
//!   identity emulation.

use std::time::Duration;

use skillstar_fingerprint::{
    DeviceFingerprint, FingerprintAwareClient, FingerprintStore, HttpProfile, TlsProfile,
    build_client_with_timeout,
};

use crate::{UsageError, UsageResult};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

tokio::task_local! {
    /// Active fingerprint id for the current async task tree.
    ///
    /// OAuth fetchers set this once in their top-level `fetch()` via
    /// [`with_fingerprint`]; any nested helper that later calls
    /// [`usage_reqwest_with_active_fingerprint`] picks it up implicitly,
    /// so we don't have to thread an `Option<&str>` through every helper.
    static CURRENT_FINGERPRINT_ID: Option<String>;
}

/// Run an async block with the given fingerprint id as the current
/// task-local context. Used by OAuth fetchers to scope a fetch run.
pub async fn with_fingerprint<F, T>(fingerprint_id: Option<String>, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    CURRENT_FINGERPRINT_ID.scope(fingerprint_id, fut).await
}

/// The fingerprint id currently in scope, if any. Returns `None` outside
/// of a [`with_fingerprint`] block.
pub fn current_fingerprint_id() -> Option<String> {
    CURRENT_FINGERPRINT_ID
        .try_with(|x| x.clone())
        .ok()
        .flatten()
}

/// Variant of [`usage_reqwest_with_fingerprint`] that reads the
/// `task_local!` fingerprint set by [`with_fingerprint`]. Callers that
/// don't know the fingerprint id explicitly (e.g. nested OAuth helpers)
/// should use this.
pub fn usage_reqwest_with_active_fingerprint() -> UsageResult<reqwest::Client> {
    let id = current_fingerprint_id();
    usage_reqwest_with_fingerprint(id.as_deref())
}

/// Legacy entry point — returns a plain reqwest client with SkillStar proxy
/// applied. New fetchers should prefer [`usage_client_with_fingerprint`] or
/// [`usage_reqwest_with_fingerprint`].
pub fn usage_http_client() -> UsageResult<reqwest::Client> {
    skillstar_core::infra::http_client::probe_http_client(DEFAULT_TIMEOUT)
        .map_err(|e| UsageError::Other(format!("http client: {e}")))
}

/// Build a `reqwest::Client` that carries the HTTP-layer identity of the
/// given fingerprint (User-Agent, Accept-Language, Sec-CH-UA, etc.).
///
/// **TLS layer note**: this path still uses reqwest's default rustls
/// ClientHello — to also swap the TLS fingerprint use
/// [`usage_client_with_fingerprint`] (which returns a `FingerprintAwareClient`
/// backed by `wreq` when the profile requests browser emulation).
///
/// `fingerprint_id == None` reproduces the historical behaviour
/// ([`usage_http_client`]) — only proxy settings applied.
pub fn usage_reqwest_with_fingerprint(
    fingerprint_id: Option<&str>,
) -> UsageResult<reqwest::Client> {
    usage_reqwest_with_fingerprint_timeout(fingerprint_id, DEFAULT_TIMEOUT)
}

/// Variant of [`usage_reqwest_with_fingerprint`] with a custom timeout.
pub fn usage_reqwest_with_fingerprint_timeout(
    fingerprint_id: Option<&str>,
    timeout: Duration,
) -> UsageResult<reqwest::Client> {
    // No fingerprint requested — return the cached proxy-aware client untouched.
    let fp = load_fingerprint(fingerprint_id)?;
    let Some(fp) = fp else {
        return skillstar_core::infra::http_client::probe_http_client(timeout)
            .map_err(|e| UsageError::Other(format!("http client: {e}")));
    };

    build_reqwest_with_http_profile(&fp.http, timeout)
}

fn build_reqwest_with_http_profile(
    http: &HttpProfile,
    timeout: Duration,
) -> UsageResult<reqwest::Client> {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10))
        .user_agent(&http.user_agent);

    // Honour user proxy config the same way `probe_http_client` does.
    if let Ok(cfg) = skillstar_core::config::proxy::load_config()
        && cfg.enabled
        && !cfg.host.trim().is_empty()
    {
        let url = format!(
            "{}://{}:{}",
            cfg.proxy_type.as_scheme(),
            cfg.host.trim(),
            cfg.port
        );
        let mut p = reqwest::Proxy::all(&url)
            .map_err(|e| UsageError::Other(format!("invalid proxy: {e}")))?;
        if let Some(user) = cfg.username.as_deref() {
            p = p.basic_auth(user, cfg.password.as_deref().unwrap_or(""));
        }
        builder = builder.proxy(p);
    }

    let mut headers = HeaderMap::new();
    let push = |map: &mut HeaderMap, name: &str, value: &str| -> UsageResult<()> {
        let n = HeaderName::from_bytes(name.as_bytes())
            .map_err(|e| UsageError::Other(format!("bad header name `{name}`: {e}")))?;
        let v = HeaderValue::from_str(value)
            .map_err(|e| UsageError::Other(format!("bad header value: {e}")))?;
        map.insert(n, v);
        Ok(())
    };

    push(&mut headers, "Accept-Language", &http.accept_language)?;
    push(&mut headers, "Accept-Encoding", &http.accept_encoding)?;
    if let Some(v) = &http.sec_ch_ua {
        push(&mut headers, "Sec-CH-UA", v)?;
    }
    if let Some(v) = &http.sec_ch_ua_platform {
        push(&mut headers, "Sec-CH-UA-Platform", v)?;
    }
    if http.sec_ch_ua_mobile {
        push(&mut headers, "Sec-CH-UA-Mobile", "?1")?;
    }
    for (k, v) in &http.extra_headers {
        push(&mut headers, k, v)?;
    }

    if !headers.is_empty() {
        builder = builder.default_headers(headers);
    }

    builder
        .build()
        .map_err(|e| UsageError::Other(format!("failed to build reqwest client: {e}")))
}

/// Resolve a fingerprint by id from the on-disk store. `None` means "no
/// specific fingerprint requested" — callers should treat that as the
/// reqwest default.
pub fn load_fingerprint(id: Option<&str>) -> UsageResult<Option<DeviceFingerprint>> {
    let Some(id) = id else { return Ok(None) };
    let store = FingerprintStore::load_default()
        .map_err(|e| UsageError::Other(format!("fingerprint store: {e}")))?;
    match store.get(id) {
        Ok(fp) => Ok(Some(fp.clone())),
        Err(e) => {
            tracing::warn!(
                target: "skillstar_usage::http",
                fingerprint = id,
                error = %e,
                "fingerprint not found in store, falling back to default",
            );
            Ok(None)
        }
    }
}

/// Build a fingerprint-aware client.
///
/// When `fingerprint` is `None` (or it requests [`TlsProfile::Default`]) the
/// returned client wraps the same `reqwest` client SkillStar shipped before
/// fingerprinting — so existing fetchers see no behaviour change.
pub fn usage_client_with_fingerprint(
    fingerprint: Option<&DeviceFingerprint>,
) -> UsageResult<FingerprintAwareClient> {
    usage_client_with_fingerprint_timeout(fingerprint, DEFAULT_TIMEOUT)
}

/// Like [`usage_client_with_fingerprint`] with a custom timeout (matches the
/// per-fetcher timeouts that some providers need, e.g. 10s for GLM).
pub fn usage_client_with_fingerprint_timeout(
    fingerprint: Option<&DeviceFingerprint>,
    timeout: Duration,
) -> UsageResult<FingerprintAwareClient> {
    let resolved = fingerprint
        .cloned()
        .unwrap_or_else(DeviceFingerprint::original);

    // If the fingerprint resolves to the default TLS profile we want to
    // reuse the existing proxy-aware reqwest client (which is also cached
    // and respects user proxy config). Only branch into wreq when the user
    // explicitly opted into browser emulation.
    if matches!(resolved.tls, TlsProfile::Default) {
        let client = skillstar_core::infra::http_client::probe_http_client(timeout)
            .map_err(|e| UsageError::Other(format!("http client: {e}")))?;
        return Ok(FingerprintAwareClient::Reqwest(client));
    }

    build_client_with_timeout(&resolved, timeout)
        .map_err(|e| UsageError::Other(format!("fingerprint client: {e:#}")))
}

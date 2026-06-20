//! Fingerprint-aware HTTP client factory.
//!
//! This is the single entry point fetchers should use to obtain an HTTP
//! client that honours a [`DeviceFingerprint`]. The factory transparently
//! picks between `reqwest` (for `TlsProfile::Default`) and `wreq` (for any
//! browser emulation profile).
//!
//! ## Why an enum and not a trait?
//!
//! `reqwest::Client` and `wreq::Client` have very similar but *not* identical
//! APIs (request builder return types differ, error types differ). Wrapping
//! them in an enum lets every fetcher write idiomatic code for the layer it
//! actually uses, while letting the factory pick the right backend.

use crate::{DeviceFingerprint, TlsProfile};
use anyhow::{Context, Result};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// HTTP client that may be backed by either `reqwest` or `wreq`,
/// depending on the fingerprint's [`TlsProfile`].
///
/// `Debug` is implemented manually because `wreq::Client` doesn't derive it.
#[derive(Clone)]
pub enum FingerprintAwareClient {
    /// Plain reqwest — used when [`TlsProfile::Default`] is selected.
    Reqwest(reqwest::Client),

    /// wreq client emulating a browser TLS/H2 fingerprint.
    #[cfg(feature = "impersonate")]
    Wreq(wreq::Client),
}

impl std::fmt::Debug for FingerprintAwareClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FingerprintAwareClient")
            .field("backend", &self.backend())
            .finish()
    }
}

impl FingerprintAwareClient {
    /// Short label suitable for logs / metrics.
    pub fn backend(&self) -> &'static str {
        match self {
            Self::Reqwest(_) => "reqwest",
            #[cfg(feature = "impersonate")]
            Self::Wreq(_) => "wreq",
        }
    }

    /// Convenience getter for the reqwest variant (returns `None` for wreq).
    pub fn as_reqwest(&self) -> Option<&reqwest::Client> {
        match self {
            Self::Reqwest(c) => Some(c),
            #[cfg(feature = "impersonate")]
            Self::Wreq(_) => None,
        }
    }

    /// Convenience getter for the wreq variant.
    #[cfg(feature = "impersonate")]
    pub fn as_wreq(&self) -> Option<&wreq::Client> {
        match self {
            Self::Reqwest(_) => None,
            Self::Wreq(c) => Some(c),
        }
    }
}

/// Build a fingerprint-aware client.
///
/// Returns a `FingerprintAwareClient` whose backend matches
/// `fingerprint.tls`.  The HTTP-layer headers (UA, Accept-Language, etc.)
/// from `fingerprint.http` are baked in as default headers.
pub fn build_client(fingerprint: &DeviceFingerprint) -> Result<FingerprintAwareClient> {
    build_client_with_timeout(fingerprint, DEFAULT_TIMEOUT)
}

/// Like [`build_client`] but with a custom request timeout.
pub fn build_client_with_timeout(
    fingerprint: &DeviceFingerprint,
    timeout: Duration,
) -> Result<FingerprintAwareClient> {
    match &fingerprint.tls {
        TlsProfile::Default => build_reqwest(fingerprint, timeout),
        #[cfg(feature = "impersonate")]
        _ => build_wreq(fingerprint, timeout),
        #[cfg(not(feature = "impersonate"))]
        other => {
            tracing::warn!(
                target: "skillstar_fingerprint",
                profile = %other.label(),
                "impersonate feature disabled — falling back to reqwest",
            );
            build_reqwest(fingerprint, timeout)
        }
    }
}

fn build_reqwest(
    fingerprint: &DeviceFingerprint,
    timeout: Duration,
) -> Result<FingerprintAwareClient> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .user_agent(&fingerprint.http.user_agent);

    let mut headers = reqwest::header::HeaderMap::new();
    push_header(
        &mut headers,
        "Accept-Language",
        &fingerprint.http.accept_language,
    )?;
    push_header(
        &mut headers,
        "Accept-Encoding",
        &fingerprint.http.accept_encoding,
    )?;
    if let Some(v) = &fingerprint.http.sec_ch_ua {
        push_header(&mut headers, "Sec-CH-UA", v)?;
    }
    if let Some(v) = &fingerprint.http.sec_ch_ua_platform {
        push_header(&mut headers, "Sec-CH-UA-Platform", v)?;
    }
    if fingerprint.http.sec_ch_ua_mobile {
        push_header(&mut headers, "Sec-CH-UA-Mobile", "?1")?;
    }
    for (k, v) in &fingerprint.http.extra_headers {
        push_header(&mut headers, k, v)?;
    }

    if !headers.is_empty() {
        builder = builder.default_headers(headers);
    }

    if let Some(proxy_url) = fingerprint.network.proxy_url.as_deref() {
        let proxy = reqwest::Proxy::all(proxy_url).context("invalid proxy URL")?;
        builder = builder.proxy(proxy);
    }

    let client = builder.build().context("failed to build reqwest client")?;
    Ok(FingerprintAwareClient::Reqwest(client))
}

fn push_header(map: &mut reqwest::header::HeaderMap, name: &str, value: &str) -> Result<()> {
    use reqwest::header::{HeaderName, HeaderValue};
    let n = HeaderName::from_bytes(name.as_bytes())
        .with_context(|| format!("invalid header name `{name}`"))?;
    let v = HeaderValue::from_str(value)
        .with_context(|| format!("invalid header value for `{name}`"))?;
    map.insert(n, v);
    Ok(())
}

#[cfg(feature = "impersonate")]
fn build_wreq(
    fingerprint: &DeviceFingerprint,
    timeout: Duration,
) -> Result<FingerprintAwareClient> {
    let emulation = fingerprint
        .tls
        .to_emulation()
        .context("TLS profile has no wreq-util Emulation mapping")?;

    let builder = wreq::Client::builder()
        .timeout(timeout)
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .emulation(emulation)
        .user_agent(&fingerprint.http.user_agent);

    let mut headers = wreq::header::HeaderMap::new();
    push_wreq_header(
        &mut headers,
        "Accept-Language",
        &fingerprint.http.accept_language,
    )?;
    push_wreq_header(
        &mut headers,
        "Accept-Encoding",
        &fingerprint.http.accept_encoding,
    )?;
    if let Some(v) = &fingerprint.http.sec_ch_ua {
        push_wreq_header(&mut headers, "Sec-CH-UA", v)?;
    }
    if let Some(v) = &fingerprint.http.sec_ch_ua_platform {
        push_wreq_header(&mut headers, "Sec-CH-UA-Platform", v)?;
    }
    if fingerprint.http.sec_ch_ua_mobile {
        push_wreq_header(&mut headers, "Sec-CH-UA-Mobile", "?1")?;
    }
    for (k, v) in &fingerprint.http.extra_headers {
        push_wreq_header(&mut headers, k, v)?;
    }

    let builder = if headers.is_empty() {
        builder
    } else {
        builder.default_headers(headers)
    };

    let builder = if let Some(proxy_url) = fingerprint.network.proxy_url.as_deref() {
        let proxy = wreq::Proxy::all(proxy_url).context("invalid proxy URL")?;
        builder.proxy(proxy)
    } else {
        builder
    };

    let client = builder.build().context("failed to build wreq client")?;
    Ok(FingerprintAwareClient::Wreq(client))
}

#[cfg(feature = "impersonate")]
fn push_wreq_header(map: &mut wreq::header::HeaderMap, name: &str, value: &str) -> Result<()> {
    use wreq::header::{HeaderName, HeaderValue};
    let n = HeaderName::from_bytes(name.as_bytes())
        .with_context(|| format!("invalid header name `{name}`"))?;
    let v = HeaderValue::from_str(value)
        .with_context(|| format!("invalid header value for `{name}`"))?;
    map.insert(n, v);
    Ok(())
}

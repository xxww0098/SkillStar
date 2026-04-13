//! Shared HTTP client with automatic proxy configuration and connection pooling.
//!
//! The client caches TLS sessions and HTTP/2 connections, and auto-refreshes
//! when the user's proxy settings change.

use anyhow::{Context, Result};
use std::sync::{LazyLock, Mutex};

use crate::core::config::proxy;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProxyFingerprint {
    enabled: bool,
    scheme: String,
    host: String,
    port: u16,
    username: String,
    password: String,
}

impl ProxyFingerprint {
    fn from_config(config: &proxy::ProxyConfig) -> Self {
        Self {
            enabled: config.enabled && !config.host.trim().is_empty(),
            scheme: config.proxy_type.as_scheme().to_string(),
            host: config.host.trim().to_string(),
            port: config.port,
            username: config
                .username
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_string(),
            password: config.password.clone().unwrap_or_default(),
        }
    }
}

static SHARED_HTTP_CLIENT: LazyLock<Mutex<Option<(ProxyFingerprint, reqwest::Client)>>> =
    LazyLock::new(|| Mutex::new(None));

fn current_proxy_fingerprint() -> ProxyFingerprint {
    match proxy::load_config() {
        Ok(config) => ProxyFingerprint::from_config(&config),
        Err(_) => ProxyFingerprint {
            enabled: false,
            scheme: "http".to_string(),
            host: String::new(),
            port: 7897,
            username: String::new(),
            password: String::new(),
        },
    }
}

/// Build a reqwest client, optionally honouring the user's proxy config.
fn build_http_client_inner(fingerprint: &ProxyFingerprint) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        // Total request timeout (covers entire request lifecycle).
        .timeout(std::time::Duration::from_secs(120))
        // Fast-fail on network-unreachable / DNS-timeout scenarios
        // instead of waiting the full 120s.
        .connect_timeout(std::time::Duration::from_secs(10))
        // Keep idle connections alive to reuse TLS sessions.
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        // Match the AI concurrency budget default.
        .pool_max_idle_per_host(4);

    if fingerprint.enabled {
        let proxy_url = format!(
            "{}://{}:{}",
            fingerprint.scheme, fingerprint.host, fingerprint.port
        );

        let mut proxy = reqwest::Proxy::all(&proxy_url).context("Invalid proxy URL")?;
        if !fingerprint.username.is_empty() {
            proxy = proxy.basic_auth(&fingerprint.username, &fingerprint.password);
        }

        builder = builder.proxy(proxy);
    }

    builder.build().context("Failed to build HTTP client")
}

/// Get or lazily create the shared HTTP client.  Reuses TLS sessions and
/// HTTP/2 connections between requests — eliminates ~100-200ms per request.
/// The cache auto-refreshes when proxy settings change.
pub(super) fn get_http_client() -> Result<reqwest::Client> {
    let fingerprint = current_proxy_fingerprint();
    let mut guard = SHARED_HTTP_CLIENT
        .lock()
        .map_err(|_| anyhow::anyhow!("HTTP client cache lock poisoned"))?;

    if let Some((cached_fp, client)) = guard.as_ref() {
        if *cached_fp == fingerprint {
            return Ok(client.clone());
        }
    }

    let rebuilt = build_http_client_inner(&fingerprint)
        .with_context(|| "Failed to build HTTP client with current proxy settings")?;
    *guard = Some((fingerprint, rebuilt.clone()));
    Ok(rebuilt)
}

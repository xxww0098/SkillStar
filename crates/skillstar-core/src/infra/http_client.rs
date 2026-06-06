//! Proxy-aware `reqwest` client used by models latency probes and other HTTP calls.

use anyhow::{Context, Result};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use crate::config::proxy;

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

static SHARED_PROBE_CLIENT: LazyLock<Mutex<Option<(ProxyFingerprint, Duration, reqwest::Client)>>> =
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

fn build_client(fingerprint: &ProxyFingerprint, timeout: Duration) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10));

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

/// Shared HTTP client with SkillStar proxy settings from `~/.skillstar/config/proxy.json`.
pub fn probe_http_client(timeout: Duration) -> Result<reqwest::Client> {
    let fingerprint = current_proxy_fingerprint();
    let mut guard = SHARED_PROBE_CLIENT
        .lock()
        .map_err(|_| anyhow::anyhow!("HTTP client cache lock poisoned"))?;

    if let Some((cached_fp, cached_timeout, client)) = guard.as_ref()
        && *cached_fp == fingerprint && *cached_timeout == timeout {
            return Ok(client.clone());
        }

    let rebuilt = build_client(&fingerprint, timeout)?;
    *guard = Some((fingerprint, timeout, rebuilt.clone()));
    Ok(rebuilt)
}

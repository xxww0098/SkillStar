//! Proxy-aware `reqwest` client used by models latency probes and other HTTP calls.

use anyhow::{Context, Result};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use crate::config::proxy;

/// Canonical form of `ProxyConfig::bypass` — comma-separated entries, each
/// trimmed, empties dropped. Empty output means "no bypass list".
///
/// Normalizing here rather than at `build_client` time serves both jobs the
/// field has: `NoProxy::from_string("")` in reqwest 0.13 returns
/// `Some(<matches nothing>)` instead of `None`, so an empty or comma-only value
/// must be rejected before it reaches reqwest; and the fingerprint must not
/// churn the shared client just because the user retyped
/// `localhost, 127.0.0.1` with different spacing.
fn normalize_bypass(raw: &str) -> String {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProxyFingerprint {
    enabled: bool,
    scheme: String,
    host: String,
    port: u16,
    username: String,
    password: String,
    /// Normalized no-proxy list (see [`normalize_bypass`]). Part of the
    /// fingerprint on purpose: `probe_http_client` caches by
    /// `(fingerprint, timeout)`, so a bypass edit that is not represented here
    /// would be saved to disk and then never reach a client.
    bypass: String,
}

impl ProxyFingerprint {
    fn from_config(config: &proxy::ProxyConfig) -> Self {
        Self {
            enabled: config.enabled && !config.host.trim().is_empty(),
            scheme: config.proxy_type.as_transport_scheme().to_string(),
            host: config.host.trim().to_string(),
            port: config.port,
            username: config
                .username
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_string(),
            password: config.password.clone().unwrap_or_default(),
            bypass: normalize_bypass(config.bypass.as_deref().unwrap_or_default()),
        }
    }
}

static SHARED_PROBE_CLIENT: LazyLock<Mutex<Option<(ProxyFingerprint, Duration, reqwest::Client)>>> =
    LazyLock::new(|| Mutex::new(None));

static SHARED_STREAM_CLIENT: LazyLock<Mutex<Option<(ProxyFingerprint, reqwest::Client)>>> =
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
            bypass: String::new(),
        },
    }
}

/// Connect budget shared by probes and the streaming client.
pub const STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

fn apply_proxy(
    builder: reqwest::ClientBuilder,
    fingerprint: &ProxyFingerprint,
) -> Result<reqwest::ClientBuilder> {
    if !fingerprint.enabled {
        return Ok(builder);
    }
    let proxy_url = format!(
        "{}://{}:{}",
        fingerprint.scheme, fingerprint.host, fingerprint.port
    );
    let mut proxy = reqwest::Proxy::all(&proxy_url).context("Invalid proxy URL")?;
    if !fingerprint.username.is_empty() {
        proxy = proxy.basic_auth(&fingerprint.username, &fingerprint.password);
    }
    // Hosts the user excluded from the proxy. `NoProxy::from_string` parses
    // lazily and hands back `Some` for anything, including `""` — a list
    // that matches nothing — so the empty case is filtered out here rather
    // than relying on that `Option`. Accepted entry forms are reqwest's:
    // domains (`example.com`, `.example.com`, matching subdomains too),
    // literal IPs, CIDR blocks (`10.0.0.0/8`), and the single wildcard `*`.
    if !fingerprint.bypass.is_empty() {
        proxy = proxy.no_proxy(reqwest::NoProxy::from_string(&fingerprint.bypass));
    }
    Ok(builder.proxy(proxy))
}

fn pooled(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder
        .connect_timeout(STREAM_CONNECT_TIMEOUT)
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(4)
}

/// Total timeout for a streaming call. `None` means the body is not capped.
fn stream_total_timeout() -> Option<Duration> {
    None
}

fn build_client(fingerprint: &ProxyFingerprint, timeout: Duration) -> Result<reqwest::Client> {
    let builder = pooled(reqwest::Client::builder().timeout(timeout));
    apply_proxy(builder, fingerprint)?
        .build()
        .context("Failed to build HTTP client")
}

fn build_stream_client(fingerprint: &ProxyFingerprint) -> Result<reqwest::Client> {
    let mut builder = pooled(reqwest::Client::builder());
    if let Some(total) = stream_total_timeout() {
        builder = builder.timeout(total);
    }
    apply_proxy(builder, fingerprint)?
        .build()
        .context("Failed to build streaming HTTP client")
}

/// Shared HTTP client with SkillStar proxy settings from `~/.skillstar/config/proxy.json`.
pub fn probe_http_client(timeout: Duration) -> Result<reqwest::Client> {
    let fingerprint = current_proxy_fingerprint();
    let mut guard = SHARED_PROBE_CLIENT
        .lock()
        .map_err(|_| anyhow::anyhow!("HTTP client cache lock poisoned"))?;

    if let Some((cached_fp, cached_timeout, client)) = guard.as_ref()
        && *cached_fp == fingerprint
        && *cached_timeout == timeout
    {
        return Ok(client.clone());
    }

    let rebuilt = build_client(&fingerprint, timeout)?;
    *guard = Some((fingerprint, timeout, rebuilt.clone()));
    Ok(rebuilt)
}

/// Streaming client with the same `proxy.json` fingerprint as [`probe_http_client`].
///
/// Connect is capped at [`STREAM_CONNECT_TIMEOUT`]. There is no header deadline
/// and no total timeout: a long body stays open.
pub fn stream_http_client() -> Result<reqwest::Client> {
    let fingerprint = current_proxy_fingerprint();
    let mut guard = SHARED_STREAM_CLIENT
        .lock()
        .map_err(|_| anyhow::anyhow!("HTTP client cache lock poisoned"))?;

    if let Some((cached_fp, client)) = guard.as_ref()
        && *cached_fp == fingerprint
    {
        return Ok(client.clone());
    }

    let rebuilt = build_stream_client(&fingerprint)?;
    *guard = Some((fingerprint, rebuilt.clone()));
    Ok(rebuilt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::proxy::{ProxyConfig, ProxyType};
    use tempfile::TempDir;

    fn proxied(bypass: Option<&str>) -> ProxyConfig {
        ProxyConfig {
            enabled: true,
            proxy_type: ProxyType::Http,
            host: "127.0.0.1".into(),
            port: 7897,
            username: None,
            password: None,
            bypass: bypass.map(str::to_string),
        }
    }

    #[test]
    fn normalize_bypass_drops_blank_and_empty_entries() {
        // Nothing usable: reqwest must never see these, an all-empty
        // `NoProxy` still counts as a configured (but matchless) list.
        assert_eq!(normalize_bypass(""), "");
        assert_eq!(normalize_bypass("   "), "");
        assert_eq!(normalize_bypass(",,,"), "");
        assert_eq!(normalize_bypass(" , , "), "");

        // Spacing is cosmetic, so it must not survive into the fingerprint.
        assert_eq!(
            normalize_bypass(" localhost , 127.0.0.1 "),
            "localhost,127.0.0.1"
        );
        assert_eq!(
            normalize_bypass("localhost,,127.0.0.1"),
            "localhost,127.0.0.1"
        );

        // Entry shapes reqwest accepts are passed through verbatim.
        assert_eq!(normalize_bypass("*"), "*");
        assert_eq!(normalize_bypass(".example.com"), ".example.com");
        assert_eq!(
            normalize_bypass("10.0.0.0/8, 192.168.1.0/24"),
            "10.0.0.0/8,192.168.1.0/24"
        );
        assert_eq!(normalize_bypass("::1, fd00::/8"), "::1,fd00::/8");
    }

    #[test]
    fn fingerprint_changes_when_bypass_changes() {
        let none = ProxyFingerprint::from_config(&proxied(None));
        let local = ProxyFingerprint::from_config(&proxied(Some("localhost")));
        let wider = ProxyFingerprint::from_config(&proxied(Some("localhost,127.0.0.1")));

        // Each of these is a different routing decision, so each must miss the
        // `(fingerprint, timeout)` cache in `probe_http_client`.
        assert_ne!(none, local);
        assert_ne!(local, wider);
        assert_ne!(none, wider);
    }

    #[test]
    fn fingerprint_ignores_bypass_formatting_noise() {
        // Same routing decision written differently: rebuilding here would
        // throw away a live connection pool for nothing.
        assert_eq!(
            ProxyFingerprint::from_config(&proxied(Some("localhost,127.0.0.1"))),
            ProxyFingerprint::from_config(&proxied(Some(" localhost , 127.0.0.1 ")))
        );
        assert_eq!(
            ProxyFingerprint::from_config(&proxied(None)),
            ProxyFingerprint::from_config(&proxied(Some("  ,  ")))
        );
    }

    #[test]
    fn build_client_accepts_every_supported_bypass_shape() {
        for raw in [
            "",
            ",,,",
            "localhost",
            " localhost , 127.0.0.1 ",
            "*",
            ".example.com",
            "10.0.0.0/8,192.168.1.0/24",
            "::1,fd00::/8",
        ] {
            let fingerprint = ProxyFingerprint::from_config(&proxied(Some(raw)));
            assert!(
                build_client(&fingerprint, Duration::from_secs(5)).is_ok(),
                "bypass {raw:?} should still yield a usable client"
            );
        }
    }

    #[test]
    fn fingerprint_uses_socks5h_transport_for_both_socks_types() {
        let socks5 = ProxyFingerprint::from_config(&ProxyConfig {
            enabled: true,
            proxy_type: ProxyType::Socks5,
            host: "127.0.0.1".into(),
            port: 1080,
            username: None,
            password: None,
            bypass: None,
        });
        let socks5h = ProxyFingerprint::from_config(&ProxyConfig {
            enabled: true,
            proxy_type: ProxyType::Socks5h,
            host: "127.0.0.1".into(),
            port: 1080,
            username: None,
            password: None,
            bypass: None,
        });
        assert_eq!(socks5.scheme, "socks5h");
        assert_eq!(socks5h.scheme, "socks5h");
        assert_eq!(socks5, socks5h);
        assert!(build_client(&socks5, Duration::from_secs(5)).is_ok());
    }

    #[test]
    fn probe_http_client_rebuilds_after_bypass_edit() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();

        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let timeout = Duration::from_secs(11);

        proxy::save_config(&proxied(Some("localhost"))).unwrap();
        probe_http_client(timeout).unwrap();
        let first = SHARED_PROBE_CLIENT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|(fp, _, _)| fp.clone())
            .expect("client cached after first build");
        assert_eq!(first.bypass, "localhost");

        proxy::save_config(&proxied(Some("localhost,10.0.0.0/8"))).unwrap();
        probe_http_client(timeout).unwrap();
        let second = SHARED_PROBE_CLIENT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|(fp, _, _)| fp.clone())
            .expect("client cached after second build");

        // The cached entry moved: the edit was not swallowed by the cache.
        assert_eq!(second.bypass, "localhost,10.0.0.0/8");
        assert_ne!(first, second);

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    fn cached_probe_fingerprint() -> ProxyFingerprint {
        SHARED_PROBE_CLIENT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|(fingerprint, _, _)| fingerprint.clone())
            .expect("probe client cached")
    }

    fn cached_stream_fingerprint() -> ProxyFingerprint {
        SHARED_STREAM_CLIENT
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|(fingerprint, _)| fingerprint.clone())
            .expect("stream client cached")
    }

    #[test]
    fn stream_client_uses_probe_proxy_fingerprint() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        assert_eq!(STREAM_CONNECT_TIMEOUT, Duration::from_secs(10));
        assert!(stream_total_timeout().is_none());

        proxy::save_config(&proxied(Some("localhost,10.1.0.0/16"))).unwrap();
        probe_http_client(Duration::from_secs(11)).unwrap();
        stream_http_client().unwrap();
        let probe = cached_probe_fingerprint();
        let stream = cached_stream_fingerprint();
        assert_eq!(probe, stream);
        assert_eq!(stream.bypass, "localhost,10.1.0.0/16");
        assert!(stream.enabled);

        proxy::save_config(&proxied(Some("127.0.0.1"))).unwrap();
        probe_http_client(Duration::from_secs(11)).unwrap();
        stream_http_client().unwrap();
        let probe_after = cached_probe_fingerprint();
        let stream_after = cached_stream_fingerprint();
        assert_eq!(probe_after, stream_after);
        assert_ne!(stream, stream_after);
        assert_eq!(stream_after.bypass, "127.0.0.1");

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}

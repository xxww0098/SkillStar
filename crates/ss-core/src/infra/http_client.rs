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

static SHARED_LOOPBACK_CLIENT: LazyLock<Mutex<Option<(Duration, reqwest::Client)>>> =
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

/// Connect budget for pooled HTTP clients.
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
    // Clear reqwest's implicit `auto_sys_proxy` first so user bypass entries
    // are never re-intercepted by an external OS or environment proxy.
    Ok(builder.no_proxy().proxy(proxy))
}

fn pooled(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder
        .connect_timeout(STREAM_CONNECT_TIMEOUT)
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(4)
}

fn build_client(fingerprint: &ProxyFingerprint, timeout: Duration) -> Result<reqwest::Client> {
    let builder = pooled(reqwest::Client::builder().timeout(timeout));
    apply_proxy(builder, fingerprint)?
        .build()
        .context("Failed to build HTTP client")
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

fn is_loopback_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_loopback(),
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    }
}

/// Return `true` when `url` targets a loopback host (`localhost`, `*.localhost`,
/// `127.0.0.0/8`, `::1`, `[::1]`, or IPv4-mapped `::ffff:127.x.y.z`).
pub fn is_loopback_url(url: &str) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return false;
    }

    let authority = if let Some((_, rest)) = trimmed.split_once("://") {
        rest.split(['/', '?', '#']).next().unwrap_or("")
    } else if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        return is_loopback_ip(ip);
    } else {
        trimmed.split(['/', '?', '#']).next().unwrap_or("")
    };

    let host_port = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    let host = if let Some(after_open) = host_port.strip_prefix('[') {
        let Some((inner, after_close)) = after_open.split_once(']') else {
            return false;
        };
        if !after_close.is_empty() && !after_close.starts_with(':') {
            return false;
        }
        inner
    } else if host_port.parse::<std::net::IpAddr>().is_ok() {
        host_port
    } else if let Some((h, port)) = host_port.split_once(':') {
        if !port.is_empty() && !port.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        h
    } else {
        host_port
    };

    let host = host.trim_end_matches('.');
    if host.is_empty() {
        return false;
    }
    if host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .is_ok_and(is_loopback_ip)
}

/// HTTP client appropriate for `url`: bypasses system and environment proxies
/// for loopback targets while keeping [`probe_http_client`] behavior for all
/// remote destinations.
pub fn probe_http_client_for_url(url: &str, timeout: Duration) -> Result<reqwest::Client> {
    if !is_loopback_url(url) {
        return probe_http_client(timeout);
    }

    let mut guard = SHARED_LOOPBACK_CLIENT
        .lock()
        .map_err(|_| anyhow::anyhow!("Loopback HTTP client cache lock poisoned"))?;
    if let Some((cached_timeout, client)) = guard.as_ref()
        && *cached_timeout == timeout
    {
        return Ok(client.clone());
    }

    let rebuilt = pooled(reqwest::Client::builder().timeout(timeout).no_proxy())
        .build()
        .context("Failed to build loopback HTTP client")?;
    *guard = Some((timeout, rebuilt.clone()));
    Ok(rebuilt)
}

/// Drop the cached shared HTTP client so the next request rebuilds it and
/// re-reads proxy settings.
pub fn invalidate_probe_http_client() {
    if let Ok(mut guard) = SHARED_PROBE_CLIENT.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = SHARED_LOOPBACK_CLIENT.lock() {
        *guard = None;
    }
}

/// Format an error together with its non-duplicate `source()` chain so
/// transport failures keep the underlying DNS, proxy, TLS, or timeout cause.
pub fn format_error_chain(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut current = error.source();
    while let Some(source) = current {
        let part = source.to_string();
        let trimmed = part.trim();
        if !trimmed.is_empty() && !out.contains(trimmed) {
            if !out.is_empty() {
                out.push_str(": ");
            }
            out.push_str(trimmed);
        }
        current = source.source();
    }
    out
}

/// Proxy-aware client that does not follow redirects.
///
/// Used when the request URL itself is sensitive (a public-repo audit query).
/// A redirect would send that URL to another host before the caller can refuse it.
pub fn probe_http_client_no_redirect(timeout: Duration) -> Result<reqwest::Client> {
    let fingerprint = current_proxy_fingerprint();
    apply_proxy(
        pooled(
            reqwest::Client::builder()
                .timeout(timeout)
                .redirect(reqwest::redirect::Policy::none()),
        ),
        &fingerprint,
    )?
    .build()
    .context("Failed to build HTTP client")
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

    #[test]
    fn format_error_chain_appends_nested_sources_and_skips_duplicates() {
        #[derive(Debug)]
        struct TestErr {
            msg: &'static str,
            cause: Option<Box<dyn std::error::Error + 'static>>,
        }

        impl std::fmt::Display for TestErr {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.msg)
            }
        }

        impl std::error::Error for TestErr {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                self.cause.as_deref()
            }
        }

        let leaf = TestErr {
            msg: "connection refused",
            cause: None,
        };
        let dup = TestErr {
            msg: "request failed: connection refused",
            cause: Some(Box::new(leaf)),
        };
        let root = TestErr {
            msg: "error sending request",
            cause: Some(Box::new(dup)),
        };

        assert_eq!(
            format_error_chain(&root),
            "error sending request: request failed: connection refused"
        );

        let empty_mid = TestErr {
            msg: "",
            cause: Some(Box::new(TestErr {
                msg: "timed out",
                cause: None,
            })),
        };
        let root_with_empty = TestErr {
            msg: "request error",
            cause: Some(Box::new(empty_mid)),
        };
        assert_eq!(format_error_chain(&root_with_empty), "request error: timed out");
    }

    #[test]
    fn invalidate_probe_http_client_clears_cached_client() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();

        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let timeout = Duration::from_secs(12);
        proxy::save_config(&proxied(Some("localhost"))).unwrap();
        probe_http_client(timeout).unwrap();
        assert!(
            SHARED_PROBE_CLIENT
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some(),
            "client should be cached after probe_http_client"
        );

        invalidate_probe_http_client();
        assert!(
            SHARED_PROBE_CLIENT
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_none(),
            "invalidate_probe_http_client must clear cached client"
        );

        probe_http_client(timeout).unwrap();
        assert!(
            SHARED_PROBE_CLIENT
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .is_some(),
            "client should rebuild cleanly after invalidation"
        );

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn is_loopback_url_recognizes_loopback_hosts_and_rejects_remote_hosts() {
        for url in [
            "http://localhost",
            "http://localhost:8080/v1/models",
            "https://LOCALHOST./status",
            "http://api.localhost:3000/test",
            "http://127.0.0.1:9090/oauth",
            "http://127.0.0.42/health",
            "http://[::1]:8080/callback",
            "http://[::1]/callback",
            "http://[0:0:0:0:0:0:0:1]:8080",
            "http://[::ffff:127.0.0.1]:8080/",
            "http://user:pass@127.0.0.1:8080/",
            "localhost",
            "127.0.0.1",
            "::1",
            "[::1]",
        ] {
            assert!(is_loopback_url(url), "{url} should be treated as loopback");
        }

        for url in [
            "",
            "   ",
            "https://example.com/localhost",
            "https://localhost.example.com",
            "https://127.0.0.1.example.com",
            "http://127.0.0.1@evil.com/path",
            "https://192.168.1.10:8080",
            "http://[::1",
            "http://[::1]evil",
        ] {
            assert!(
                !is_loopback_url(url),
                "{url} must NOT be treated as loopback"
            );
        }
    }

    #[tokio::test]
    async fn probe_http_client_for_url_bypasses_env_proxy_on_loopback() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Grab an unused local port for a dead proxy target.
        let dead_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead_port = dead_listener.local_addr().unwrap().port();
        drop(dead_listener);
        let dead_proxy = format!("http://127.0.0.1:{dead_port}");

        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_addr = server.local_addr().unwrap();
        let accept_task = tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut buf = [0u8; 512];
            let _ = socket.read(&mut buf).await;
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await;
        });

        let target = format!("http://{server_addr}/ping");
        // Build the loopback client synchronously while the env-var lock is held,
        // then drop the lock before any `.await` so clippy::await_holding_lock stays clean.
        let client = {
            let _guard = crate::config::test_env_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prev_http = std::env::var_os("HTTP_PROXY");
            let prev_http_lower = std::env::var_os("http_proxy");
            unsafe {
                std::env::set_var("HTTP_PROXY", &dead_proxy);
                std::env::set_var("http_proxy", &dead_proxy);
            }
            invalidate_probe_http_client();
            let built = probe_http_client_for_url(&target, Duration::from_secs(3));
            unsafe {
                match prev_http {
                    Some(v) => std::env::set_var("HTTP_PROXY", v),
                    None => std::env::remove_var("HTTP_PROXY"),
                }
                match prev_http_lower {
                    Some(v) => std::env::set_var("http_proxy", v),
                    None => std::env::remove_var("http_proxy"),
                }
            }
            built.unwrap()
        };

        let response = client.get(&target).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "ok");
        accept_task.await.unwrap();
    }
}

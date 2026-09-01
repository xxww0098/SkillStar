//! One-shot network diagnosis for Settings.
//!
//! Probes the SkillStar proxy, direct GitHub, each configured accelerator,
//! skills.sh, and the official MCP registry. Recommendations are machine
//! keys; the UI maps them through i18n. HTTP stays in this module so the
//! Tauri command remains a thin adapter (command-boundary ratchet).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use super::{github_mirror, github_rewrite, proxy};
use crate::infra::http_client::probe_http_client;

const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkHostCheck {
    pub id: String,
    pub label: String,
    pub url: String,
    /// `ok` | `fail` | `skip`
    pub status: String,
    pub latency_ms: Option<u64>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkDiagnosis {
    pub proxy_enabled: bool,
    pub proxy_type: Option<String>,
    pub checks: Vec<NetworkHostCheck>,
    pub recommendations: Vec<String>,
}

pub async fn diagnose_network() -> Result<NetworkDiagnosis> {
    let proxy_config = proxy::load_config().unwrap_or_default();
    let mut checks = Vec::new();

    checks.push(check_proxy_listener(&proxy_config));
    checks.push(probe_http("github", "GitHub", "https://github.com/").await);
    checks.push(probe_http("api_github", "GitHub API", "https://api.github.com/").await);
    checks.push(probe_http("skills_sh", "skills.sh", "https://skills.sh/").await);
    checks.push(
        probe_http(
            "mcp_registry",
            "MCP Registry",
            "https://registry.modelcontextprotocol.io/v0.1/servers?limit=1",
        )
        .await,
    );

    let mut mirror_checks = probe_mirrors().await;
    checks.append(&mut mirror_checks);

    if github_mirror::load_config()
        .map(|c| c.enabled)
        .unwrap_or(false)
        && let Some(wrap) = github_mirror::effective_mirror_url()
            .and_then(|mirror| github_rewrite::wrap_skills_sh(&mirror))
    {
        checks.push(probe_http("skills_sh_wrap", "skills.sh via GitHub mirror", &wrap).await);
    }

    let mirrors_enabled = github_mirror::load_config()
        .map(|c| c.enabled)
        .unwrap_or(false);
    let recommendations = recommendations(&proxy_config, &checks, mirrors_enabled);
    Ok(NetworkDiagnosis {
        proxy_enabled: proxy_config.enabled && !proxy_config.host.trim().is_empty(),
        proxy_type: proxy_config
            .enabled
            .then(|| proxy_config.proxy_type.as_scheme().to_string()),
        checks,
        recommendations,
    })
}

fn check_proxy_listener(config: &proxy::ProxyConfig) -> NetworkHostCheck {
    if !config.enabled || config.host.trim().is_empty() {
        return NetworkHostCheck {
            id: "proxy".into(),
            label: "Proxy".into(),
            url: String::new(),
            status: "skip".into(),
            latency_ms: None,
            detail: Some("proxy disabled".into()),
        };
    }
    let addr = format!("{}:{}", config.host.trim(), config.port);
    let start = Instant::now();
    let reachable = match addr.parse::<std::net::SocketAddr>() {
        Ok(socket) => TcpStream::connect_timeout(&socket, PROBE_TIMEOUT).is_ok(),
        Err(_) => std::net::ToSocketAddrs::to_socket_addrs(&addr)
            .ok()
            .into_iter()
            .flatten()
            .any(|socket| TcpStream::connect_timeout(&socket, PROBE_TIMEOUT).is_ok()),
    };
    if reachable {
        NetworkHostCheck {
            id: "proxy".into(),
            label: "Proxy".into(),
            url: addr,
            status: "ok".into(),
            latency_ms: Some(start.elapsed().as_millis() as u64),
            detail: Some(format!("{}://", config.proxy_type.as_scheme())),
        }
    } else {
        NetworkHostCheck {
            id: "proxy".into(),
            label: "Proxy".into(),
            url: addr,
            status: "fail".into(),
            latency_ms: None,
            detail: Some("could not connect to proxy listener".into()),
        }
    }
}

async fn probe_http(id: &str, label: &str, url: &str) -> NetworkHostCheck {
    let client = match probe_http_client(PROBE_TIMEOUT) {
        Ok(client) => client,
        Err(err) => {
            return NetworkHostCheck {
                id: id.into(),
                label: label.into(),
                url: url.into(),
                status: "fail".into(),
                latency_ms: None,
                detail: Some(err.to_string()),
            };
        }
    };
    let start = Instant::now();
    match client.get(url).send().await {
        Ok(response) => {
            let status_code = response.status();
            let latency = start.elapsed().as_millis() as u64;
            if status_code.is_success() || status_code.is_redirection() {
                NetworkHostCheck {
                    id: id.into(),
                    label: label.into(),
                    url: url.into(),
                    status: "ok".into(),
                    latency_ms: Some(latency),
                    detail: Some(format!("HTTP {}", status_code.as_u16())),
                }
            } else {
                NetworkHostCheck {
                    id: id.into(),
                    label: label.into(),
                    url: url.into(),
                    status: "fail".into(),
                    latency_ms: Some(latency),
                    detail: Some(format!("HTTP {}", status_code.as_u16())),
                }
            }
        }
        Err(err) => NetworkHostCheck {
            id: id.into(),
            label: label.into(),
            url: url.into(),
            status: "fail".into(),
            latency_ms: None,
            detail: Some(format!("{err:#}")),
        },
    }
}

async fn probe_mirrors() -> Vec<NetworkHostCheck> {
    let config = github_mirror::load_config().unwrap_or_default();
    if !config.enabled {
        return Vec::new();
    }
    let mut set = tokio::task::JoinSet::new();
    for (idx, url) in github_mirror::declaration_order_candidates()
        .into_iter()
        .enumerate()
    {
        set.spawn(async move {
            let start = Instant::now();
            let result = github_mirror::test_mirror(&url).await;
            let check = match result {
                Ok(latency) => NetworkHostCheck {
                    id: format!("mirror_{idx}"),
                    label: format!("GitHub mirror {url}"),
                    url: url.clone(),
                    status: "ok".into(),
                    latency_ms: Some(latency),
                    detail: None,
                },
                Err(err) => NetworkHostCheck {
                    id: format!("mirror_{idx}"),
                    label: format!("GitHub mirror {url}"),
                    url: url.clone(),
                    status: "fail".into(),
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    detail: Some(format!("{err:#}")),
                },
            };
            (idx, check)
        });
    }
    let mut indexed = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Ok(item) = joined {
            indexed.push(item);
        }
    }
    indexed.sort_by_key(|(idx, _)| *idx);
    indexed.into_iter().map(|(_, check)| check).collect()
}

fn check<'a>(checks: &'a [NetworkHostCheck], id: &str) -> Option<&'a NetworkHostCheck> {
    checks.iter().find(|check| check.id == id)
}

fn recommendations(
    proxy_config: &proxy::ProxyConfig,
    checks: &[NetworkHostCheck],
    mirrors_enabled: bool,
) -> Vec<String> {
    let mut recs = Vec::new();
    let github_ok = check(checks, "github").is_some_and(|c| c.status == "ok");
    let api_ok = check(checks, "api_github").is_some_and(|c| c.status == "ok");
    let skills_ok = check(checks, "skills_sh").is_some_and(|c| c.status == "ok");
    let any_mirror_ok = checks
        .iter()
        .any(|c| c.id.starts_with("mirror_") && c.status == "ok");
    let wrap_ok = check(checks, "skills_sh_wrap").is_some_and(|c| c.status == "ok");
    let proxy_fail = check(checks, "proxy").is_some_and(|c| c.status == "fail");

    if proxy_fail {
        recs.push("check_proxy_reachability".into());
    }
    if !github_ok && !api_ok && !any_mirror_ok && !proxy_config.enabled {
        recs.push("enable_proxy".into());
    }
    if (!github_ok || !api_ok) && !mirrors_enabled {
        recs.push("enable_github_mirrors".into());
    }
    if proxy_config.enabled && matches!(proxy_config.proxy_type, proxy::ProxyType::Socks5) {
        recs.push("use_socks5h".into());
    }
    if !skills_ok && (wrap_ok || any_mirror_ok) {
        recs.push("use_marketplace_wrap".into());
    }
    if !github_ok && !api_ok && !any_mirror_ok {
        recs.push("all_github_paths_blocked".into());
    }
    recs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::proxy::{ProxyConfig, ProxyType};

    fn fail(id: &str) -> NetworkHostCheck {
        NetworkHostCheck {
            id: id.into(),
            label: id.into(),
            url: String::new(),
            status: "fail".into(),
            latency_ms: None,
            detail: None,
        }
    }

    fn ok(id: &str) -> NetworkHostCheck {
        NetworkHostCheck {
            id: id.into(),
            label: id.into(),
            url: String::new(),
            status: "ok".into(),
            latency_ms: Some(10),
            detail: None,
        }
    }

    #[test]
    fn recommends_mirrors_when_github_is_blocked() {
        let proxy = ProxyConfig::default();
        let recs = recommendations(
            &proxy,
            &[fail("github"), fail("api_github"), fail("skills_sh")],
            false,
        );
        assert!(recs.contains(&"enable_github_mirrors".to_string()));
        assert!(recs.contains(&"enable_proxy".to_string()));
    }

    #[test]
    fn recommends_socks5h_for_local_dns_socks() {
        let proxy = ProxyConfig {
            enabled: true,
            proxy_type: ProxyType::Socks5,
            host: "127.0.0.1".into(),
            port: 1080,
            username: None,
            password: None,
            bypass: None,
        };
        let recs = recommendations(&proxy, &[ok("github")], true);
        assert!(recs.contains(&"use_socks5h".to_string()));
    }
}

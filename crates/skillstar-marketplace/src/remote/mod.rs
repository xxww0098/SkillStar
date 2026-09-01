//! Remote fetchers for the skills.sh marketplace (search, leaderboard,
//! publishers, publisher repos, skill details, AI keyword search).
//!
//! Split from the original single `remote.rs` into cohesive submodules; the
//! shared HTTP client + build constants live here and are reached by the
//! submodules via `use super::*`. Public items are re-exported so external
//! callers keep using `remote::NAME`.

use std::time::Duration;

use anyhow::{Context, Result};
use tracing::warn;

use skillstar_core::config::{github_mirror, github_rewrite, marketplace_mirror};

const MARKETPLACE_TIMEOUT: Duration = Duration::from_secs(30);
/// Primary marketplace host.
const PRIMARY_HOST: &str = "https://skills.sh";

/// Candidate marketplace hosts, most preferred first.
///
/// Anti-censorship design: the marketplace must not depend on a single host —
/// DNS poisoning or a network-level block on `skills.sh` would take the whole
/// store offline. Extra hosts are appended from the optional marketplace
/// mirror config (`config/marketplace_mirror.json`), so users behind blocking
/// can point the store at an accelerator that proxies the same content.
///
/// The primary host is always first; mirrors that duplicate it are dropped.
///
/// Every returned host is normalized (trailing slash), including the primary —
/// a host without the trailing slash used to concatenate straight onto a
/// slash-stripped path and produce `https://skills.shhot`. See
/// `docs/errors.md` ("Marketplace URL 拼接丢斜杠").
pub fn marketplace_hosts() -> Vec<String> {
    let mut hosts =
        vec![normalize_host(PRIMARY_HOST).unwrap_or_else(|| format!("{PRIMARY_HOST}/"))];

    // When GitHub accelerators are enabled, wrap skills.sh through each
    // healthy mirror so a DNS-poisoned or SNI-blocked primary does not take
    // the store offline. User-configured marketplace mirrors still append
    // after the wrap chain.
    if github_mirror::load_config()
        .map(|config| config.enabled)
        .unwrap_or(false)
    {
        for mirror in github_mirror::candidate_mirror_urls() {
            let Some(wrapped) = github_rewrite::wrap_skills_sh(&mirror) else {
                continue;
            };
            if !hosts.iter().any(|existing| existing == &wrapped) {
                hosts.push(wrapped);
            }
        }
    }

    if let Ok(config) = marketplace_mirror::load_config()
        && config.enabled
    {
        for raw in &config.hosts {
            let Some(with_slash) = normalize_host(raw) else {
                // Anti-censorship feature: a silently dropped mirror leaves the
                // user's traffic on the blocked primary host while the config UI
                // still reads "enabled". Make the rejection visible.
                warn!(
                    target: "skills_sh",
                    host = %raw,
                    "marketplace mirror ignored: only absolute https:// hosts are accepted"
                );
                continue;
            };
            if !hosts
                .iter()
                .any(|existing| normalize_host(existing).as_deref() == Some(with_slash.as_str()))
            {
                hosts.push(with_slash);
            }
        }
    }
    hosts
}

/// Join a normalized-or-not host with a leading-or-not path.
///
/// Defensive on both sides so a host that lost its trailing slash can never
/// swallow the path separator (`https://skills.sh` + `/hot` must never become
/// `https://skills.shhot`), and `"/"` still yields the bare host root.
pub(crate) fn join_url(host: &str, path: &str) -> String {
    format!(
        "{}/{}",
        host.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// The exact `(host, url)` pairs a failover fetch will try, in order.
///
/// This — not `join_url` in isolation — is the real request path, so it is the
/// thing tests have to pin. `join_url` is defensive on both ends, so feeding it
/// only normalized hosts proves nothing: a caller that stopped using it and
/// concatenated by hand produces identical URLs for a normalized host and
/// `https://skills.shhot` for a host that lost its trailing slash. Taking the
/// host list as a parameter lets the tests cover both shapes without touching
/// the network. See `docs/errors.md` ("Marketplace URL 拼接丢斜杠").
pub(crate) fn failover_targets_for(hosts: Vec<String>, path: &str) -> Vec<(String, String)> {
    hosts
        .into_iter()
        .map(|host| {
            let url = join_url(&host, path);
            (host, url)
        })
        .collect()
}

/// [`failover_targets_for`] over the live host list (primary + mirrors).
pub(crate) fn failover_targets(path: &str) -> Vec<(String, String)> {
    failover_targets_for(marketplace_hosts(), path)
}

/// Normalize a host to a trailing-slash `https://` URL, or `None` for unusable
/// input (empty, non-https, not a URL). Comparison uses the normalized form so
/// `https://skills.sh` and `https://skills.sh/` deduplicate.
fn normalize_host(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.starts_with("https://") {
        return None;
    }
    Some(if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    })
}

/// Metadata captured from a successful remote fetch, used for content
/// addressing: the store records `payload_sha256` per scope and skips a full
/// rewrite when a later fetch returns identical bytes.
#[derive(Debug, Clone)]
pub struct FetchMeta {
    /// SHA-256 of the raw response body.
    pub payload_sha256: String,
    /// Which host actually served the body (audit + diagnostics).
    pub source_host: String,
    /// Server-provided ETag, when the host sends one (optional).
    pub etag: Option<String>,
    /// True when the payload behind this fetch could only be parsed through a
    /// lossy fallback (e.g. the leaderboard HTML no longer parses and the
    /// capped `/api/search` result stood in for it). The snapshot writer must
    /// not overwrite a complete local scope with a degraded payload.
    pub degraded: bool,
}

/// Fetch `path` (e.g. `/hot`) from each candidate host in preference order,
/// returning the first success together with content-addressing metadata.
///
/// Requests carry an `If-None-Match` header when `etag` is given **and** the
/// current host matches `etag_host`. Sending a skills.sh ETag to a GitHub
/// accelerator (or vice versa) produces a false 304 and pins a stale
/// snapshot — see `docs/errors.md`.
pub(crate) async fn fetch_with_failover(
    path: &str,
    etag: Option<&str>,
    etag_host: Option<&str>,
) -> Result<(String, FetchMeta)> {
    let client = marketplace_client()?;
    let mut last_error: Option<anyhow::Error> = None;

    for (host, url) in failover_targets(path) {
        let mut request = client
            .get(&url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            )
            .header("Accept", "text/html,application/xhtml+xml,application/json");
        if should_send_etag(etag, etag_host, &host) {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag.unwrap_or_default());
        }

        match request.send().await {
            Ok(response) => {
                if response.status() == reqwest::StatusCode::NOT_MODIFIED {
                    return Ok((
                        String::new(),
                        FetchMeta {
                            payload_sha256: String::new(),
                            source_host: host,
                            etag: etag.map(str::to_string),
                            degraded: false,
                        },
                    ));
                }
                if !response.status().is_success() {
                    last_error = Some(anyhow::anyhow!(
                        "{} returned HTTP {}",
                        host,
                        response.status().as_u16()
                    ));
                    continue;
                }
                let server_etag = response
                    .headers()
                    .get(reqwest::header::ETAG)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string);
                match response.text().await {
                    Ok(body) => {
                        let digest = sha256_hex(&body);
                        return Ok((
                            body,
                            FetchMeta {
                                payload_sha256: digest,
                                source_host: host,
                                etag: server_etag,
                                degraded: false,
                            },
                        ));
                    }
                    Err(err) => {
                        let err = anyhow::Error::new(err);
                        last_error = Some(anyhow::anyhow!("{host} body read failed: {err:#}"));
                    }
                }
            }
            Err(err) => {
                // `{err}` alone stops at "error sending request for url (...)",
                // which cannot tell a dead proxy from a poisoned DNS answer or a
                // TLS failure. Wrapping in `anyhow` and printing with `{:#}`
                // flattens reqwest's whole source chain into the one line the
                // diagnostics panel shows.
                let err = anyhow::Error::new(err);
                last_error = Some(anyhow::anyhow!("{host} request failed: {err:#}"));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All marketplace hosts failed for {path}")))
}

/// Send `If-None-Match` only when the stored ETag belongs to this host.
pub(crate) fn should_send_etag(
    etag: Option<&str>,
    etag_host: Option<&str>,
    current_host: &str,
) -> bool {
    let Some(etag) = etag.filter(|value| !value.is_empty()) else {
        return false;
    };
    let _ = etag;
    let Some(etag_host) = etag_host else {
        return false;
    };
    normalize_host(etag_host).as_deref() == normalize_host(current_host).as_deref()
        || etag_host.trim_end_matches('/') == current_host.trim_end_matches('/')
}

/// SHA-256 hex digest of a byte string.
pub(crate) fn sha256_hex(body: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Shared HTTP client for marketplace requests, rebuilt when the app proxy changes.
fn marketplace_client() -> Result<reqwest::Client> {
    skillstar_core::infra::http_client::probe_http_client(MARKETPLACE_TIMEOUT)
        .context("Failed to build marketplace HTTP client")
}

mod leaderboard;
mod publisher_repos;
mod publishers;
mod search;
mod skill_details;

pub use leaderboard::*;
pub use publisher_repos::*;
pub use publishers::*;
pub use search::*;
pub use skill_details::*;

#[cfg(test)]
mod tests;

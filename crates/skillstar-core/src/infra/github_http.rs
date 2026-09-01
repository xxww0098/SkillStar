//! Anonymous GitHub-family HTTP through the health-ranked mirror chain.
//!
//! Authenticated requests (anything with `Authorization`) always go direct:
//! public accelerators must never see a GitHub App token (D-014 / D-050).

use anyhow::{Context, Result};
use std::time::{Duration, Instant};

use crate::config::{github_health, github_mirror, github_rewrite};
use crate::infra::http_client::probe_http_client;

/// GET `url` without credentials. GitHub-family hosts are tried through each
/// healthy accelerator first, then the original URL. Non-GitHub URLs go
/// direct. Failures of a wrap are recorded on the circuit breaker.
pub async fn get_anonymous(url: &str, timeout: Duration) -> Result<reqwest::Response> {
    get_anonymous_with_headers(url, timeout, &[]).await
}

pub async fn get_anonymous_with_headers(
    url: &str,
    timeout: Duration,
    headers: &[(&str, &str)],
) -> Result<reqwest::Response> {
    if headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
    {
        anyhow::bail!(
            "refusing to send an Authorization header through get_anonymous; use probe_http_client directly"
        );
    }

    let client = probe_http_client(timeout).context("Failed to build HTTP client")?;
    let mut last_error: Option<anyhow::Error> = None;

    for candidate in anonymous_candidates(url) {
        let mut request = client.get(&candidate.url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let start = Instant::now();
        match request.send().await {
            Ok(response)
                if response.status().is_success() || response.status().is_redirection() =>
            {
                if let Some(mirror) = &candidate.mirror {
                    github_health::record_success(mirror, Some(start.elapsed().as_millis() as u64));
                }
                return Ok(response);
            }
            Ok(response) => {
                let status = response.status();
                if let Some(mirror) = &candidate.mirror {
                    github_health::record_failure(mirror);
                }
                last_error = Some(anyhow::anyhow!(
                    "{} returned HTTP {}",
                    candidate.url,
                    status.as_u16()
                ));
            }
            Err(err) => {
                if let Some(mirror) = &candidate.mirror {
                    github_health::record_failure(mirror);
                }
                let err = anyhow::Error::new(err);
                last_error = Some(anyhow::anyhow!("{} request failed: {err:#}", candidate.url));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No candidates to fetch {url}")))
}

struct Candidate {
    url: String,
    /// `Some` when this URL was produced by wrapping through an accelerator.
    mirror: Option<String>,
}

fn anonymous_candidates(url: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    if github_rewrite::is_anonymous_rewritable(url) {
        for mirror in github_mirror::candidate_mirror_urls() {
            if let Some(wrapped) = github_rewrite::wrap_with_mirror(url, &mirror)
                && wrapped != url
            {
                out.push(Candidate {
                    url: wrapped,
                    mirror: Some(mirror),
                });
            }
        }
    }
    out.push(Candidate {
        url: url.to_string(),
        mirror: None,
    });
    out
}

/// Fetch SkillStar's `latest.json` through the anonymous GitHub chain so the
/// updater can still *see* a newer version when the plugin's direct GitHub
/// Releases endpoint is blocked. Install still goes through the signed plugin
/// path or a manual Releases download — this never downloads an unsigned
/// installer from a third-party accelerator.
pub async fn fetch_github_latest_json(url: &str, timeout: Duration) -> Result<serde_json::Value> {
    let response = get_anonymous(url, timeout).await?;
    if !response.status().is_success() {
        anyhow::bail!("latest.json returned HTTP {}", response.status());
    }
    let body = response.text().await?;
    serde_json::from_str(&body).context("latest.json was not valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_github_urls_have_a_single_direct_candidate() {
        let candidates = anonymous_candidates("https://skills.sh/hot");
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].mirror.is_none());
        assert_eq!(candidates[0].url, "https://skills.sh/hot");
    }

    #[test]
    fn github_family_urls_try_wraps_before_direct_when_mirrors_enabled() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        crate::config::github_mirror::save_config(
            &crate::config::github_mirror::GitHubMirrorConfig {
                enabled: true,
                preset_id: Some("ghproxy_vip".into()),
                custom_url: None,
            },
        )
        .unwrap();

        let candidates = anonymous_candidates(
            "https://raw.githubusercontent.com/octocat/Hello-World/master/README",
        );
        assert!(
            candidates.len() > 1,
            "enabled mirrors must produce wrap candidates plus the original"
        );
        assert!(
            candidates
                .first()
                .and_then(|c| c.mirror.as_deref())
                .is_some(),
            "first candidate should be a wrap"
        );
        assert!(
            candidates.last().unwrap().mirror.is_none(),
            "direct GitHub is the final authority"
        );
        assert!(
            candidates.iter().all(|c| !c.url.contains('@')),
            "candidates must stay credential-free"
        );

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}

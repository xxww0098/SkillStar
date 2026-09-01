//! GitHub mirror/accelerator configuration for users without a VPN/proxy.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Command};

use super::{github_health, github_rewrite};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorPreset {
    pub id: String,
    pub name: String,
    pub url: String,
    pub supports_clone: bool,
}

pub fn builtin_presets() -> Vec<MirrorPreset> {
    vec![
        MirrorPreset {
            id: "ghproxy_vip".into(),
            name: "GHProxy.vip".into(),
            url: "https://ghproxy.vip/".into(),
            supports_clone: true,
        },
        MirrorPreset {
            id: "gh_proxy_com".into(),
            name: "GH-Proxy.com".into(),
            url: "https://gh-proxy.com/".into(),
            supports_clone: true,
        },
        MirrorPreset {
            id: "github_akams".into(),
            name: "GitHub Akams".into(),
            url: "https://github.akams.cn/".into(),
            supports_clone: true,
        },
        MirrorPreset {
            id: "gh_llkk".into(),
            name: "GH LLKK".into(),
            url: "https://gh.llkk.cc/".into(),
            supports_clone: true,
        },
        MirrorPreset {
            id: "ghfast_top".into(),
            name: "GHFast.top".into(),
            url: "https://ghfast.top/".into(),
            supports_clone: true,
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubMirrorConfig {
    pub enabled: bool,
    pub preset_id: Option<String>,
    pub custom_url: Option<String>,
}

impl Default for GitHubMirrorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            preset_id: Some("ghproxy_vip".into()),
            custom_url: None,
        }
    }
}

fn config_path() -> PathBuf {
    crate::infra::paths::github_mirror_config_path()
}

pub fn load_config() -> Result<GitHubMirrorConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(GitHubMirrorConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config: GitHubMirrorConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config)
}

pub fn save_config(config: &GitHubMirrorConfig) -> Result<()> {
    let path = config_path();
    let content = serde_json::to_string_pretty(config)?;
    crate::infra::fs_ops::atomic_write(&path, content.as_bytes())?;
    github_health::reset();
    Ok(())
}

/// Preferred mirror URL (first ranked candidate), for compatibility with
/// callers that need a single value (e.g. the settings UI).
pub fn effective_mirror_url() -> Option<String> {
    candidate_mirror_urls().into_iter().next()
}

/// Declaration-order candidates (custom → selected preset → remaining
/// presets), before the circuit breaker reorders them. Used by the network
/// doctor so the UI still lists every configured accelerator.
pub fn declaration_order_candidates() -> Vec<String> {
    let Ok(config) = load_config() else {
        return Vec::new();
    };
    if !config.enabled {
        return Vec::new();
    }

    let mut seen = std::collections::HashSet::new();
    let mut candidates: Vec<String> = Vec::new();

    let mut push = |url: String| {
        if let Some(normalized) = github_rewrite::normalize_mirror_url(&url)
            && seen.insert(normalized.clone())
        {
            candidates.push(normalized);
        }
    };

    if let Some(custom) = config.custom_url.clone() {
        push(custom);
    }
    if let Some(preset_id) = &config.preset_id {
        for preset in builtin_presets() {
            if &preset.id == preset_id {
                push(preset.url.clone());
            }
        }
    }
    for preset in builtin_presets() {
        push(preset.url.clone());
    }

    candidates
}

/// Ordered mirror candidates, most preferred first.
///
/// Anti-censorship design: a single mirror is a single point of failure — if
/// the chosen mirror is unreachable or rate-limited there is no fallback.
/// The candidates are declaration order (custom → selected preset → remaining
/// presets), then ranked by the circuit breaker so recently-dead hosts are
/// skipped and recently-fast hosts lead. See [`github_health`].
///
/// Callers that spawn git subprocesses should try each candidate in order and
/// fall back to a direct GitHub connection only after every candidate fails.
pub fn candidate_mirror_urls() -> Vec<String> {
    github_health::rank_candidates(declaration_order_candidates())
}

/// Apply GitHub-family `insteadOf` rewrites for a single mirror to a git
/// command. Origins include github.com, raw, codeload, objects, and gist —
/// never `api.github.com`, and never when the operation carries credentials
/// (callers already gate that).
pub fn apply_mirror_args_for(cmd: &mut Command, mirror_url: &str) {
    let Some(normalized) = github_rewrite::normalize_mirror_url(mirror_url) else {
        return;
    };
    for origin in github_rewrite::GIT_INSTEAD_OF_ORIGINS {
        let key = format!("url.{normalized}{origin}.insteadOf={origin}");
        cmd.arg("-c").arg(key);
    }
}

/// Apply the preferred mirror's `insteadOf` rewrite to a git command.
/// Prefer [`apply_mirror_args_for`] when trying candidates in a chain.
pub fn apply_mirror_args(cmd: &mut Command) {
    if let Some(mirror) = effective_mirror_url() {
        apply_mirror_args_for(cmd, &mirror);
    }
}

/// True when git stderr suggests the configured GitHub mirror is unreachable.
pub fn is_mirror_transport_error(stderr: &str) -> bool {
    let s = stderr.to_lowercase();
    s.contains("ssl_connect")
        || s.contains("ssl_error_syscall")
        || s.contains("could not resolve host")
        || s.contains("couldn't resolve host")
        || s.contains("failed to connect")
        || s.contains("connection refused")
        || s.contains("connection timed out")
        || s.contains("unable to access 'https://gh-")
        || s.contains("ghproxy")
        || s.contains("gh-proxy")
}

/// Probe a mirror by fetching a tiny public raw file through it. A 200 on the
/// accelerator root does not prove git/raw proxying works.
pub async fn test_mirror(url: &str) -> Result<u64> {
    let client = crate::infra::http_client::probe_http_client(std::time::Duration::from_secs(10))?;
    let probe = github_rewrite::mirror_probe_url(url)
        .ok_or_else(|| anyhow::anyhow!("Mirror URL is not a usable http(s) endpoint"))?;

    let start = std::time::Instant::now();
    let resp = client.get(&probe).send().await?;
    let latency = start.elapsed().as_millis() as u64;
    let status = resp.status();
    if !status.is_success() {
        github_health::record_failure(url);
        anyhow::bail!("Mirror probe returned HTTP {}", status);
    }
    let body = resp.text().await.unwrap_or_default();
    if !body.to_ascii_lowercase().contains("hello") {
        github_health::record_failure(url);
        anyhow::bail!("Mirror probe returned an unexpected body");
    }
    github_health::record_success(url, Some(latency));
    Ok(latency)
}

#[cfg(test)]
mod tests {
    use super::{
        GitHubMirrorConfig, builtin_presets, candidate_mirror_urls, declaration_order_candidates,
        effective_mirror_url, load_config, save_config,
    };
    use tempfile::TempDir;

    #[test]
    fn builtin_presets_are_valid() {
        let presets = builtin_presets();
        assert!(presets.len() >= 4);
        for preset in &presets {
            assert!(!preset.id.is_empty());
            assert!(!preset.name.is_empty());
            assert!(preset.url.starts_with("https://"));
            assert!(preset.url.ends_with('/'));
        }
    }

    #[test]
    fn load_config_returns_default_when_missing() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();

        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let config = load_config().unwrap();
        assert!(!config.enabled);
        assert_eq!(config.preset_id.as_deref(), Some("ghproxy_vip"));
        assert_eq!(effective_mirror_url(), None);

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn save_and_load_config_roundtrip() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();

        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let original = GitHubMirrorConfig {
            enabled: true,
            preset_id: None,
            custom_url: Some("https://mirror.example".into()),
        };

        save_config(&original).unwrap();
        let loaded = load_config().unwrap();

        assert!(loaded.enabled);
        assert_eq!(loaded.preset_id, None);
        assert_eq!(loaded.custom_url.as_deref(), Some("https://mirror.example"));
        assert_eq!(
            effective_mirror_url().as_deref(),
            Some("https://mirror.example/")
        );
        assert_eq!(declaration_order_candidates()[0], "https://mirror.example/");
        assert!(candidate_mirror_urls().contains(&"https://mirror.example/".to_string()));

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn save_config_resets_health_so_a_dead_custom_url_is_retried() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        save_config(&GitHubMirrorConfig {
            enabled: true,
            preset_id: Some("ghproxy_vip".into()),
            custom_url: None,
        })
        .unwrap();
        let first = declaration_order_candidates()[0].clone();
        crate::config::github_health::record_failure(&first);
        crate::config::github_health::record_failure(&first);
        assert!(
            !candidate_mirror_urls().contains(&first)
                || candidate_mirror_urls()[0] != first
                || candidate_mirror_urls().len() == 1,
            "open circuit should skip or fail-open; after two failures with other presets, skip"
        );

        save_config(&GitHubMirrorConfig {
            enabled: true,
            preset_id: Some("ghproxy_vip".into()),
            custom_url: None,
        })
        .unwrap();
        assert_eq!(candidate_mirror_urls()[0], first);

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn instead_of_rewrites_github_family_not_just_github_com() {
        let mut cmd = std::process::Command::new("git");
        super::apply_mirror_args_for(&mut cmd, "https://ghproxy.vip/");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let joined = args.join(" ");
        assert!(
            joined.contains(
                "url.https://ghproxy.vip/https://github.com/.insteadOf=https://github.com/"
            )
        );
        assert!(joined.contains(
            "url.https://ghproxy.vip/https://raw.githubusercontent.com/.insteadOf=https://raw.githubusercontent.com/"
        ));
        assert!(joined.contains(
            "url.https://ghproxy.vip/https://codeload.github.com/.insteadOf=https://codeload.github.com/"
        ));
        assert!(
            !joined.contains("api.github.com"),
            "authenticated API traffic must never be rewritten"
        );
    }

    #[test]
    fn mirror_transport_error_detects_ssl_failures() {
        let stderr = "fatal: unable to access 'https://gh-proxy.com/https://github.com/foo.git/': LibreSSL SSL_connect: SSL_ERROR_SYSCALL";
        assert!(super::is_mirror_transport_error(stderr));
    }

    #[test]
    fn builtin_preset_lookup_contains_default_id() {
        let preset = builtin_presets()
            .into_iter()
            .find(|preset| preset.id == "ghproxy_vip")
            .unwrap();

        assert_eq!(preset.url, "https://ghproxy.vip/");
        assert!(preset.supports_clone);
    }
}

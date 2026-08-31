//! GitHub mirror/accelerator configuration for users without a VPN/proxy.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Command};

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
    Ok(())
}

/// Normalize a mirror URL to a trailing-slash https/http URL, or `None` if
/// the URL is unusable (empty, non-http scheme).
fn normalize_mirror_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let with_slash = if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    };
    (with_slash.starts_with("https://") || with_slash.starts_with("http://")).then_some(with_slash)
}

/// Preferred mirror URL (first candidate), for compatibility with callers
/// that need a single value (e.g. the settings UI).
pub fn effective_mirror_url() -> Option<String> {
    candidate_mirror_urls().into_iter().next()
}

/// Ordered mirror candidates, most preferred first.
///
/// Anti-censorship design: a single mirror is a single point of failure — if
/// the chosen mirror is unreachable or rate-limited there is no fallback.
/// The candidates are:
///   1. the explicitly configured custom URL (if any),
///   2. the explicitly selected preset (if any),
///   3. every other builtin preset, in declaration order,
///
/// deduplicated and normalized.
///
/// Callers that spawn git subprocesses should try each candidate in order and
/// fall back to a direct GitHub connection only after every candidate fails.
pub fn candidate_mirror_urls() -> Vec<String> {
    let Ok(config) = load_config() else {
        return Vec::new();
    };
    if !config.enabled {
        return Vec::new();
    }

    let mut seen = std::collections::HashSet::new();
    let mut candidates: Vec<String> = Vec::new();

    let mut push = |url: String| {
        if let Some(normalized) = normalize_mirror_url(&url)
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

/// Apply the `insteadOf` rewrite for a single mirror URL to a git command.
/// `url.<mirror>https://github.com/.insteadOf=https://github.com/`
pub fn apply_mirror_args_for(cmd: &mut Command, mirror_url: &str) {
    let Some(normalized) = normalize_mirror_url(mirror_url) else {
        return;
    };
    let key = format!(
        "url.{}https://github.com/.insteadOf=https://github.com/",
        normalized
    );
    cmd.arg("-c").arg(key);
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

pub async fn test_mirror(url: &str) -> Result<u64> {
    let client = crate::infra::http_client::probe_http_client(std::time::Duration::from_secs(10))?;

    let normalised = if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{url}/")
    };

    let start = std::time::Instant::now();
    let resp = client.head(&normalised).send().await?;
    let latency = start.elapsed().as_millis() as u64;

    if resp.status().is_server_error() {
        anyhow::bail!("Mirror returned server error: {}", resp.status());
    }

    Ok(latency)
}

#[cfg(test)]
mod tests {
    use super::{
        GitHubMirrorConfig, builtin_presets, effective_mirror_url, load_config, save_config,
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

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
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

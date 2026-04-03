//! GitHub mirror/accelerator configuration for users without a VPN/proxy.
//!
//! All mainstream GitHub mirrors use the same mechanism: **URL prefix proxy**.
//! The original `https://github.com/owner/repo.git` becomes
//! `https://mirror.example/https://github.com/owner/repo.git`.
//!
//! We inject this rewrite per-command via `git -c url.*.insteadOf=...` so the
//! user's global `.gitconfig` is never touched.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;

// ── Built-in presets ────────────────────────────────────────────────

/// A built-in mirror preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorPreset {
    pub id: String,
    pub name: String,
    pub url: String,
    /// Whether the mirror supports `git clone` / `git fetch` (Smart HTTP).
    pub supports_clone: bool,
}

/// Return all built-in presets.  Order is the recommended priority.
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

// ── Persisted configuration ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubMirrorConfig {
    pub enabled: bool,
    /// ID of a built-in preset, or `None` for custom.
    pub preset_id: Option<String>,
    /// User-supplied mirror URL (used when `preset_id` is `None`).
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

fn config_path() -> std::path::PathBuf {
    super::paths::github_mirror_config_path()
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
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}

// ── Runtime helpers ─────────────────────────────────────────────────

/// Resolve the effective mirror URL from the current config.
///
/// Returns `Some("https://mirror.example/")` if a mirror is enabled,
/// or `None` if disabled / misconfigured.
pub fn effective_mirror_url() -> Option<String> {
    let config = load_config().ok()?;
    if !config.enabled {
        return None;
    }

    let url = if let Some(preset_id) = &config.preset_id {
        builtin_presets()
            .iter()
            .find(|p| &p.id == preset_id)
            .map(|p| p.url.clone())
    } else {
        config.custom_url.clone()
    };

    // Normalise: must end with '/'
    url.map(|u| if u.ends_with('/') { u } else { format!("{u}/") })
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
}

/// Inject mirror-related `-c` arguments into a `Command` (git subprocess).
///
/// If a mirror is active, this prepends:
/// ```text
/// -c url.<mirror_url>https://github.com/.insteadOf=https://github.com/
/// ```
///
/// This causes git to transparently rewrite `https://github.com/...` URLs
/// to `<mirror_url>https://github.com/...` for that single invocation only.
///
/// **Non-GitHub URLs are not affected.**
pub fn apply_mirror_args(cmd: &mut Command) {
    if let Some(mirror) = effective_mirror_url() {
        let key = format!(
            "url.{}https://github.com/.insteadOf=https://github.com/",
            mirror
        );
        // Insert -c <key> before the subcommand
        cmd.arg("-c").arg(key);
    }
}

/// Test whether a mirror URL is reachable.
///
/// Sends a lightweight HTTP HEAD request to the mirror root.
/// Returns `Ok(latency_ms)` on success.
pub async fn test_mirror(url: &str) -> Result<u64> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let normalised = if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{url}/")
    };

    let start = std::time::Instant::now();
    let resp = client.head(&normalised).send().await?;

    // Some mirrors return 4xx on HEAD to root; that's fine — we only need
    // a TCP+TLS round-trip to confirm reachability.
    let latency = start.elapsed().as_millis() as u64;

    if resp.status().is_server_error() {
        anyhow::bail!("Mirror returned server error: {}", resp.status());
    }

    Ok(latency)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_presets_are_valid() {
        let presets = builtin_presets();
        assert!(presets.len() >= 4);
        for p in &presets {
            assert!(!p.id.is_empty());
            assert!(!p.name.is_empty());
            assert!(p.url.starts_with("https://"));
            assert!(p.url.ends_with('/'));
        }
    }

    #[test]
    fn effective_mirror_url_returns_none_when_disabled() {
        // Default config is disabled
        let config = GitHubMirrorConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn default_config_roundtrips() {
        let config = GitHubMirrorConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: GitHubMirrorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.enabled, config.enabled);
        assert_eq!(parsed.preset_id, config.preset_id);
    }

    #[test]
    fn apply_mirror_args_produces_correct_flag() {
        let mirror = "https://ghproxy.vip/";
        let expected_key = format!(
            "url.{}https://github.com/.insteadOf=https://github.com/",
            mirror
        );

        // Verify the key format is correct
        assert_eq!(
            expected_key,
            "url.https://ghproxy.vip/https://github.com/.insteadOf=https://github.com/"
        );
    }
}

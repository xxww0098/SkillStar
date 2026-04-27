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
    skillstar_infra::paths::github_mirror_config_path()
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

pub fn effective_mirror_url() -> Option<String> {
    let config = load_config().ok()?;
    if !config.enabled {
        return None;
    }

    let url = if let Some(preset_id) = &config.preset_id {
        builtin_presets()
            .iter()
            .find(|preset| &preset.id == preset_id)
            .map(|preset| preset.url.clone())
    } else {
        config.custom_url.clone()
    };

    url.map(|u| if u.ends_with('/') { u } else { format!("{u}/") })
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
}

pub fn apply_mirror_args(cmd: &mut Command) {
    if let Some(mirror) = effective_mirror_url() {
        let key = format!(
            "url.{}https://github.com/.insteadOf=https://github.com/",
            mirror
        );
        cmd.arg("-c").arg(key);
    }
}

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
        let _guard = crate::test_env_lock()
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
        let _guard = crate::test_env_lock()
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
    fn builtin_preset_lookup_contains_default_id() {
        let preset = builtin_presets()
            .into_iter()
            .find(|preset| preset.id == "ghproxy_vip")
            .unwrap();

        assert_eq!(preset.url, "https://ghproxy.vip/");
        assert!(preset.supports_clone);
    }
}

//! Marketplace mirror/accelerator configuration.
//!
//! The marketplace is a single point of failure for anti-censorship purposes:
//! if `skills.sh` is blocked or poisoned, the whole store is unreachable.
//! This config lets users append mirror hosts that serve the same content;
//! the marketplace fetch chain tries the primary host first, then each
//! configured mirror in order. See `skillstar-marketplace::remote::marketplace_hosts`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketplaceMirrorConfig {
    pub enabled: bool,
    /// Mirror hosts, in preference order. Each must be `https://…`.
    pub hosts: Vec<String>,
}

fn config_path() -> PathBuf {
    crate::infra::paths::marketplace_mirror_config_path()
}

pub fn load_config() -> Result<MarketplaceMirrorConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(MarketplaceMirrorConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config: MarketplaceMirrorConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config)
}

pub fn save_config(config: &MarketplaceMirrorConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MarketplaceMirrorConfig, load_config, save_config};
    use tempfile::TempDir;

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
        assert!(config.hosts.is_empty());

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

        let original = MarketplaceMirrorConfig {
            enabled: true,
            hosts: vec![
                "https://mirror.example/".into(),
                "https://mirror2.example".into(),
            ],
        };

        save_config(&original).unwrap();
        let loaded = load_config().unwrap();

        assert!(loaded.enabled);
        assert_eq!(loaded.hosts.len(), 2);
        assert_eq!(loaded.hosts[0], "https://mirror.example/");

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}

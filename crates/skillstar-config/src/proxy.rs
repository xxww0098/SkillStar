use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    Http,
    Https,
    Socks5,
}

impl ProxyType {
    fn parse_loose(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "http" => Self::Http,
            "https" => Self::Https,
            "socks5" => Self::Socks5,
            _ => Self::Http,
        }
    }

    pub fn as_scheme(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks5 => "socks5",
        }
    }
}

impl Default for ProxyType {
    fn default() -> Self {
        Self::Http
    }
}

fn deserialize_proxy_type<'de, D>(deserializer: D) -> Result<ProxyType, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(ProxyType::parse_loose(&raw))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProxyConfig {
    pub enabled: bool,
    #[serde(default, deserialize_with = "deserialize_proxy_type")]
    pub proxy_type: ProxyType,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bypass: Option<String>,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            proxy_type: ProxyType::default(),
            host: String::new(),
            port: 7897,
            username: None,
            password: None,
            bypass: None,
        }
    }
}

fn config_path() -> PathBuf {
    skillstar_infra::paths::proxy_config_path()
}

pub fn load_config() -> Result<ProxyConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(ProxyConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config: ProxyConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config)
}

pub fn save_config(config: &ProxyConfig) -> Result<()> {
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
    use super::{ProxyConfig, ProxyType, load_config, save_config};
    use tempfile::TempDir;

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
        assert_eq!(config.proxy_type, ProxyType::Http);
        assert_eq!(config.port, 7897);

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

        let original = ProxyConfig {
            enabled: true,
            proxy_type: ProxyType::Socks5,
            host: "127.0.0.1".into(),
            port: 1080,
            username: Some("alice".into()),
            password: Some("secret".into()),
            bypass: Some("localhost,127.0.0.1".into()),
        };

        save_config(&original).unwrap();
        let loaded = load_config().unwrap();

        assert!(loaded.enabled);
        assert_eq!(loaded.proxy_type, ProxyType::Socks5);
        assert_eq!(loaded.host, "127.0.0.1");
        assert_eq!(loaded.port, 1080);
        assert_eq!(loaded.username.as_deref(), Some("alice"));
        assert_eq!(loaded.password.as_deref(), Some("secret"));
        assert_eq!(loaded.bypass.as_deref(), Some("localhost,127.0.0.1"));

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}

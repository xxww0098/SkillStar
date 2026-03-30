use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub proxy_type: String, // "http", "https", "socks5"
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bypass: Option<String>, // comma-separated list
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            proxy_type: "http".to_string(),
            host: String::new(),
            port: 7897,
            username: None,
            password: None,
            bypass: None,
        }
    }
}

fn config_path() -> std::path::PathBuf {
    super::paths::data_root().join("proxy.json")
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

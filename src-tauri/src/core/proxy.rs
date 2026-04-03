use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};

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
    pub bypass: Option<String>, // comma-separated list
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

fn config_path() -> std::path::PathBuf {
    super::paths::proxy_config_path()
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

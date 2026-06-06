//! HTTP-layer fingerprint (headers, UA, locale).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP-layer identity claims. Applied to every outgoing request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpProfile {
    pub user_agent: String,
    pub accept_language: String,
    #[serde(default = "default_accept_encoding")]
    pub accept_encoding: String,

    /// Client Hint headers (Chrome family). `None` → don't emit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sec_ch_ua: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sec_ch_ua_platform: Option<String>,
    #[serde(default)]
    pub sec_ch_ua_mobile: bool,

    /// Free-form extras, applied after the canonical fields.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_headers: HashMap<String, String>,
}

fn default_accept_encoding() -> String {
    "gzip, deflate, br, zstd".to_string()
}

impl Default for HttpProfile {
    fn default() -> Self {
        Self::reqwest_default()
    }
}

impl HttpProfile {
    /// What reqwest produces out of the box (used by the "original" fingerprint).
    pub fn reqwest_default() -> Self {
        Self {
            user_agent: format!(
                "skillstar/{} reqwest/0.13",
                env!("CARGO_PKG_VERSION", "0.1.0")
            ),
            accept_language: "en-US,en;q=0.9".to_string(),
            accept_encoding: default_accept_encoding(),
            sec_ch_ua: None,
            sec_ch_ua_platform: None,
            sec_ch_ua_mobile: false,
            extra_headers: HashMap::new(),
        }
    }

    /// Chrome 147 on macOS preset (matches `TlsProfile::Chrome { major: 147 }`).
    pub fn chrome_macos() -> Self {
        Self {
            user_agent: concat!(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) ",
                "AppleWebKit/537.36 (KHTML, like Gecko) ",
                "Chrome/147.0.0.0 Safari/537.36"
            )
            .to_string(),
            accept_language: "en-US,en;q=0.9".to_string(),
            accept_encoding: default_accept_encoding(),
            sec_ch_ua: Some(
                "\"Chromium\";v=\"147\", \"Not?A_Brand\";v=\"24\", \"Google Chrome\";v=\"147\""
                    .to_string(),
            ),
            sec_ch_ua_platform: Some("\"macOS\"".to_string()),
            sec_ch_ua_mobile: false,
            extra_headers: HashMap::new(),
        }
    }

    /// Chrome 147 on Windows preset.
    pub fn chrome_windows() -> Self {
        Self {
            user_agent: concat!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
                "AppleWebKit/537.36 (KHTML, like Gecko) ",
                "Chrome/147.0.0.0 Safari/537.36"
            )
            .to_string(),
            sec_ch_ua_platform: Some("\"Windows\"".to_string()),
            ..Self::chrome_macos()
        }
    }

    /// Safari 26 on macOS preset.
    pub fn safari_macos() -> Self {
        Self {
            user_agent: concat!(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) ",
                "AppleWebKit/605.1.15 (KHTML, like Gecko) ",
                "Version/18.0 Safari/605.1.15"
            )
            .to_string(),
            accept_language: "en-US,en;q=0.9".to_string(),
            accept_encoding: "gzip, deflate, br".to_string(),
            sec_ch_ua: None,
            sec_ch_ua_platform: None,
            sec_ch_ua_mobile: false,
            extra_headers: HashMap::new(),
        }
    }

    /// Set locale to zh-CN (China) — must be paired with matching TZ/IP.
    pub fn with_locale_zh_cn(mut self) -> Self {
        self.accept_language = "zh-CN,zh;q=0.9,en;q=0.8".to_string();
        self
    }
}

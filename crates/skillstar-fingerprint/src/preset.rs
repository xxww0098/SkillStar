//! Built-in fingerprint presets.
//!
//! A preset is a recipe a user can pick from the UI to instantly generate
//! a fresh fingerprint with sensible defaults (locale, TLS profile, UA
//! string, client hints). Each preset is identified by a stable string id
//! so the frontend can list them and call `create_from_preset(id, name)`.
//!
//! Presets are intentionally minimal — they shadow real-world browser
//! versions that `wreq-util` ships emulations for. If you want a more
//! exotic mix, build a fingerprint by hand via [`DeviceFingerprint::new`].

use crate::types::FingerprintSource;
use crate::{DeviceFingerprint, HttpProfile, TlsProfile};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identifier for a built-in preset. Stable across versions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PresetId {
    /// reqwest's stock rustls — same as the "original" fingerprint in the
    /// store, but freshly named so the user can keep multiple "plain" rows.
    Default,
    /// Chrome 147 on macOS, en-US.
    ChromeMac,
    /// Chrome 147 on Windows, en-US.
    ChromeWindows,
    /// Safari 26 on macOS, en-US.
    SafariMac,
    /// Firefox 136 on macOS, en-US.
    FirefoxMac,
    /// Edge 147 on macOS, en-US.
    EdgeMac,
    /// Chrome 147 on macOS with zh-CN locale (paired with `Asia/Shanghai`
    /// once the network layer ships).
    ChromeMacZhCn,
}

/// A preset row exposed to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetTemplate {
    pub id: PresetId,
    /// Short human label, in English. The frontend localises if needed.
    pub label: &'static str,
    /// One-line description shown under the label.
    pub description: &'static str,
    /// Short tag rendered as a chip in the picker (`Chrome` / `Safari` / ...).
    pub family: &'static str,
}

impl PresetId {
    /// Stable string used in JSON (matches `#[serde(rename_all = "kebab-case")]`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::ChromeMac => "chrome-mac",
            Self::ChromeWindows => "chrome-windows",
            Self::SafariMac => "safari-mac",
            Self::FirefoxMac => "firefox-mac",
            Self::EdgeMac => "edge-mac",
            Self::ChromeMacZhCn => "chrome-mac-zh-cn",
        }
    }

    /// Parse from a string id (used by Tauri commands).
    pub fn from_id(s: &str) -> Option<Self> {
        Some(match s {
            "default" => Self::Default,
            "chrome-mac" => Self::ChromeMac,
            "chrome-windows" => Self::ChromeWindows,
            "safari-mac" => Self::SafariMac,
            "firefox-mac" => Self::FirefoxMac,
            "edge-mac" => Self::EdgeMac,
            "chrome-mac-zh-cn" => Self::ChromeMacZhCn,
            _ => return None,
        })
    }
}

/// All built-in presets, in display order.
pub fn all_presets() -> Vec<PresetTemplate> {
    vec![
        PresetTemplate {
            id: PresetId::Default,
            label: "reqwest 默认",
            description: "rustls stock ClientHello — 完全等同于改造前 SkillStar 的行为，作为基线 / 安全回退",
            family: "Default",
        },
        PresetTemplate {
            id: PresetId::ChromeMac,
            label: "Chrome 147 · macOS",
            description: "最常见的桌面身份。JA3/JA4 与真实 Chrome 147 一致，Sec-CH-UA 已配齐",
            family: "Chrome",
        },
        PresetTemplate {
            id: PresetId::ChromeWindows,
            label: "Chrome 147 · Windows",
            description: "Windows 10/11 上的 Chrome 147，Sec-CH-UA-Platform=\"Windows\"",
            family: "Chrome",
        },
        PresetTemplate {
            id: PresetId::SafariMac,
            label: "Safari 26 · macOS",
            description: "WebKit 引擎独立的 H2 指纹（与 Chromium 系明显不同）",
            family: "Safari",
        },
        PresetTemplate {
            id: PresetId::FirefoxMac,
            label: "Firefox 136 · macOS",
            description: "Gecko 引擎，TLS 扩展顺序与 Chrome 不同",
            family: "Firefox",
        },
        PresetTemplate {
            id: PresetId::EdgeMac,
            label: "Edge 147 · macOS",
            description: "基于 Chromium，与 Chrome 共享 H2 fingerprint，UA 标 Edg/147",
            family: "Edge",
        },
        PresetTemplate {
            id: PresetId::ChromeMacZhCn,
            label: "Chrome 147 · macOS · 中文",
            description: "Accept-Language=zh-CN,zh;q=0.9 — 适合搭配国内代理出口",
            family: "Chrome",
        },
    ]
}

/// Materialise a fresh [`DeviceFingerprint`] from a preset.
///
/// The new fingerprint gets a fresh UUID id, the given name, and timestamps
/// set to "now". The preset itself is recorded in `source` for later auditing.
pub fn instantiate(preset: PresetId, name: impl Into<String>) -> DeviceFingerprint {
    let now = Utc::now().timestamp();
    let name = name.into();
    let (http, tls): (HttpProfile, TlsProfile) = match preset {
        PresetId::Default => (HttpProfile::reqwest_default(), TlsProfile::Default),
        PresetId::ChromeMac => (HttpProfile::chrome_macos(), TlsProfile::chrome_latest()),
        PresetId::ChromeWindows => (HttpProfile::chrome_windows(), TlsProfile::chrome_latest()),
        PresetId::SafariMac => (HttpProfile::safari_macos(), TlsProfile::safari_latest()),
        PresetId::FirefoxMac => {
            let mut h = HttpProfile::chrome_macos();
            h.user_agent = concat!(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:136.0) ",
                "Gecko/20100101 Firefox/136.0"
            )
            .to_string();
            h.sec_ch_ua = None;
            h.sec_ch_ua_platform = None;
            (h, TlsProfile::firefox_latest())
        }
        PresetId::EdgeMac => {
            let mut h = HttpProfile::chrome_macos();
            h.user_agent = concat!(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) ",
                "AppleWebKit/537.36 (KHTML, like Gecko) ",
                "Chrome/147.0.0.0 Safari/537.36 Edg/147.0.0.0"
            )
            .to_string();
            h.sec_ch_ua = Some(
                "\"Chromium\";v=\"147\", \"Not?A_Brand\";v=\"24\", \"Microsoft Edge\";v=\"147\""
                    .to_string(),
            );
            (h, TlsProfile::edge_latest())
        }
        PresetId::ChromeMacZhCn => (
            HttpProfile::chrome_macos().with_locale_zh_cn(),
            TlsProfile::chrome_latest(),
        ),
    };

    DeviceFingerprint {
        id: Uuid::new_v4().to_string(),
        name,
        source: FingerprintSource::GeneratedFromPersona {
            template_id: preset.as_str().to_string(),
        },
        created_at: now,
        updated_at: now,
        http,
        tls,
        http2: Default::default(),
        network: Default::default(),
        // Generate a fresh device identity unless this is the explicit
        // "default" preset (which should leave system telemetry untouched).
        telemetry: if matches!(preset, PresetId::Default) {
            crate::IdeTelemetry::default()
        } else {
            crate::IdeTelemetry::generate()
        },
    }
}

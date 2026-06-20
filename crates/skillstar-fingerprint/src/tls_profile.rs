//! TLS / HTTP-2 protocol fingerprint configuration.

use serde::{Deserialize, Serialize};

/// TLS ClientHello fingerprint profile.
///
/// `Default` keeps reqwest's stock rustls behaviour (what SkillStar uses today).
/// The other variants delegate to `wreq` browser emulation profiles, which
/// reproduce real-world JA3/JA4 hashes for popular browsers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[derive(Default)]
pub enum TlsProfile {
    /// reqwest + rustls defaults — what SkillStar shipped before fingerprinting.
    /// Useful for "original" identity and as a safe fallback.
    #[default]
    Default,

    /// Latest Google Chrome on desktop.
    Chrome {
        /// Major version, e.g. `131`, `147`. Maps to `wreq_util::Emulation::Chrome*`.
        major: u16,
    },

    /// Apple Safari on macOS/iOS.
    Safari { major: u16 },

    /// Microsoft Edge (Chromium).
    Edge { major: u16 },

    /// Mozilla Firefox.
    Firefox { major: u16 },

    /// Opera browser.
    Opera { major: u16 },

    /// Android OkHttp (mobile API client).
    OkHttp { major: u16 },
}

impl TlsProfile {
    /// Latest known-good Chrome profile. Updated as `wreq-util` ships new versions.
    pub fn chrome_latest() -> Self {
        Self::Chrome { major: 147 }
    }

    /// Latest known-good Safari profile.
    pub fn safari_latest() -> Self {
        Self::Safari { major: 26 }
    }

    /// Latest known-good Firefox profile.
    pub fn firefox_latest() -> Self {
        Self::Firefox { major: 133 }
    }

    /// Latest known-good Edge profile.
    pub fn edge_latest() -> Self {
        Self::Edge { major: 134 }
    }

    /// Short label for logs / UI.
    pub fn label(&self) -> String {
        match self {
            Self::Default => "default (rustls)".to_string(),
            Self::Chrome { major } => format!("Chrome {major}"),
            Self::Safari { major } => format!("Safari {major}"),
            Self::Edge { major } => format!("Edge {major}"),
            Self::Firefox { major } => format!("Firefox {major}"),
            Self::Opera { major } => format!("Opera {major}"),
            Self::OkHttp { major } => format!("OkHttp {major}"),
        }
    }

    /// Map this profile to a `wreq_util::Emulation` variant.
    ///
    /// Returns `None` for [`Self::Default`] (use plain `reqwest` instead).
    /// For browser families that ship multiple minor versions in `wreq-util`,
    /// we pick a sensible recent default keyed off the major.
    #[cfg(feature = "impersonate")]
    pub fn to_emulation(&self) -> Option<wreq_util::Emulation> {
        use wreq_util::Emulation::*;
        Some(match self {
            Self::Default => return None,

            // Chrome ships one variant per major from 100 → 147.
            Self::Chrome { major } => match major {
                147 => Chrome147,
                146 => Chrome146,
                145 => Chrome145,
                144 => Chrome144,
                143 => Chrome143,
                142 => Chrome142,
                141 => Chrome141,
                140 => Chrome140,
                139 => Chrome139,
                138 => Chrome138,
                137 => Chrome137,
                136 => Chrome136,
                135 => Chrome135,
                134 => Chrome134,
                133 => Chrome133,
                132 => Chrome132,
                131 => Chrome131,
                130 => Chrome130,
                129 => Chrome129,
                128 => Chrome128,
                127 => Chrome127,
                126 => Chrome126,
                _ => Chrome147,
            },

            // Safari major → most-representative minor known to wreq-util.
            // wreq-util has separate Safari17_x and Safari18_x minors; we
            // pick the latest patch of each major so the default mapping
            // is forward-looking.
            Self::Safari { major } => match major {
                26 => Safari26,
                18 => Safari18_5,
                17 => Safari17_6,
                16 => Safari16,
                15 => Safari15_6_1,
                _ => Safari26,
            },

            Self::Edge { major } => match major {
                147 => Edge147,
                146 => Edge146,
                145 => Edge145,
                144 => Edge144,
                143 => Edge143,
                142 => Edge142,
                141 => Edge141,
                140 => Edge140,
                139 => Edge139,
                138 => Edge138,
                137 => Edge137,
                136 => Edge136,
                135 => Edge135,
                134 => Edge134,
                131 => Edge131,
                127 => Edge127,
                122 => Edge122,
                _ => Edge147,
            },

            Self::Firefox { major } => match major {
                136 => Firefox136,
                135 => Firefox135,
                133 => Firefox133,
                128 => Firefox128,
                117 => Firefox117,
                109 => Firefox109,
                _ => Firefox136,
            },

            Self::Opera { major } => match major {
                130 => Opera130,
                129 => Opera129,
                128 => Opera128,
                127 => Opera127,
                126 => Opera126,
                125 => Opera125,
                124 => Opera124,
                123 => Opera123,
                122 => Opera122,
                121 => Opera121,
                120 => Opera120,
                119 => Opera119,
                118 => Opera118,
                117 => Opera117,
                116 => Opera116,
                _ => Opera130,
            },

            // OkHttp uses 3.x minor releases; expose `major` as the minor here.
            Self::OkHttp { major } => match major {
                13 => OkHttp3_13,
                11 => OkHttp3_11,
                9 => OkHttp3_9,
                _ => OkHttp3_13,
            },
        })
    }
}

/// HTTP/2 protocol fingerprint hints.
///
/// `wreq` derives these automatically from the chosen [`TlsProfile`] emulation;
/// this struct exists for future fine-grained override (e.g. custom SETTINGS).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Http2Profile {
    /// Override SETTINGS_INITIAL_WINDOW_SIZE. `None` → use emulation default.
    pub initial_window_size: Option<u32>,
    /// Override SETTINGS_MAX_CONCURRENT_STREAMS.
    pub max_concurrent_streams: Option<u32>,
    /// Override SETTINGS_HEADER_TABLE_SIZE.
    pub header_table_size: Option<u32>,
    /// Override SETTINGS_ENABLE_PUSH.
    pub enable_push: Option<bool>,
}

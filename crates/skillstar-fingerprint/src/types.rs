//! Top-level fingerprint entity types.

use crate::{Http2Profile, HttpProfile, IdeTelemetry, TlsProfile};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A composite device-identity descriptor.
///
/// Covers the five layers SkillStar cares about:
///
/// 1. **HTTP** ([`HttpProfile`])      — UA, Accept-Language, client hints
/// 2. **TLS** ([`TlsProfile`])        — JA3/JA4 ClientHello shape
/// 3. **HTTP/2** ([`Http2Profile`])   — SETTINGS / priority overrides
/// 4. **Network** ([`NetworkProfile`])— proxy + DoH + egress claim
/// 5. **Telemetry** (TODO Phase 1)    — IDE-specific machine IDs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceFingerprint {
    pub id: String,
    pub name: String,
    pub source: FingerprintSource,
    pub created_at: i64,
    pub updated_at: i64,

    #[serde(default)]
    pub http: HttpProfile,
    #[serde(default)]
    pub tls: TlsProfile,
    #[serde(default)]
    pub http2: Http2Profile,
    #[serde(default)]
    pub network: NetworkProfile,
    /// IDE telemetry identity applied by [`crate::IdeProjector`] when
    /// the user asks for it. Absent for the immutable `"original"` row.
    #[serde(default, skip_serializing_if = "IdeTelemetry::is_empty")]
    pub telemetry: IdeTelemetry,
}

impl DeviceFingerprint {
    /// Build a new fingerprint with the given name and `Default` TLS.
    /// The id is a fresh UUID.
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now().timestamp();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            source: FingerprintSource::Manual,
            created_at: now,
            updated_at: now,
            http: HttpProfile::default(),
            tls: TlsProfile::default(),
            http2: Http2Profile::default(),
            network: NetworkProfile::default(),
            telemetry: IdeTelemetry::default(),
        }
    }

    /// The "original" fingerprint — what SkillStar looked like before this crate.
    ///
    /// Uses reqwest's stock rustls ClientHello and a generic UA. Cannot be
    /// deleted from the store.
    pub fn original() -> Self {
        Self {
            id: "original".to_string(),
            name: "Original (reqwest default)".to_string(),
            source: FingerprintSource::Original,
            created_at: 0,
            updated_at: 0,
            http: HttpProfile::reqwest_default(),
            tls: TlsProfile::Default,
            http2: Http2Profile::default(),
            network: NetworkProfile::default(),
            telemetry: IdeTelemetry::default(),
        }
    }

    /// Convenience: Chrome 147 on macOS, en-US locale.
    pub fn generate_chrome() -> Self {
        let now = Utc::now().timestamp();
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Chrome 147 (macOS)".to_string(),
            source: FingerprintSource::GeneratedRandom,
            created_at: now,
            updated_at: now,
            http: HttpProfile::chrome_macos(),
            tls: TlsProfile::chrome_latest(),
            http2: Http2Profile::default(),
            network: NetworkProfile::default(),
            telemetry: IdeTelemetry::default(),
        }
    }

    /// Convenience: Safari 26 on macOS, en-US locale.
    pub fn generate_safari() -> Self {
        let now = Utc::now().timestamp();
        Self {
            id: Uuid::new_v4().to_string(),
            name: "Safari 26 (macOS)".to_string(),
            source: FingerprintSource::GeneratedRandom,
            created_at: now,
            updated_at: now,
            http: HttpProfile::safari_macos(),
            tls: TlsProfile::safari_latest(),
            http2: Http2Profile::default(),
            network: NetworkProfile::default(),
            telemetry: IdeTelemetry::default(),
        }
    }

    /// True when this fingerprint should never be deleted by the UI.
    pub fn is_original(&self) -> bool {
        self.id == "original"
    }
}

/// Why a fingerprint exists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FingerprintSource {
    /// Captured from the host system at first launch. Read-only.
    Original,
    /// Generated programmatically.
    GeneratedRandom,
    /// Generated from a named persona template.
    GeneratedFromPersona { template_id: String },
    /// User-imported JSON (or via deep link).
    Imported,
    /// Manually edited fields.
    Manual,
}

/// Network-layer hints. Currently informational; full proxy/DoH wiring
/// will land alongside [`crate::FingerprintAwareClient`] in a later phase.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkProfile {
    /// HTTP/HTTPS proxy URL (e.g. `socks5h://127.0.0.1:1080`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,

    /// DNS-over-HTTPS endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doh_url: Option<String>,

    /// ISO 3166-1 alpha-2 country code the fingerprint *claims* to egress from.
    /// Used for consistency checks (timezone vs IP geolocation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress_country: Option<String>,
}

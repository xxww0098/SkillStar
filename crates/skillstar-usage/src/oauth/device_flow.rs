//! Device Flow (RFC 8628) skeleton.
//!
//! Thin marker module holding the canonical device-code response shape.
//! No catalog currently wires a Device Flow fetcher; keep the type for reuse.

use serde::Deserialize;

/// Canonical RFC 8628 device-code response. Not all providers conform.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    #[serde(default)]
    pub expires_in: u64,
    #[serde(default)]
    pub interval: u64,
}

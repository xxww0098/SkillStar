//! Device Flow (RFC 8628) skeleton.
//!
//! Currently a thin marker module — Qoder's variant lives in
//! `fetchers/oauth/qoder.rs` (it uses `poll_flow` internally because the
//! "state → poll → token" shape is slightly different from canonical RFC 8628).

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

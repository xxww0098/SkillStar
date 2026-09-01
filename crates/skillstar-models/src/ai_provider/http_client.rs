//! Shared HTTP client for AI provider calls.
//!
//! Every remote request in this crate goes through
//! [`skillstar_core::infra::http_client::probe_http_client`] so proxy, SOCKS5h
//! remote DNS, and the no-proxy bypass list cannot drift away from the rest
//! of the app (D-050). The previous local copy of this factory dropped
//! `bypass` and so sent mainland LLM traffic through a GFW-bound proxy.

use anyhow::Result;
use std::time::Duration;

use super::config::AiConfig;

/// Per-request timeout from resolved provider meta (falls back to 120s).
pub fn request_timeout_duration(config: &AiConfig) -> Duration {
    config
        .request_timeout_secs
        .filter(|&s| (5..=600).contains(&s))
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(120))
}

/// Shared HTTP client honouring `~/.skillstar/config/proxy.json`.
pub fn get_http_client() -> Result<reqwest::Client> {
    skillstar_core::infra::http_client::probe_http_client(Duration::from_secs(120))
}

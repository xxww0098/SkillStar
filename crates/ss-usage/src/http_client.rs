//! Proxy-aware HTTP client used by usage fetchers and OAuth flows.
//!
//! [`usage_http_client`] returns a plain `reqwest::Client` with proxy settings
//! from `~/.skillstar/config/proxy.json`. The underlying client is cached and
//! rebuilt whenever the proxy config changes, so callers can request one per
//! request without paying for a rebuild.

use std::time::Duration;

use crate::{UsageError, UsageResult};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared proxy-aware `reqwest::Client` with the default 30s timeout.
pub fn usage_http_client() -> UsageResult<reqwest::Client> {
    usage_http_client_with_timeout(DEFAULT_TIMEOUT)
}

/// Variant of [`usage_http_client`] with a custom timeout, for fetchers whose
/// provider answers faster than the 30s default makes acceptable.
pub fn usage_http_client_with_timeout(timeout: Duration) -> UsageResult<reqwest::Client> {
    ss_core::infra::http_client::probe_http_client(timeout)
        .map_err(|e| UsageError::Other(format!("http client: {e}")))
}

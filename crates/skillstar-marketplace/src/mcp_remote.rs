//! Remote fetch for the GitHub MCP Registry (`https://api.mcp.github.com/v0`).
//!
//! Public, unauthenticated, CORS-enabled. We page through `/v0/servers` via the
//! `metadata.next_cursor` token until exhausted, then hand the normalized
//! servers to the snapshot layer for caching.

use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{debug, warn};

use crate::mcp_models::{McpRegistryServer, parse_servers_response};

const USER_AGENT: &str = concat!("SkillStar/", env!("CARGO_PKG_VERSION"));
const MCP_REGISTRY_BASE: &str = "https://api.mcp.github.com/v0/servers";
const MCP_REGISTRY_TIMEOUT: Duration = Duration::from_secs(30);
const PAGE_LIMIT: u32 = 100;
/// Hard cap on pages, so a misbehaving cursor can't loop forever.
const MAX_PAGES: usize = 25;

fn registry_client() -> Result<reqwest::Client> {
    skillstar_core::infra::http_client::probe_http_client(MCP_REGISTRY_TIMEOUT)
        .context("Failed to build MCP registry HTTP client")
}

/// Fetch one page of the registry, returning normalized servers and the next
/// cursor (if any).
async fn fetch_page(
    client: &reqwest::Client,
    cursor: Option<&str>,
) -> Result<(Vec<McpRegistryServer>, Option<String>)> {
    let mut url = format!("{MCP_REGISTRY_BASE}?limit={PAGE_LIMIT}");
    if let Some(cursor) = cursor {
        url.push_str("&cursor=");
        url.push_str(&urlencoding_minimal(cursor));
    }

    let body = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .send()
        .await
        .context("Failed to call GitHub MCP registry")?
        .error_for_status()
        .context("GitHub MCP registry returned an error status")?
        .text()
        .await
        .context("Failed to read GitHub MCP registry response body")?;

    parse_servers_response(&body)
}

/// Fetch the full GitHub MCP Registry catalog (all pages).
pub async fn fetch_mcp_registry() -> Result<Vec<McpRegistryServer>> {
    let client = registry_client()?;
    let mut all: Vec<McpRegistryServer> = Vec::new();
    let mut cursor: Option<String> = None;

    for page in 0..MAX_PAGES {
        let (servers, next) = fetch_page(&client, cursor.as_deref())
            .await
            .with_context(|| format!("Failed to fetch MCP registry page {page}"))?;
        debug!(
            target: "mcp_marketplace",
            page,
            fetched = servers.len(),
            "fetched MCP registry page"
        );
        all.extend(servers);
        match next {
            Some(next) if !next.is_empty() => cursor = Some(next),
            _ => return Ok(all),
        }
    }

    warn!(
        target: "mcp_marketplace",
        max_pages = MAX_PAGES,
        fetched = all.len(),
        "MCP registry pagination hit page cap; returning what we have"
    );
    Ok(all)
}

/// Minimal percent-encoding for the opaque cursor token (alnum/`-_.~` pass
/// through; everything else is `%XX`). Avoids pulling in a urlencoding dep.
fn urlencoding_minimal(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_cursor_safely() {
        assert_eq!(urlencoding_minimal("abc-123_X.~"), "abc-123_X.~");
        assert_eq!(urlencoding_minimal("a b/c="), "a%20b%2Fc%3D");
    }

    /// End-to-end smoke test against the live GitHub MCP Registry. Network-gated
    /// (run explicitly with `--ignored`): `cargo test -p skillstar-marketplace
    /// --ignored fetch_real_registry`.
    #[tokio::test]
    #[ignore = "hits the network; run with --ignored"]
    async fn fetch_real_registry_returns_many_servers() {
        let servers = fetch_mcp_registry().await.expect("fetch live MCP registry");
        assert!(
            servers.len() > 20,
            "expected >20 servers, got {}",
            servers.len()
        );
        assert!(
            servers.iter().any(|s| !s.packages.is_empty()),
            "expected at least one stdio-package server"
        );
        assert!(
            servers
                .iter()
                .all(|s| !s.name.is_empty() && !s.raw_server_json.is_empty()),
            "every server should have a name and raw json for install mapping"
        );
    }
}

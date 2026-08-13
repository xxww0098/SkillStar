//! Paging engine for one MCP source.
//!
//! What this buys over the previous straight-line loop:
//!
//! - **No silent truncation.** Hitting the page cap or losing a page mid-way
//!   is recorded as a `degraded_reason` that the snapshot layer persists, so a
//!   partial catalog is never mistaken for a complete one (audit A.1-a).
//! - **Partial success.** A page that fails after its retries ends the walk
//!   and keeps everything fetched so far, instead of throwing the whole sync
//!   away (audit A.1-b).
//! - **Rate-limit awareness.** `x-ratelimit-remaining` / `x-ratelimit-reset`
//!   and `429`/`Retry-After` drive a bounded backoff. The GitHub registry
//!   advertises `x-ratelimit-limit: 10`, which the old 25-page walk could
//!   blow straight through.
//! - **Conditional requests.** The stored ETag rides on the first page, so an
//!   unchanged catalog costs one `304` instead of a full walk.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::StatusCode;
use tracing::{debug, warn};

use crate::mcp_models::{McpRegistryServer, parse_servers_page};
use crate::remote::sha256_hex;

use super::sources::{McpSourceDescriptor, McpSourceKind};

const USER_AGENT: &str = concat!("SkillStar/", env!("CARGO_PKG_VERSION"));
const REGISTRY_TIMEOUT: Duration = Duration::from_secs(30);
const PAGE_LIMIT: u32 = 100;
/// Attempts per page, including the first.
const MAX_ATTEMPTS: u32 = 3;
const BASE_BACKOFF: Duration = Duration::from_millis(500);
/// Ceiling for any single wait, so a hostile `Retry-After` can't hang a sync.
const MAX_BACKOFF: Duration = Duration::from_secs(20);

/// Result of reading one source end to end.
#[derive(Debug, Clone)]
pub struct SourceFetch {
    pub source_id: String,
    pub servers: Vec<McpRegistryServer>,
    pub payload_sha256: String,
    pub source_host: String,
    pub etag: Option<String>,
    /// The server answered `304`: our cached rows are still current.
    pub unchanged: bool,
    /// Set when the catalog we return is knowingly incomplete.
    pub degraded_reason: Option<String>,
}

fn registry_client() -> Result<reqwest::Client> {
    skillstar_core::infra::http_client::probe_http_client(REGISTRY_TIMEOUT)
        .context("Failed to build MCP registry HTTP client")
}

/// Rate-limit signals from one response.
#[derive(Debug, Default, Clone, Copy)]
struct RateLimit {
    remaining: Option<u64>,
    /// Seconds to wait before the window resets.
    reset_in: Option<u64>,
}

impl RateLimit {
    fn from_headers(headers: &reqwest::header::HeaderMap) -> Self {
        let remaining = header_u64(headers, "x-ratelimit-remaining");
        // The header is an absolute epoch second on the GitHub registry and a
        // delta on some proxies; treat anything implausibly large as an epoch.
        let reset_in = header_u64(headers, "x-ratelimit-reset").map(|value| {
            let now = chrono::Utc::now().timestamp().max(0) as u64;
            if value > now {
                value - now
            } else if value < 3600 {
                value
            } else {
                0
            }
        });
        Self {
            remaining,
            reset_in,
        }
    }

    /// How long to wait before the *next* page, if the window is exhausted.
    fn cooldown(&self) -> Option<Duration> {
        match (self.remaining, self.reset_in) {
            (Some(0), Some(seconds)) if seconds > 0 => {
                Some(Duration::from_secs(seconds).min(MAX_BACKOFF))
            }
            (Some(0), _) => Some(BASE_BACKOFF),
            _ => None,
        }
    }
}

fn header_u64(headers: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

fn header_string(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// One HTTP page.
struct PageResponse {
    status: StatusCode,
    body: String,
    etag: Option<String>,
    rate: RateLimit,
}

fn page_url(source: &McpSourceDescriptor, cursor: Option<&str>, with_extra_query: bool) -> String {
    let mut url = format!("{}?limit={PAGE_LIMIT}", source.base_url);
    if with_extra_query
        && let Some(extra) = source
            .list_query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
    {
        url.push('&');
        url.push_str(extra);
    }
    if let Some(cursor) = cursor {
        url.push_str("&cursor=");
        url.push_str(&urlencoding_minimal(cursor));
    }
    url
}

async fn request_page(
    client: &reqwest::Client,
    url: &str,
    etag: Option<&str>,
) -> Result<PageResponse> {
    let mut request = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json");
    if let Some(etag) = etag {
        request = request.header("If-None-Match", etag);
    }
    let response = request
        .send()
        .await
        .context("Failed to call MCP registry")?;
    let status = response.status();
    let headers = response.headers().clone();
    let rate = RateLimit::from_headers(&headers);
    if headers
        .get("deprecation")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    {
        warn!(
            target: "mcp_marketplace",
            url,
            "MCP registry endpoint reports Deprecation: true"
        );
    }
    let etag = header_string(&headers, "etag");
    let retry_after = header_u64(&headers, "retry-after");
    if status == StatusCode::TOO_MANY_REQUESTS {
        let wait = retry_after
            .map(Duration::from_secs)
            .unwrap_or(BASE_BACKOFF)
            .min(MAX_BACKOFF);
        tokio::time::sleep(wait).await;
        return Err(anyhow!("MCP registry rate limited (429) at {url}"));
    }
    if status == StatusCode::NOT_MODIFIED {
        return Ok(PageResponse {
            status,
            body: String::new(),
            etag,
            rate,
        });
    }
    if !status.is_success() {
        return Err(anyhow!("MCP registry returned {status} for {url}"));
    }
    let body = response
        .text()
        .await
        .context("Failed to read MCP registry response body")?;
    Ok(PageResponse {
        status,
        body,
        etag,
        rate,
    })
}

/// Retry wrapper: exponential backoff, bounded, with the last error preserved.
async fn request_page_with_retry(
    client: &reqwest::Client,
    url: &str,
    etag: Option<&str>,
) -> Result<PageResponse> {
    let mut last_error = None;
    for attempt in 0..MAX_ATTEMPTS {
        match request_page(client, url, etag).await {
            Ok(page) => return Ok(page),
            Err(err) => {
                debug!(
                    target: "mcp_marketplace",
                    url, attempt, error = %err,
                    "MCP registry page attempt failed"
                );
                last_error = Some(err);
                if attempt + 1 < MAX_ATTEMPTS {
                    let backoff = (BASE_BACKOFF * 2_u32.pow(attempt)).min(MAX_BACKOFF);
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("MCP registry page failed with no error recorded")))
}

/// Read every page of one source.
pub async fn fetch_source(
    source: &McpSourceDescriptor,
    prev_etag: Option<&str>,
) -> Result<SourceFetch> {
    match source.kind {
        McpSourceKind::LocalDirectory => fetch_local_directory(source),
        McpSourceKind::Registry => fetch_registry(source, prev_etag).await,
    }
}

fn fetch_local_directory(source: &McpSourceDescriptor) -> Result<SourceFetch> {
    let body = std::fs::read_to_string(&source.base_url)
        .with_context(|| format!("Failed to read MCP directory file {}", source.base_url))?;
    let page = parse_servers_page(&body, source.cursor_style, Some(&source.id))
        .with_context(|| format!("Failed to parse MCP directory file {}", source.base_url))?;
    Ok(SourceFetch {
        source_id: source.id.clone(),
        servers: page.servers,
        payload_sha256: sha256_hex(&body),
        source_host: source.source_host(),
        etag: None,
        unchanged: false,
        degraded_reason: None,
    })
}

async fn fetch_registry(
    source: &McpSourceDescriptor,
    prev_etag: Option<&str>,
) -> Result<SourceFetch> {
    let client = registry_client()?;
    let mut all: Vec<McpRegistryServer> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut canonical_bytes = String::new();
    let mut first_etag: Option<String> = None;
    let mut degraded_reason: Option<String> = None;
    // Set to false once the source rejects our extra query params, so the walk
    // can continue against a registry that doesn't implement them.
    let mut with_extra_query = true;
    let mut reported_total: Option<u64> = None;

    for page_index in 0..source.max_pages {
        let url = page_url(source, cursor.as_deref(), with_extra_query);
        let conditional = (page_index == 0).then_some(prev_etag).flatten();
        let response = match request_page_with_retry(&client, &url, conditional).await {
            Ok(response) => response,
            Err(err) => {
                if page_index == 0 {
                    // A registry that rejects `version=latest` should still be
                    // readable without it — try once before giving up.
                    if with_extra_query && source.list_query.is_some() {
                        with_extra_query = false;
                        warn!(
                            target: "mcp_marketplace",
                            source = %source.id, error = %err,
                            "MCP source rejected the list query; retrying without it"
                        );
                        continue;
                    }
                    return Err(err.context(format!(
                        "Failed to fetch MCP source '{}' first page",
                        source.id
                    )));
                }
                // Partial success: keep the pages we already have and say so.
                warn!(
                    target: "mcp_marketplace",
                    source = %source.id, page = page_index, error = %err,
                    "MCP source page failed; keeping earlier pages"
                );
                degraded_reason = Some(format!(
                    "{}: page {page_index} failed ({err}); kept {} servers",
                    source.id,
                    all.len()
                ));
                break;
            }
        };

        if response.status == StatusCode::NOT_MODIFIED {
            return Ok(SourceFetch {
                source_id: source.id.clone(),
                servers: Vec::new(),
                payload_sha256: String::new(),
                source_host: source.source_host(),
                etag: response.etag.or_else(|| prev_etag.map(str::to_string)),
                unchanged: true,
                degraded_reason: None,
            });
        }

        if page_index == 0 {
            first_etag = response.etag.clone();
        }

        let page = parse_servers_page(&response.body, source.cursor_style, Some(&source.id))
            .with_context(|| {
                format!(
                    "Failed to parse MCP source '{}' page {page_index}",
                    source.id
                )
            })?;
        debug!(
            target: "mcp_marketplace",
            source = %source.id,
            page = page_index,
            fetched = page.servers.len(),
            "fetched MCP registry page"
        );
        canonical_bytes.push_str(&response.body);
        reported_total = reported_total.or(page.total);
        all.extend(page.servers);

        match page.next_cursor {
            Some(next) if !next.is_empty() => cursor = Some(next),
            _ => {
                return Ok(SourceFetch {
                    source_id: source.id.clone(),
                    servers: all,
                    payload_sha256: sha256_hex(&canonical_bytes),
                    source_host: source.source_host(),
                    etag: first_etag,
                    unchanged: false,
                    degraded_reason,
                });
            }
        }

        if let Some(cooldown) = response.rate.cooldown() {
            debug!(
                target: "mcp_marketplace",
                source = %source.id,
                seconds = cooldown.as_secs(),
                "MCP source rate limit exhausted; waiting before next page"
            );
            tokio::time::sleep(cooldown).await;
        }
    }

    if degraded_reason.is_none() {
        // We left the loop with a live cursor: the cap truncated the catalog.
        let total = reported_total
            .map(|t| format!(" of {t} reported"))
            .unwrap_or_default();
        let reason = format!(
            "{}: pagination hit the {}-page cap; kept {}{total} servers",
            source.id,
            source.max_pages,
            all.len()
        );
        warn!(target: "mcp_marketplace", reason, "MCP source truncated");
        degraded_reason = Some(reason);
    }

    Ok(SourceFetch {
        source_id: source.id.clone(),
        servers: all,
        payload_sha256: sha256_hex(&canonical_bytes),
        source_host: source.source_host(),
        etag: first_etag,
        unchanged: false,
        degraded_reason,
    })
}

/// Minimal percent-encoding for the opaque cursor token (alnum/`-_.~` pass
/// through; everything else is `%XX`). Avoids pulling in a urlencoding dep.
pub(crate) fn urlencoding_minimal(value: &str) -> String {
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
pub(crate) fn test_page_url(
    source: &McpSourceDescriptor,
    cursor: Option<&str>,
    with_extra_query: bool,
) -> String {
    page_url(source, cursor, with_extra_query)
}

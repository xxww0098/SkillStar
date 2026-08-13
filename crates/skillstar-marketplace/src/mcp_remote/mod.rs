//! Remote catalog fetch for the MCP marketplace.
//!
//! The catalog is no longer "whatever `api.mcp.github.com/v0` returned". It is
//! the merge of every enabled [`sources::McpSourceDescriptor`]: the official
//! MCP Registry as the primary source of truth, GitHub's registry as a
//! display-enrichment mirror, plus any registry URL or local JSON directory
//! the user added.
//!
//! Layout:
//! - [`sources`] — the source registry (built-ins + user overrides).
//! - [`config`] — persistence for user-added sources.
//! - [`fetch`] — per-source paging, retries, rate limits, ETags.
//! - [`merge`] — version dedup and cross-source field merging.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use tracing::warn;

use crate::mcp_models::McpRegistryServer;
use crate::remote::{FetchMeta, sha256_hex};

pub mod config;
pub mod fetch;
pub mod merge;
pub mod sources;

#[cfg(test)]
mod tests;

pub use config::{McpCustomSource, McpSourcesConfig};
pub use sources::{McpSourceDescriptor, McpSourceKind, McpSourceLicense};

/// What one source contributed to a catalog fetch.
#[derive(Debug, Clone)]
pub struct McpSourceOutcome {
    pub source_id: String,
    pub display_name: String,
    pub source_host: String,
    pub payload_sha256: Option<String>,
    pub etag: Option<String>,
    pub unchanged: bool,
    pub server_count: usize,
    /// Set when the source could not be read at all.
    pub error: Option<String>,
    /// Set when the source was read but the result is knowingly incomplete.
    pub degraded_reason: Option<String>,
}

impl McpSourceOutcome {
    pub fn succeeded(&self) -> bool {
        self.error.is_none()
    }
}

/// A full catalog fetch across every enabled source.
#[derive(Debug, Clone)]
pub struct McpCatalogFetch {
    pub servers: Vec<McpRegistryServer>,
    pub meta: FetchMeta,
    pub outcomes: Vec<McpSourceOutcome>,
    /// Every source answered `304`: the local catalog is already current and
    /// `servers` is intentionally empty.
    pub all_unchanged: bool,
}

/// Fetch and merge every enabled source.
///
/// `prev_etags` maps source id → last stored ETag, used for conditional
/// requests. A source that answers `304` while another source *did* change is
/// re-fetched unconditionally: the catalog is stored as one fully-replaced
/// table, so a partial merge would delete the unchanged source's rows.
pub async fn fetch_mcp_catalog(prev_etags: &HashMap<String, String>) -> Result<McpCatalogFetch> {
    let sources = sources::active_sources();
    if sources.is_empty() {
        return Err(anyhow!(
            "No MCP catalog sources are enabled; enable one in the MCP source settings"
        ));
    }

    let mut fetched: Vec<(McpSourceDescriptor, Result<fetch::SourceFetch>)> = Vec::new();
    for source in sources {
        let etag = prev_etags.get(&source.id).map(String::as_str);
        let result = fetch::fetch_source(&source, etag).await;
        if let Err(err) = &result {
            warn!(
                target: "mcp_marketplace",
                source = %source.id, error = %format!("{err:#}"),
                "MCP source fetch failed"
            );
        }
        fetched.push((source, result));
    }

    let any_success = fetched.iter().any(|(_, result)| result.is_ok());
    if !any_success {
        let detail = fetched
            .iter()
            .filter_map(|(source, result)| {
                result
                    .as_ref()
                    .err()
                    .map(|err| format!("{}: {err:#}", source.id))
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(anyhow!("Every MCP catalog source failed ({detail})"));
    }

    let all_unchanged = fetched
        .iter()
        .all(|(_, result)| result.as_ref().is_ok_and(|fetch| fetch.unchanged));
    if all_unchanged {
        let outcomes = fetched
            .iter()
            .map(|(source, result)| outcome_for(source, result.as_ref().ok(), None))
            .collect::<Vec<_>>();
        let meta = aggregate_meta(&outcomes, true);
        return Ok(McpCatalogFetch {
            servers: Vec::new(),
            meta,
            outcomes,
            all_unchanged: true,
        });
    }

    // At least one source changed → the swap will rewrite the whole table, so
    // every 304'd source has to be read in full after all.
    for (source, result) in fetched.iter_mut() {
        if result.as_ref().is_ok_and(|fetch| fetch.unchanged) {
            *result = fetch::fetch_source(source, None).await;
        }
    }

    let mut outcomes: Vec<McpSourceOutcome> = Vec::new();
    let mut inputs: Vec<merge::SourceServers> = Vec::new();
    for (source, result) in &fetched {
        match result {
            Ok(source_fetch) => {
                outcomes.push(outcome_for(source, Some(source_fetch), None));
                inputs.push(merge::SourceServers {
                    source_id: source.id.clone(),
                    priority: source.priority,
                    servers: source_fetch.servers.clone(),
                });
            }
            Err(err) => outcomes.push(outcome_for(source, None, Some(format!("{err:#}")))),
        }
    }

    let servers = merge::merge_catalogs(inputs);
    let meta = aggregate_meta(&outcomes, false);
    Ok(McpCatalogFetch {
        servers,
        meta,
        outcomes,
        all_unchanged: false,
    })
}

/// Human-readable summary of everything degraded about a fetch, or `None` when
/// the catalog is complete.
pub fn degraded_reason(outcomes: &[McpSourceOutcome]) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for outcome in outcomes {
        if let Some(error) = &outcome.error {
            parts.push(format!("{} unavailable: {error}", outcome.source_id));
        } else if let Some(reason) = &outcome.degraded_reason {
            parts.push(reason.clone());
        }
    }
    (!parts.is_empty()).then(|| parts.join(" | "))
}

fn outcome_for(
    source: &McpSourceDescriptor,
    fetched: Option<&fetch::SourceFetch>,
    error: Option<String>,
) -> McpSourceOutcome {
    McpSourceOutcome {
        source_id: source.id.clone(),
        display_name: source.display_name.clone(),
        source_host: source.source_host(),
        payload_sha256: fetched
            .map(|f| f.payload_sha256.clone())
            .filter(|hash| !hash.is_empty()),
        etag: fetched.and_then(|f| f.etag.clone()),
        unchanged: fetched.is_some_and(|f| f.unchanged),
        server_count: fetched.map(|f| f.servers.len()).unwrap_or(0),
        error,
        degraded_reason: fetched.and_then(|f| f.degraded_reason.clone()),
    }
}

/// Aggregate fingerprint across sources.
///
/// Hashing `id=hash` pairs in id order (rather than concatenating bodies) keeps
/// the fingerprint stable when a source is added, removed, or answers 304 —
/// which is exactly when the old whole-body hash produced a spurious "changed".
fn aggregate_meta(outcomes: &[McpSourceOutcome], unchanged: bool) -> FetchMeta {
    let mut pairs: Vec<String> = outcomes
        .iter()
        .filter(|outcome| outcome.succeeded())
        .map(|outcome| {
            format!(
                "{}={}",
                outcome.source_id,
                outcome.payload_sha256.as_deref().unwrap_or("unchanged")
            )
        })
        .collect();
    pairs.sort();
    let hosts: Vec<String> = outcomes
        .iter()
        .filter(|outcome| outcome.succeeded())
        .map(|outcome| outcome.source_host.clone())
        .collect();
    FetchMeta {
        payload_sha256: sha256_hex(&pairs.join("\n")),
        source_host: if hosts.is_empty() {
            "none".to_string()
        } else {
            hosts.join(",")
        },
        // Per-source ETags are stored on per-source sync-state rows; the
        // aggregate scope has no single ETag to speak of.
        etag: None,
        degraded: !unchanged
            && outcomes
                .iter()
                .any(|o| !o.succeeded() || o.degraded_reason.is_some()),
    }
}

/// Back-compatible single-call fetch: merged catalog + aggregate metadata.
pub async fn fetch_mcp_registry() -> Result<(Vec<McpRegistryServer>, FetchMeta)> {
    let fetched = fetch_mcp_catalog(&HashMap::new()).await?;
    Ok((fetched.servers, fetched.meta))
}

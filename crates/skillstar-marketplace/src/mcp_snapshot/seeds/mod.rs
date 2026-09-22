//! Curated MCP seed data.
//!
//! [`catalog::catalog`] is the single source of truth for the rows; this module
//! only turns it into the priority-ordered seed list that
//! `seed_default_curated_mcp_servers` writes into `mcp_curated_server`.
//!
//! `priority` is the global sort key the store uses (`is_recommended DESC,
//! priority ASC, name ASC`), so the catalog's own order *is* the store order —
//! no second ordering table to keep in sync. Retiring a server is likewise one
//! edit: delete the row from the catalog, and `prune_retired_curated_rows`
//! drops it (and its FTS row) from the snapshot on the next seed.

mod catalog;
mod helpers;

use crate::mcp_models::McpRegistryServer;

use helpers::build;

pub(super) struct CuratedMcpSeed {
    pub(super) priority: i64,
    pub(super) server: McpRegistryServer,
}

pub(super) fn default_curated_mcp_servers() -> Vec<CuratedMcpSeed> {
    catalog::catalog()
        .iter()
        .enumerate()
        .map(|(index, spec)| CuratedMcpSeed {
            priority: index as i64,
            server: build(spec),
        })
        .collect()
}

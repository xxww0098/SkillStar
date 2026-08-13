//! Parameterized query shape for MCP marketplace cards.
//!
//! Before this existed the SQL layer offered exactly two reads — "everything,
//! in one hardcoded order" and "FTS search, same hardcoded order, with a
//! LIMIT" — so the whole GitHub registry crossed the IPC boundary on every
//! list and every filter ran in the frontend's memory (audit A.3-d / A.3-e).
//!
//! Everything here is data, never interpolated SQL: sort keys are an enum
//! mapped to fixed `ORDER BY` fragments and every value is bound as a
//! parameter, so no caller can reach the query text.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::mcp_models::{McpMarketEntry, McpServerKind, McpServerStatus};

/// Ordering options. `Default` is the historical order — recommended first,
/// then curated priority, then stars — and stays the default so existing UI
/// does not shift.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpSortKey.ts")]
pub enum McpSortKey {
    #[default]
    Default,
    Stars,
    Name,
    Updated,
    Published,
}

impl McpSortKey {
    /// Whether this key sorts descending unless the caller says otherwise.
    fn defaults_to_descending(self) -> bool {
        !matches!(self, McpSortKey::Name)
    }

    /// Fixed `ORDER BY` fragment. `search_rank` is only meaningful when the
    /// query carries a search term, so it is spliced in by the builder.
    pub(crate) fn order_by(self, descending: bool, ranked: bool) -> String {
        let dir = if descending { "DESC" } else { "ASC" };
        let rank = if ranked { "search_rank ASC, " } else { "" };
        match self {
            McpSortKey::Default => {
                format!("recommended DESC, sort_priority ASC, {rank}stars DESC, name ASC")
            }
            McpSortKey::Stars => format!("stars {dir}, {rank}name ASC"),
            McpSortKey::Name => format!("name {dir}, {rank}stars DESC"),
            McpSortKey::Updated => {
                format!("COALESCE(updated_at, '') {dir}, {rank}stars DESC, name ASC")
            }
            McpSortKey::Published => {
                format!("COALESCE(published_at, '') {dir}, {rank}stars DESC, name ASC")
            }
        }
    }
}

/// A card query: filters + ordering + pagination.
///
/// `Default::default()` is "every card, historical order, no pagination" —
/// exactly what `load_cards` used to do.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "McpServerQuery.ts")]
pub struct McpServerQuery {
    /// FTS search terms. Empty/whitespace behaves like no search.
    pub search: Option<String>,
    /// `"github"` reads the remote registry table; any other value reads the
    /// curated bucket with that `source`; `None` reads both.
    pub publisher_id: Option<String>,
    pub kinds: Vec<McpServerKind>,
    /// Runner commands (`npx`, `uvx`, `docker`, …). A row matches when any of
    /// its runtimes is listed.
    pub runtimes: Vec<String>,
    /// SPDX-ish license strings, matched case-insensitively.
    pub licenses: Vec<String>,
    /// Lifecycle statuses to include. Empty means "no status filter" —
    /// deprecated rows are still returned, just flagged, matching the
    /// registry's own "list but warn" rule.
    pub statuses: Vec<McpServerStatus>,
    pub recommended_only: bool,
    /// Drop rows the registry has superseded with a newer version.
    pub latest_only: bool,
    pub min_stars: Option<u32>,
    pub max_stars: Option<u32>,
    pub sort: McpSortKey,
    /// Overrides the sort key's natural direction.
    pub descending: Option<bool>,
    /// `None` means unpaged (every match).
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl McpServerQuery {
    /// Search terms, or `None` when the query is blank.
    pub(crate) fn search_terms(&self) -> Option<&str> {
        self.search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    pub(crate) fn is_descending(&self) -> bool {
        self.descending
            .unwrap_or_else(|| self.sort.defaults_to_descending())
    }

    /// Convenience for the common "search with a cap" call.
    pub fn search_query(query: &str, limit: u32) -> Self {
        Self {
            search: Some(query.to_string()),
            limit: Some(limit),
            ..Self::default()
        }
    }

    /// Convenience for "one publisher's cards".
    pub fn for_publisher(publisher_id: &str) -> Self {
        Self {
            publisher_id: Some(publisher_id.to_string()),
            ..Self::default()
        }
    }
}

/// One page of cards plus the total number of matches, so a UI can render
/// "showing 60 of 218" without a second round trip.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpServerPage.ts")]
pub struct McpServerPage {
    pub items: Vec<McpMarketEntry>,
    /// Matches before `limit`/`offset` were applied.
    pub total: u32,
    pub offset: u32,
    pub limit: Option<u32>,
}

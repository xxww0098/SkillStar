//! Data model for the MCP server marketplace.
//!
//! Shapes follow the official `server.json` schema
//! (`https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json`)
//! and are shared by every source we consume — the official MCP Registry, the
//! GitHub MCP Registry mirror, user-supplied registries, and local JSON
//! directory files.
//!
//! These types are the local-snapshot + IPC representation. They intentionally
//! use `camelCase` serde (matching the MCP domain's `McpServerEntry`) so the
//! frontend MCP feature sees one consistent naming convention.
//!
//! The snapshot also keeps the **raw `server` JSON** for each entry
//! (`raw_server_json`) so the app layer can reconstruct a full install spec
//! (`McpServerEntry`) without re-fetching — see `registry_to_entry`.
//!
//! Layout:
//! - this `mod.rs` — the snapshot/card/detail records.
//! - [`inputs`] — `Input` semantics (env vars, headers, CLI arguments).
//! - [`spec`] — packages, remotes, icons, lifecycle status.
//! - [`parse`] — wire → snapshot parsing, shared by all sources.
//! - [`raw`] — camelCase/snake_case tolerant JSON readers.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub mod inputs;
pub mod parse;
mod raw;
pub mod spec;

#[cfg(test)]
mod tests;

pub use inputs::{
    McpArgument, McpArgumentKind, McpInput, McpInputFormat, McpInputVariable, McpKeyValueInput,
};
pub use parse::{
    McpCursorStyle, McpRegistryPage, parse_registry_element, parse_registry_element_from,
    parse_servers_page, parse_servers_response,
};
pub use spec::{
    McpIcon, McpRegistryPackageSummary, McpRegistryRemoteSummary, McpServerStatus,
    McpTransportSpec, runtime_command_for,
};

/// Whether a registry server can run locally (stdio package), remotely
/// (http/sse url), or both.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpServerKind.ts")]
pub enum McpServerKind {
    Stdio,
    Remote,
    Both,
    #[default]
    Unknown,
}

impl McpServerKind {
    /// Stable string used for DB storage (matches the camelCase serde tag).
    pub fn as_db_str(&self) -> &'static str {
        match self {
            McpServerKind::Stdio => "stdio",
            McpServerKind::Remote => "remote",
            McpServerKind::Both => "both",
            McpServerKind::Unknown => "unknown",
        }
    }

    pub fn from_db_str(value: &str) -> Self {
        match value {
            "stdio" => McpServerKind::Stdio,
            "remote" => McpServerKind::Remote,
            "both" => McpServerKind::Both,
            _ => McpServerKind::Unknown,
        }
    }
}

/// A normalized registry server stored in the local snapshot. Carries card
/// metadata, structured package/remote specs, readme, and the raw `server`
/// JSON for install.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRegistryServer {
    /// Stable registry id (falls back to `namespace` when absent).
    pub id: String,
    /// Cleaned display name — last path segment of `namespace`.
    pub name: String,
    /// Full registry name, e.g. `io.github.netdata/mcp-server`. This is the
    /// reverse-DNS identity the multi-source merge keys on.
    pub namespace: String,
    pub description: String,
    pub repo_url: String,
    pub stars: u32,
    pub license: Option<String>,
    pub version: Option<String>,
    pub kind: McpServerKind,
    /// Distinct runner hints across packages, e.g. `["uvx"]`, `["npx"]`.
    pub runtimes: Vec<String>,
    pub readme: Option<String>,
    pub updated_at: Option<String>,
    pub packages: Vec<McpRegistryPackageSummary>,
    pub remotes: Vec<McpRegistryRemoteSummary>,
    /// Serialized `server` object — input for the app's install mapping.
    pub raw_server_json: String,
    /// Curated SkillStar recommendation, stored outside the remote registry so
    /// refreshes never overwrite it.
    #[serde(default)]
    pub recommended: bool,
    /// Publisher bucket for local curated rows, e.g. `skillstar-curated`.
    #[serde(default)]
    pub source: Option<String>,
    /// Human display name, distinct from the reverse-DNS `namespace`.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub icons: Vec<McpIcon>,
    /// `active` / `deprecated` / `deleted` — a queryable column, so the UI can
    /// flag deprecated servers instead of offering them like healthy ones.
    #[serde(default)]
    pub status: McpServerStatus,
    /// False when the registry knows of a newer version of this server.
    #[serde(default = "default_true")]
    pub is_latest: bool,
    #[serde(default)]
    pub published_at: Option<String>,
    /// Id of the remote source this row was primarily built from.
    #[serde(default)]
    pub registry_source: Option<String>,
    /// Every source that contributed a field to this row, after the merge.
    #[serde(default)]
    pub contributing_sources: Vec<String>,
}

fn default_true() -> bool {
    true
}

// Hand-written so `is_latest` defaults to `true`. `#[derive(Default)]` would
// make it `false`, i.e. every row built with `..Default::default()` (the
// curated seeds, tests) would claim the registry knows a newer version and the
// UI would flag them all as outdated.
impl Default for McpRegistryServer {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            namespace: String::new(),
            description: String::new(),
            repo_url: String::new(),
            stars: 0,
            license: None,
            version: None,
            kind: McpServerKind::Unknown,
            runtimes: Vec::new(),
            readme: None,
            updated_at: None,
            packages: Vec::new(),
            remotes: Vec::new(),
            raw_server_json: String::new(),
            recommended: false,
            source: None,
            title: None,
            website_url: None,
            icons: Vec::new(),
            status: McpServerStatus::Active,
            is_latest: true,
            published_at: None,
            registry_source: None,
            contributing_sources: Vec::new(),
        }
    }
}

/// Lightweight card model for list/search results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpMarketEntry.ts")]
pub struct McpMarketEntry {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub description: String,
    pub repo_url: String,
    pub stars: u32,
    pub license: Option<String>,
    pub version: Option<String>,
    pub kind: McpServerKind,
    pub runtimes: Vec<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub website_url: Option<String>,
    /// First declared icon, so a card can render without loading the detail.
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub status: McpServerStatus,
    #[serde(default = "default_true")]
    pub is_latest: bool,
    #[serde(default)]
    pub registry_source: Option<String>,
}

/// Detail model: card fields + readme + full package/remote specs.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpMarketServerDetail.ts")]
pub struct McpMarketServerDetail {
    #[serde(flatten)]
    #[ts(flatten)]
    pub entry: McpMarketEntry,
    pub readme: Option<String>,
    pub packages: Vec<McpRegistryPackageSummary>,
    pub remotes: Vec<McpRegistryRemoteSummary>,
    #[serde(default)]
    pub icons: Vec<McpIcon>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub contributing_sources: Vec<String>,
}

/// One official MCP publisher shown on the marketplace grid.
///
/// The `id` doubles as the curated `source` bucket (`"adspower"` / `"bigmodel"`)
/// or the special `"github"` publisher which maps to the full
/// `mcp_registry_server` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpPublisherSummary.ts")]
pub struct McpPublisherSummary {
    /// Publisher id — also the curated `source` value, or `"github"`.
    pub id: String,
    /// Display name (e.g. "AdsPower", "BigModel", "GitHub").
    pub name: String,
    /// Number of MCP servers offered by this publisher.
    pub server_count: u32,
    /// External landing page (docs / repo).
    pub url: String,
}

impl McpRegistryServer {
    pub fn to_card(&self) -> McpMarketEntry {
        McpMarketEntry {
            id: self.id.clone(),
            name: self.name.clone(),
            namespace: self.namespace.clone(),
            description: self.description.clone(),
            repo_url: self.repo_url.clone(),
            stars: self.stars,
            license: self.license.clone(),
            version: self.version.clone(),
            kind: self.kind,
            runtimes: self.runtimes.clone(),
            updated_at: self.updated_at.clone(),
            recommended: self.recommended,
            source: self.source.clone(),
            title: self.title.clone(),
            website_url: self.website_url.clone(),
            icon_url: self.icons.first().map(|icon| icon.src.clone()),
            status: self.status,
            is_latest: self.is_latest,
            registry_source: self.registry_source.clone(),
        }
    }

    pub fn to_detail(&self) -> McpMarketServerDetail {
        McpMarketServerDetail {
            entry: self.to_card(),
            readme: self.readme.clone(),
            packages: self.packages.clone(),
            remotes: self.remotes.clone(),
            icons: self.icons.clone(),
            published_at: self.published_at.clone(),
            contributing_sources: self.contributing_sources.clone(),
        }
    }
}

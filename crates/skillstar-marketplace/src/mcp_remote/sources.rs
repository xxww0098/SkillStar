//! The MCP source registry.
//!
//! Every place the catalog can come from is described by one
//! [`McpSourceDescriptor`] — built-in registries, user-added registries, and
//! local JSON directory files alike. Nothing downstream hardcodes a URL, so
//! adding a source is data, not control flow.
//!
//! Ranking (`priority`, lower wins) encodes *authority*, not preference:
//!
//! | source | authority | why |
//! | --- | --- | --- |
//! | Official MCP Registry | 0 | the publisher's own record, and the only source whose data is released under a re-distribution licence (CC0 1.0), so it is the only one we may mirror long-term |
//! | GitHub MCP Registry | 10 | a mirror of the same OSS snapshot, but it adds stars/license/readme the official registry does not carry — so it enriches rather than defines |
//! | user registries / directories | 50+ | the user asked for them; they lose to official data on conflicts but can add servers nobody else lists |

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::mcp_models::McpCursorStyle;

/// Where a source's data physically comes from.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpSourceKind.ts")]
pub enum McpSourceKind {
    /// A `v0.1`-shaped REST registry (`GET /servers?limit=&cursor=`).
    #[default]
    Registry,
    /// A local JSON file holding `{"servers":[…]}` or a bare array.
    LocalDirectory,
}

/// Re-distribution licence of a source's data. Only a source whose terms
/// actually permit it may be mirrored into the local snapshot indefinitely.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpSourceLicense.ts")]
pub enum McpSourceLicense {
    /// CC0 1.0 Universal — the official registry's Terms of Service.
    Cc0,
    /// No published re-distribution terms.
    #[default]
    Unspecified,
    /// The user's own file; their data, their call.
    UserProvided,
}

/// One catalog source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpSourceDescriptor.ts")]
pub struct McpSourceDescriptor {
    /// Stable id. Built-ins use bare names (`official`, `github`); user
    /// sources are namespaced `custom:<slug>` so they can never shadow one.
    pub id: String,
    pub display_name: String,
    /// Absolute `…/servers` URL for [`McpSourceKind::Registry`], or an
    /// absolute file path for [`McpSourceKind::LocalDirectory`].
    pub base_url: String,
    pub kind: McpSourceKind,
    pub cursor_style: McpCursorStyle,
    /// Extra query string appended to every page request (no leading `&`).
    #[serde(default)]
    pub list_query: Option<String>,
    /// True when the source cannot be read without a user-supplied API key.
    /// We do not ship shared keys, so such sources stay off by default.
    pub requires_key: bool,
    pub license: McpSourceLicense,
    /// Whether the source's terms allow keeping a long-lived local mirror.
    pub mirrorable: bool,
    pub enabled: bool,
    pub builtin: bool,
    /// Merge authority; lower wins.
    pub priority: i32,
    /// Circuit breaker on pagination, so a misbehaving cursor cannot loop
    /// forever. Set per source rather than globally: the primary source has to
    /// be readable in full (truncating it leaves the catalog permanently
    /// degraded), while a rate-limited mirror is better cut short than
    /// hammered.
    pub max_pages: usize,
}

/// The official MCP Registry — primary source of truth.
pub const OFFICIAL_SOURCE_ID: &str = "official";
/// The GitHub MCP Registry — enrichment mirror (stars / license / readme).
pub const GITHUB_SOURCE_ID: &str = "github";
/// Prefix reserved for user-added sources.
pub const CUSTOM_SOURCE_PREFIX: &str = "custom:";

/// Built-in sources, in merge-authority order.
pub fn builtin_sources() -> Vec<McpSourceDescriptor> {
    vec![
        McpSourceDescriptor {
            id: OFFICIAL_SOURCE_ID.to_string(),
            display_name: "Official MCP Registry".to_string(),
            base_url: "https://registry.modelcontextprotocol.io/v0.1/servers".to_string(),
            kind: McpSourceKind::Registry,
            cursor_style: McpCursorStyle::Camel,
            // The registry publishes one row per *version*; asking for the
            // latest of each collapses the catalog to what a store shows and
            // keeps us well inside the page budget.
            list_query: Some("version=latest".to_string()),
            requires_key: false,
            license: McpSourceLicense::Cc0,
            mirrorable: true,
            enabled: true,
            builtin: true,
            priority: 0,
            // Measured 2026-08-13: >20k servers even with `version=latest`.
            // The cap is a runaway-cursor breaker, not a budget — truncating
            // the primary source leaves the catalog permanently degraded, so
            // it is set above the real size with headroom.
            max_pages: 400,
        },
        McpSourceDescriptor {
            id: GITHUB_SOURCE_ID.to_string(),
            display_name: "GitHub MCP Registry".to_string(),
            // `/v0` still answers 200 but sends `deprecation: true`.
            base_url: "https://api.mcp.github.com/v0.1/servers".to_string(),
            kind: McpSourceKind::Registry,
            cursor_style: McpCursorStyle::Snake,
            list_query: None,
            requires_key: false,
            license: McpSourceLicense::Unspecified,
            mirrorable: false,
            enabled: true,
            builtin: true,
            priority: 10,
            // 218 servers as measured; `x-ratelimit-limit: 10` means a long
            // walk here costs real backoff, and it is only an enrichment
            // mirror.
            max_pages: 50,
        },
    ]
}

impl McpSourceDescriptor {
    /// Host shown in sync diagnostics.
    pub fn source_host(&self) -> String {
        match self.kind {
            McpSourceKind::LocalDirectory => "file".to_string(),
            McpSourceKind::Registry => self
                .base_url
                .split("://")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .unwrap_or(&self.base_url)
                .to_string(),
        }
    }

    /// A source is usable when it is on and not blocked on a key we don't have.
    pub fn is_usable(&self) -> bool {
        self.enabled && !self.requires_key
    }
}

/// Resolve the effective source list: built-ins with user overrides applied,
/// followed by the user's own sources, sorted by merge authority.
pub fn resolve_sources() -> Vec<McpSourceDescriptor> {
    let config = super::config::load_config();
    let mut out: Vec<McpSourceDescriptor> = builtin_sources()
        .into_iter()
        .map(|mut source| {
            if config.disabled.iter().any(|id| id == &source.id) {
                source.enabled = false;
            }
            source
        })
        .collect();
    out.extend(config.custom.iter().map(|custom| custom.to_descriptor()));
    out.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    out
}

/// Only the sources a sync will actually read.
pub fn active_sources() -> Vec<McpSourceDescriptor> {
    resolve_sources()
        .into_iter()
        .filter(McpSourceDescriptor::is_usable)
        .collect()
}

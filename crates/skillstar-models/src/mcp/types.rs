//! Data types for the unified MCP store: server entries, patches, the store
//! root, and sync/status result shapes.
//!
//! The built-in preset catalog lives next door in `presets.rs`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

// ---------------------------------------------------------------------------
// Supported tools
// ---------------------------------------------------------------------------

/// Tool ids that can receive MCP servers, in display order.
///
/// Adding an id here is only half the contract — [`MCP_TOOL_SPECS`] must gain
/// the matching row (pinned by `registry::tests`), and the frontend mirrors
/// (`src/types/mcp.ts`, `lib/toolRegistry.ts`'s `MCP_TOOL_LABELS`,
/// `lib/agentTargets.ts`) must be updated in the same change.
///
/// [`MCP_TOOL_SPECS`]: super::registry
pub const MCP_TOOL_IDS: &[&str] = &[
    "claude-code",
    "claude-desktop-chat",
    "codex",
    "grok",
    "opencode",
    "zcode",
    "kiro",
    "cursor",
    "vscode",
    "windsurf",
    "cline",
    "gemini-cli",
    "zed",
    "maka",
];

/// Legacy Desktop Chat projection retained only so existing SkillStar-managed
/// entries can still be located and removed. It is not a public Agent target.
///
/// Claude Desktop's chat surface is a public target again under the distinct id
/// [`CLAUDE_DESKTOP_CHAT_TOOL_ID`], and both write the same
/// `claude_desktop_config.json` → `mcpServers.<name>` key. The two ids stay
/// separate on purpose: this tombstone authorizes exactly one *removal* of the
/// old projection, while the public id is a normal enable flag. When an entry
/// carries both, the tombstone is *subsumed* — consumed without deleting the
/// key the modern target now owns (see `sync::cleanup_legacy_desktop_chat`).
pub(crate) const LEGACY_CLAUDE_DESKTOP_TOOL_ID: &str = "claude-desktop";

/// Public Claude Desktop *chat* target. Shares
/// `<config_dir>/Claude/claude_desktop_config.json` with
/// [`LEGACY_CLAUDE_DESKTOP_TOOL_ID`]; see that constant for the subsumption
/// rule.
///
/// The id names the *surface*, not just the app, because Claude Desktop has
/// two of them and they read different files: its Code surface reads
/// `~/.claude.json` and is already served by `claude-code`, while its Chat
/// surface reads `claude_desktop_config.json` in a wholly different wire
/// format (no `type` key). A bare `claude-desktop` would both collide with the
/// tombstone and hide that split.
pub(crate) const CLAUDE_DESKTOP_CHAT_TOOL_ID: &str = "claude-desktop-chat";

/// Legacy Gemini CLI MCP projection retained only so existing SkillStar-managed
/// entries can still be located and removed after Gemini was dropped as a
/// public Agent target.
///
/// Gemini CLI is a public target again under the distinct id `gemini-cli`, and
/// both write `~/.gemini/settings.json` → `mcpServers.<name>`. The two ids stay
/// separate on purpose: the tombstone authorizes exactly one *removal* of the
/// old projection, while `gemini-cli` is a normal enable flag. When an entry
/// carries both, the tombstone is *subsumed* — it is consumed without deleting
/// the key the modern target now owns (see `sync::cleanup_legacy_gemini`).
pub(crate) const LEGACY_GEMINI_TOOL_ID: &str = "gemini";

/// Public Gemini CLI target. Shares `~/.gemini/settings.json` with
/// [`LEGACY_GEMINI_TOOL_ID`]; see that constant for the subsumption rule.
pub(crate) const GEMINI_CLI_TOOL_ID: &str = "gemini-cli";

/// Human-readable label for a tool id (registry-driven).
pub fn mcp_tool_label(tool_id: &str) -> &'static str {
    super::mcp_tool_spec(tool_id)
        .map(|spec| spec.label)
        .unwrap_or("Unknown")
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single MCP server in the unified store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpServerEntry.ts")]
pub struct McpServerEntry {
    #[serde(default)]
    pub id: String,
    /// Server key written verbatim into each tool's config.
    pub name: String,
    /// `"stdio"` (default), `"http"`, or `"sse"`.
    #[serde(default = "default_transport")]
    pub transport: String,

    // --- stdio fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    // --- http / sse fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,

    // --- metadata ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    // --- provenance fingerprint (audit B.1-a) -------------------------------
    // Every field below is `Option` + `#[serde(default)]` on purpose: stores
    // written by older releases carry none of them and must keep parsing
    // byte-for-byte (audit R1 #2). Nothing reads them for correctness — they
    // exist so update detection, deprecation notices, and exact
    // "already installed" matching stop relying on the server *name* string.
    /// Which registry the entry was installed from (e.g. `official`,
    /// `github`, or a curated source key). `None` = hand-created or imported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// The registry's own canonical `server.json` name — a reverse-DNS
    /// identifier such as `io.github.owner/repo`. Distinct from [`Self::name`],
    /// which is the sanitized key written into tool configs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_name: Option<String>,
    /// Server version recorded at install time, verbatim from the registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    /// Which runtime shape was chosen when installing — see
    /// [`McpRuntimeKind`] for the vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_kind: Option<String>,

    /// Per-tool enable flags, keyed by tool id (see [`MCP_TOOL_IDS`]). Existing
    /// stores may retain a legacy Desktop Chat cleanup tombstone: `true` means
    /// cleanup is pending; a successful removal consumes it to `false`.
    #[serde(default)]
    pub enabled: BTreeMap<String, bool>,

    // --- tool-call approval / exposure (best-effort per tool, see `specs.rs`) ---
    /// Auto-approve every tool call for this server without prompting ("YOLO").
    /// Maps to Kiro's `autoApprove: ["*"]`. There is no verified native
    /// equivalent for Claude Code, OpenCode, Grok, or ZCode, so this flag has
    /// no effect when projected to those tools.
    #[serde(default)]
    pub auto_approve_all: bool,
    /// Specific tool names to auto-approve without prompting (Kiro
    /// `autoApprove`). Ignored when `auto_approve_all` is true (which already
    /// approves everything).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_approve_tools: Vec<String>,
    /// Tool names to hide from the agent entirely (Kiro `disabledTools`, Codex
    /// `disabled_tools`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_tools: Vec<String>,
    /// Request/startup timeout in milliseconds (OpenCode `timeout`). Converted
    /// to whole seconds for Codex's
    /// `startup_timeout_sec` / `tool_timeout_sec`. `None`/`0` means "use the
    /// tool's own default".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    // ts-rs maps u64 -> bigint by default (technically correct, since u64 can
    // exceed Number.MAX_SAFE_INTEGER), but this value only ever crosses the
    // wire as JSON through serde_json + Tauri IPC + JS `JSON.parse`, none of
    // which round-trip real bigint — they all produce a plain JS `number`.
    // Millisecond timeouts never approach 2^53, so `number` is both accurate
    // and what every consumer actually receives.
    #[ts(type = "number")]
    pub timeout_ms: Option<u64>,

    #[serde(default)]
    pub sort_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    // Same bigint-vs-number rationale as `timeout_ms` above: epoch
    // milliseconds today are ~1.7e12, far under 2^53.
    #[ts(type = "number")]
    pub created_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub updated_at: Option<u64>,
}

fn default_transport() -> String {
    "stdio".to_string()
}

/// Vocabulary for [`McpServerEntry::runtime_kind`] — the runtime shape picked
/// out of a registry entry's `remotes[]` / `packages[]` at install time.
///
/// Stored as a plain string so a future registry package type never makes an
/// existing store unparseable; this type is the *writer* side vocabulary and
/// the parse side is deliberately lossy ([`McpRuntimeKind::from_stored`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpRuntimeKind {
    /// `remotes[]` with `type: "streamable-http"` — the preferred shape.
    RemoteStreamableHttp,
    /// `remotes[]` with `type: "sse"` — usable, but the transport is deprecated.
    RemoteSse,
    /// `packages[]` with `registryType: "oci"` — container-isolated local run.
    PackageOci,
    /// `packages[]` with `registryType: "mcpb"` — prebuilt bundle.
    PackageMcpb,
    /// `packages[]` on a language registry (npm / pypi / nuget / cargo).
    PackagePlain,
    /// Typed in by hand or imported from a tool's existing config.
    Manual,
}

impl McpRuntimeKind {
    /// Stable on-disk token. Never renamed — old stores keep resolving.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RemoteStreamableHttp => "remote-streamable-http",
            Self::RemoteSse => "remote-sse",
            Self::PackageOci => "package-oci",
            Self::PackageMcpb => "package-mcpb",
            Self::PackagePlain => "package-plain",
            Self::Manual => "manual",
        }
    }

    /// Parse a stored token. Unknown tokens (written by a newer release)
    /// yield `None` rather than an error — provenance is advisory metadata and
    /// must never make a store unreadable.
    pub fn from_stored(raw: &str) -> Option<Self> {
        match raw {
            "remote-streamable-http" => Some(Self::RemoteStreamableHttp),
            "remote-sse" => Some(Self::RemoteSse),
            "package-oci" => Some(Self::PackageOci),
            "package-mcpb" => Some(Self::PackageMcpb),
            "package-plain" => Some(Self::PackagePlain),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// Partial update patch for an MCP server. Only `Some` fields are applied.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpServerPatch.ts")]
pub struct McpServerPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_approve_all: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_approve_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_tools: Option<Vec<String>>,
    /// `Some(None)` clears the timeout back to "use tool default"; `None`
    /// leaves it untouched. Represented as `Option<Option<u64>>` so the
    /// three-state patch semantics survive JSON round-tripping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    // Same bigint-vs-number rationale as McpServerEntry's timestamp fields;
    // additionally overrides the whole type (rather than just the inner u64)
    // because ts-rs's plain derive on nested Option<Option<u64>> produced a
    // redundant `bigint | null | null` — see the pilot report for the before/
    // after empirical diff.
    #[ts(type = "number | null")]
    pub timeout_ms: Option<Option<u64>>,
}

/// Schema version this build reads and writes for `mcp_servers.json`.
///
/// Bump only when a field's *meaning* changes; purely additive
/// `#[serde(default)]` fields (the provenance fingerprint, for instance) keep
/// old files parseable and therefore keep the version pinned. The read gate in
/// `store::read_mcp_store` uses this constant in both directions:
///
/// - a file at a **higher** version is refused (fail closed) rather than
///   round-tripped through this build's serializer, which would silently drop
///   whatever the newer release added;
/// - a file at a **lower** version is upgraded in place and written straight
///   back, so the on-disk version never lags the code that owns it.
pub const MCP_STORE_VERSION: u32 = 1;

/// Root structure stored in `mcp_servers.json`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "McpStore.ts")]
pub struct McpStore {
    pub version: u32,
    #[serde(default)]
    pub servers: Vec<McpServerEntry>,
}

impl Default for McpStore {
    fn default() -> Self {
        Self {
            version: MCP_STORE_VERSION,
            servers: Vec::new(),
        }
    }
}

/// Result of projecting one server into one tool's live config.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpSyncResult.ts")]
pub struct McpSyncResult {
    pub tool_id: String,
    pub server_id: String,
    pub success: bool,
    /// True when the action was a deliberate no-op: the tool is not installed,
    /// or a legacy cleanup tombstone was subsumed by the live target that now
    /// owns the same config key.
    #[serde(default)]
    pub skipped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// A failed write was undone: this tool's config is byte-for-byte back to
    /// its pre-attempt state (restored from `backup_path`, or the file this
    /// attempt created was deleted). Only ever set alongside `success: false`.
    #[serde(default)]
    pub rolled_back: bool,
    /// Set when `success` is false **and** the undo itself failed — the one
    /// state where this tool's config may have drifted. Surfaced separately
    /// from `error` because it demands a different remedy: restore the backup
    /// by hand rather than retry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_error: Option<String>,
}

impl McpSyncResult {
    /// A result that neither wrote nor rolled anything back.
    pub(crate) fn pending(tool_id: &str, server_id: &str) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            server_id: server_id.to_string(),
            success: false,
            skipped: false,
            config_path: None,
            backup_path: None,
            error: None,
            rolled_back: false,
            rollback_error: None,
        }
    }

    /// True when this tool's live config is known to match the store: the
    /// write landed, or it was skipped, or the failed write was fully undone
    /// back to a state the store still describes.
    pub fn is_settled(&self) -> bool {
        self.success || self.skipped || (self.rolled_back && self.rollback_error.is_none())
    }
}

/// Where a batch of per-tool projections left the user's machine.
///
/// `sync_server_public_tools` writes tools one at a time and each write is
/// individually atomic (a failure restores that tool's file), but the batch as
/// a whole is not transactional: tool 3 failing does not un-write tools 1 and
/// 2, because those writes are correct and undoing them would take working
/// configs away for an unrelated tool's problem. This type is how a caller
/// learns that, instead of having to re-derive it from a `Vec`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpSyncConsistency.ts")]
pub struct McpSyncConsistency {
    /// Every tool is settled — no drift anywhere.
    pub consistent: bool,
    /// Tools whose config now matches the store (written or skipped).
    pub applied: Vec<String>,
    /// Tools that failed and were rolled back to their previous content.
    /// The store still describes intent these tools have not received.
    pub rolled_back: Vec<String>,
    /// Tools that failed **and** could not be rolled back. Their config may
    /// be half-written; the `backup_path` on the matching result is the
    /// recovery handle.
    pub drifted: Vec<String>,
}

/// Summarize a batch of per-tool results into a consistency verdict.
///
/// Additive on purpose — callers that only want the raw `Vec<McpSyncResult>`
/// (every existing Tauri command) are untouched.
pub fn mcp_sync_consistency(results: &[McpSyncResult]) -> McpSyncConsistency {
    let mut applied = Vec::new();
    let mut rolled_back = Vec::new();
    let mut drifted = Vec::new();
    for result in results {
        if result.success || result.skipped {
            applied.push(result.tool_id.clone());
        } else if result.rolled_back && result.rollback_error.is_none() {
            rolled_back.push(result.tool_id.clone());
        } else {
            drifted.push(result.tool_id.clone());
        }
    }
    McpSyncConsistency {
        consistent: rolled_back.is_empty() && drifted.is_empty(),
        applied,
        rolled_back,
        drifted,
    }
}

/// Installed/probe status for one tool's MCP config target.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpToolStatus.ts")]
pub struct McpToolStatus {
    pub tool_id: String,
    pub label: String,
    pub config_path: String,
    pub installed: bool,
    /// Number of MCP servers currently present in the live config file.
    pub server_count: usize,
}

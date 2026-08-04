//! mcp domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.
//!
//! All types below are generated via ts-rs — see `src/types/generated/` and
//! `bun run types:gen`. Do not hand-edit those shapes here; edit the Rust
//! struct and regenerate instead.
//! - McpServerEntry, McpServerPatch, McpStore, McpSyncResult, McpToolStatus,
//!   McpPreset come from `skillstar_models::mcp::types`.
//! - McpPublisherSummary, McpServerKind, McpRegistryPackageSummary,
//!   McpRegistryRemoteSummary, McpMarketEntry, McpMarketServerDetail come
//!   from `skillstar_marketplace::mcp_models`.
//! - McpServerWithSync comes from
//!   `skillstar` (src-tauri) `commands::mcp_commands`.

export type { McpServerEntry } from "./generated/McpServerEntry";
export type { McpServerPatch } from "./generated/McpServerPatch";
export type { McpStore } from "./generated/McpStore";
export type { McpSyncResult } from "./generated/McpSyncResult";
export type { McpToolStatus } from "./generated/McpToolStatus";
export type { McpPreset } from "./generated/McpPreset";

export type { McpPublisherSummary } from "./generated/McpPublisherSummary";
export type { McpServerKind } from "./generated/McpServerKind";
export type { McpRegistryPackageSummary } from "./generated/McpRegistryPackageSummary";
export type { McpRegistryRemoteSummary } from "./generated/McpRegistryRemoteSummary";
export type { McpMarketEntry } from "./generated/McpMarketEntry";
export type { McpMarketServerDetail } from "./generated/McpMarketServerDetail";

export type { McpServerWithSync } from "./generated/McpServerWithSync";

/** Sub-page navigation for drill-down views */

export type McpTransport = "stdio" | "http" | "sse";

/** Tool ids that can receive MCP servers (matches `MCP_TOOL_IDS`). */

export const MCP_TOOL_IDS = ["claude-code", "codex", "grok", "opencode", "zcode", "kiro", "cursor"] as const;

export type McpToolId = (typeof MCP_TOOL_IDS)[number];

const MCP_TOOL_ID_SET: ReadonlySet<string> = new Set(MCP_TOOL_IDS);

/** Narrow raw backend/cache strings to the public MCP Agent target contract. */
export function isMcpToolId(value: string): value is McpToolId {
  return MCP_TOOL_ID_SET.has(value);
}

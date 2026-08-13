//! mcp domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.
//!
//! All types below are generated via ts-rs — see `src/types/generated/` and
//! `bun run types:gen`. Do not hand-edit those shapes here; edit the Rust
//! struct and regenerate instead.
//! - McpServerEntry, McpServerPatch, McpStore, McpSyncResult,
//!   McpSyncConsistency, McpToolStatus, McpPreset, McpProbeReport,
//!   McpProbeStatus, McpSpecEpoch come from `skillstar_models::mcp`.
//! - McpPublisherSummary, McpServerKind, McpServerStatus, McpIcon,
//!   McpTransportSpec, McpRegistryPackageSummary, McpRegistryRemoteSummary,
//!   McpMarketEntry, McpMarketServerDetail, the `Input` family (McpInput,
//!   McpInputVariable, McpInputFormat, McpKeyValueInput, McpArgument,
//!   McpArgumentKind), the source registry (McpSourceDescriptor, McpSourceKind,
//!   McpSourceLicense, McpCustomSource, McpSourcesConfig, McpCursorStyle) and
//!   the card query (McpServerQuery, McpSortKey, McpServerPage) come from
//!   `skillstar_marketplace::{mcp_models, mcp_remote, mcp_snapshot}`.
//! - McpRuntimeCandidate, McpRuntimeSelection, McpRuntimeShape, McpInstallPlan,
//!   McpInstallInput, McpInstallInputScope, McpSecretPolicy, McpSecretStorage
//!   come from `skillstar_app::mcp` (the cross-domain use cases).
//! - McpServerWithSync comes from
//!   `skillstar` (src-tauri) `commands::mcp_commands`.

export type { McpServerEntry } from "./generated/McpServerEntry";
export type { McpServerPatch } from "./generated/McpServerPatch";
export type { McpStore } from "./generated/McpStore";
export type { McpSyncResult } from "./generated/McpSyncResult";
export type { McpSyncConsistency } from "./generated/McpSyncConsistency";
export type { McpToolStatus } from "./generated/McpToolStatus";
export type { McpPreset } from "./generated/McpPreset";
export type { McpProbeReport } from "./generated/McpProbeReport";
export type { McpProbeStatus } from "./generated/McpProbeStatus";
export type { McpSpecEpoch } from "./generated/McpSpecEpoch";

export type { McpPublisherSummary } from "./generated/McpPublisherSummary";
export type { McpServerKind } from "./generated/McpServerKind";
export type { McpServerStatus } from "./generated/McpServerStatus";
export type { McpIcon } from "./generated/McpIcon";
export type { McpTransportSpec } from "./generated/McpTransportSpec";
export type { McpRegistryPackageSummary } from "./generated/McpRegistryPackageSummary";
export type { McpRegistryRemoteSummary } from "./generated/McpRegistryRemoteSummary";
export type { McpMarketEntry } from "./generated/McpMarketEntry";
export type { McpMarketServerDetail } from "./generated/McpMarketServerDetail";

export type { McpInput } from "./generated/McpInput";
export type { McpInputVariable } from "./generated/McpInputVariable";
export type { McpInputFormat } from "./generated/McpInputFormat";
export type { McpKeyValueInput } from "./generated/McpKeyValueInput";
export type { McpArgument } from "./generated/McpArgument";
export type { McpArgumentKind } from "./generated/McpArgumentKind";

export type { McpSourceDescriptor } from "./generated/McpSourceDescriptor";
export type { McpSourceKind } from "./generated/McpSourceKind";
export type { McpSourceLicense } from "./generated/McpSourceLicense";
export type { McpCustomSource } from "./generated/McpCustomSource";
export type { McpSourcesConfig } from "./generated/McpSourcesConfig";
export type { McpCursorStyle } from "./generated/McpCursorStyle";

export type { McpServerQuery } from "./generated/McpServerQuery";
export type { McpSortKey } from "./generated/McpSortKey";
export type { McpServerPage } from "./generated/McpServerPage";

export type { McpRuntimeCandidate } from "./generated/McpRuntimeCandidate";
export type { McpRuntimeSelection } from "./generated/McpRuntimeSelection";
export type { McpRuntimeShape } from "./generated/McpRuntimeShape";
export type { McpInstallPlan } from "./generated/McpInstallPlan";
export type { McpInstallInput } from "./generated/McpInstallInput";
export type { McpInstallInputScope } from "./generated/McpInstallInputScope";
export type { McpSecretPolicy } from "./generated/McpSecretPolicy";
export type { McpSecretStorage } from "./generated/McpSecretStorage";

export type { McpServerWithSync } from "./generated/McpServerWithSync";

/** Sub-page navigation for drill-down views */

export type McpTransport = "stdio" | "http" | "sse";

/**
 * Tool ids that can receive MCP servers, in display order.
 *
 * Hand-maintained mirror of `MCP_TOOL_IDS` in
 * `crates/skillstar-models/src/mcp/types.rs`. That constant is the SSOT; this
 * list plus `lib/toolRegistry.ts`'s `MCP_TOOL_LABELS` and
 * `lib/agentTargets.ts`'s `MCP_TOOL_BY_AGENT_ID` are the three frontend copies
 * that must be updated in the same change.
 *
 * `claude-desktop-chat` (not `claude-desktop`) is deliberate: `claude-desktop`
 * is a legacy cleanup tombstone, not a target — see that Rust constant.
 */

export const MCP_TOOL_IDS = [
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
] as const;

export type McpToolId = (typeof MCP_TOOL_IDS)[number];

const MCP_TOOL_ID_SET: ReadonlySet<string> = new Set(MCP_TOOL_IDS);

/** Narrow raw backend/cache strings to the public MCP Agent target contract. */
export function isMcpToolId(value: string): value is McpToolId {
  return MCP_TOOL_ID_SET.has(value);
}

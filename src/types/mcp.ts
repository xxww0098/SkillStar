//! mcp domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.
//!
//! The unified-store types below (McpServerEntry, McpServerPatch, McpStore,
//! McpSyncResult, McpToolStatus, McpPreset) are generated from
//! `skillstar_models::mcp::types` via ts-rs — see `src/types/generated/` and
//! `bun run types:gen`. Do not hand-edit those shapes here; edit the Rust
//! struct and regenerate instead. Marketplace types (McpMarketEntry & co.)
//! below are still hand-mirrored from `skillstar_marketplace::mcp_models` —
//! planned for a follow-up ts-rs pass.

import type { McpServerEntry } from "./generated/McpServerEntry";
import type { McpSyncResult } from "./generated/McpSyncResult";

export type { McpServerEntry } from "./generated/McpServerEntry";
export type { McpServerPatch } from "./generated/McpServerPatch";
export type { McpStore } from "./generated/McpStore";
export type { McpSyncResult } from "./generated/McpSyncResult";
export type { McpToolStatus } from "./generated/McpToolStatus";
export type { McpPreset } from "./generated/McpPreset";

export interface McpPublisherSummary {
  /** Publisher id — also the curated `source` value, or `"github"`. */
  id: string;
  /** Display name (e.g. "AdsPower", "BigModel", "GitHub"). */
  name: string;
  /** Number of MCP servers offered by this publisher. */
  server_count: number;
  /** External landing page (docs / repo). */
  url: string;
}

/** Sub-page navigation for drill-down views */

export type McpTransport = "stdio" | "http" | "sse";

/** Tool ids that can receive MCP servers (matches `MCP_TOOL_IDS`). */

export const MCP_TOOL_IDS = [
  "claude-code",
  "claude-desktop",
  "codex",
  "gemini",
  "grok",
  "opencode",
  "zcode",
  "kiro",
  "cursor",
] as const;

export type McpToolId = (typeof MCP_TOOL_IDS)[number];

export interface McpServerWithSync {
  server: McpServerEntry;
  syncResults: McpSyncResult[];
}

// --- MCP marketplace (GitHub MCP Registry) — mirrors skillstar_marketplace::mcp_models ---

export type McpServerKind = "stdio" | "remote" | "both" | "unknown";

export interface McpRegistryPackageSummary {
  /** Runner command: npx / uvx / docker / dnx / … */
  runtime: string;
  identifier: string;
  version?: string | null;
  /** Env var names the user must supply (required or secret). */
  requiredEnv: string[];
}

export interface McpRegistryRemoteSummary {
  /** Normalized transport: "http" | "sse". */
  transport: string;
  url: string;
  requiredHeaders: string[];
}

/** Card model for the MCP marketplace list/search. */

export interface McpMarketEntry {
  id: string;
  /** Cleaned display name (last path segment of `namespace`). */
  name: string;
  /** Full registry name, e.g. "io.github.netdata/mcp-server". */
  namespace: string;
  description: string;
  repoUrl: string;
  stars: number;
  license?: string | null;
  version?: string | null;
  kind: McpServerKind;
  /** Distinct runner hints across packages, e.g. ["uvx"], ["npx"]. */
  runtimes: string[];
  updatedAt?: string | null;
  /** SkillStar-curated recommendation shown ahead of remote registry rows. */
  recommended?: boolean;
  source?: string | null;
}

/** Detail model: card fields + readme + package/remote display. */

export interface McpMarketServerDetail extends McpMarketEntry {
  readme?: string | null;
  packages: McpRegistryPackageSummary[];
  remotes: McpRegistryRemoteSummary[];
}

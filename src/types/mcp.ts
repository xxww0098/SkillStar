//! mcp domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

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

export const MCP_TOOL_IDS = ["claude-code", "claude-desktop", "codex", "gemini", "opencode", "zcode"] as const;

export type McpToolId = (typeof MCP_TOOL_IDS)[number];

export interface McpServerEntry {
  id: string;
  /** Server key written verbatim into each tool's config. */
  name: string;
  transport: McpTransport | string;
  // stdio
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  // http / sse
  url?: string;
  headers?: Record<string, string>;
  // metadata
  description?: string;
  homepage?: string;
  tags?: string[];
  /** Per-tool enable flags, keyed by tool id. */
  enabled: Record<string, boolean>;
  sortIndex: number;
  createdAt?: number;
  updatedAt?: number;
}

/** Partial update — only present fields are applied. */

export interface McpServerPatch {
  name?: string;
  transport?: string;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  url?: string;
  headers?: Record<string, string>;
  description?: string;
  homepage?: string;
  tags?: string[];
}

export interface McpStore {
  version: number;
  servers: McpServerEntry[];
}

export interface McpSyncResult {
  toolId: string;
  serverId: string;
  success: boolean;
  skipped?: boolean;
  configPath?: string;
  backupPath?: string;
  error?: string;
}

export interface McpToolStatus {
  toolId: string;
  label: string;
  configPath: string;
  installed: boolean;
  /** Number of MCP servers currently present in the live config file. */
  serverCount: number;
}

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

/** A built-in / recommended-to-install MCP server template (mirrors `skillstar_models::mcp::McpPreset`). */

export interface McpPreset {
  id: string;
  /** Server key written verbatim into each tool's config (and the entry name). */
  name: string;
  description: string;
  homepage: string;
  transport: McpTransport | string;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
  tags?: string[];
  /** Env keys the user must fill in before the server works (e.g. ["API_KEY"]). */
  requiredEnv?: string[];
}

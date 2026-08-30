import type {
  LocalFirstResult,
  McpCustomSource,
  McpInstallPlan,
  McpMarketServerDetail,
  McpPublisherSummary,
  McpRuntimeSelection,
  McpServerPage,
  McpServerQuery,
  McpSourceDescriptor,
  SyncStateEntry,
} from "../../../types";

/**
 * MCP marketplace — browse the merged MCP catalog local-first, then install via
 * `mcp_market_install_plan`, whose plan carries the prefilled `McpServerEntry`
 * draft the form submits through the existing `create_mcp_server` command.
 *
 * The catalog is the merge of every enabled source (the official MCP Registry
 * as primary, GitHub's registry as an enrichment mirror, plus any user-added
 * registry URL or local JSON directory) — see `list_mcp_sources` and friends.
 *
 * Backed by `skillstar_marketplace::mcp_snapshot` (cache + FTS) and
 * `skillstar_app::mcp` (runtime selection + install plan) via
 * `src-tauri/src/commands/mcp_marketplace.rs`.
 */
export interface McpMarketplaceCommands {
  /**
   * Filtered / sorted / paginated card query. Every field of `McpServerQuery`
   * has a Rust-side default, so pass only the ones you set. The result carries
   * `total` alongside the page, so "showing 60 of 21363" needs no second call.
   */
  query_mcp_market_servers_local: {
    args: { query: Partial<McpServerQuery> };
    result: LocalFirstResult<McpServerPage>;
  };
  list_mcp_publishers_local: {
    args: Record<string, never>;
    result: McpPublisherSummary[];
  };
  get_mcp_market_server_detail_local: {
    args: { id: string };
    result: LocalFirstResult<McpMarketServerDetail | null>;
  };
  sync_mcp_market_scope: { args: { scope: string }; result: void };
  /**
   * One row per source (`mcp_registry:<sourceId>`), each with its own
   * `lastError` and `degradedReason` — this is what lets the UI say "this sync
   * was incomplete, because X" instead of reporting a partial catalog as whole.
   */
  get_mcp_source_sync_states: { args: Record<string, never>; result: SyncStateEntry[] };

  /** Every configured source: built-ins with user overrides, then user sources. */
  list_mcp_sources: { args: Record<string, never>; result: McpSourceDescriptor[] };
  /** Add (or replace) a user registry URL / local JSON directory file. */
  add_mcp_source: { args: { source: McpCustomSource }; result: McpSourceDescriptor[] };
  remove_mcp_source: { args: { id: string }; result: McpSourceDescriptor[] };
  /** Turn any source on/off — built-in ids included. */
  set_mcp_source_enabled: { args: { id: string; enabled: boolean }; result: McpSourceDescriptor[] };

  /**
   * Every runtime shape the server publishes, ranked against this machine
   * (remote streamable-http → sse → oci → mcpb → npm/pypi/nuget/cargo, with
   * unavailable toolchains demoted), plus the recommended pick.
   */
  mcp_market_runtime_candidates: { args: { id: string }; result: McpRuntimeSelection };
  /**
   * Pre-install confirmation payload: the complete untruncated command that
   * will run, the binary it resolves to, the runtime alternatives, and every
   * input the form must collect with full `server.json` semantics.
   */
  mcp_market_install_plan: {
    args: { id: string; runtimeId?: string };
    result: McpInstallPlan;
  };
}

import type {
  LocalFirstResult,
  McpMarketEntry,
  McpMarketServerDetail,
  McpPublisherSummary,
  McpServerEntry,
  SyncStateEntry,
} from "../../../types";

/**
 * MCP marketplace — browse the GitHub MCP Registry
 * (`https://api.mcp.github.com/v0/servers`) local-first, then install by
 * converting an entry into a prefilled `McpServerEntry` draft and submitting it
 * via the existing `create_mcp_server` command.
 *
 * Backed by `skillstar_marketplace::mcp_snapshot` (cache + FTS) via
 * `skillstar_app::commands::mcp_marketplace`.
 */
export interface McpMarketplaceCommands {
  list_mcp_market_servers_local: {
    args: Record<string, never>;
    result: LocalFirstResult<McpMarketEntry[]>;
  };
  list_mcp_publishers_local: {
    args: Record<string, never>;
    result: McpPublisherSummary[];
  };
  list_mcp_servers_by_publisher_local: {
    args: { publisherId: string };
    result: LocalFirstResult<McpMarketEntry[]>;
  };
  search_mcp_market_local: {
    args: { query: string; limit?: number };
    result: LocalFirstResult<McpMarketEntry[]>;
  };
  get_mcp_market_server_detail_local: {
    args: { id: string };
    result: LocalFirstResult<McpMarketServerDetail | null>;
  };
  sync_mcp_market_scope: { args: { scope: string }; result: void };
  get_mcp_market_sync_states: { args: Record<string, never>; result: SyncStateEntry[] };
  /** Convert a marketplace entry into a prefilled draft for the create form. */
  mcp_market_entry_to_draft: { args: { id: string }; result: McpServerEntry };
}

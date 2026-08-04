/**
 * Dev-mock fragment: MCP — managed servers, tool sync statuses, presets, and
 * the MCP marketplace (GitHub MCP Registry). Sample data lives in
 * ./mcpData.ts.
 */

import { MCP_MARKET, MCP_MARKET_DETAILS, MCP_PRESETS, MCP_STORE, MCP_TOOL_STATUSES, mcpMarketDraft } from "./mcpData";
import { type DevMockHandlers, iso } from "./shared";

export const MCP_HANDLERS: DevMockHandlers = {
  list_mcp_servers: () => MCP_STORE,
  mcp_tool_statuses: () => MCP_TOOL_STATUSES,
  get_mcp_presets: () => MCP_PRESETS,

  // MCP marketplace (GitHub MCP Registry)
  list_mcp_market_servers_local: () => ({
    data: MCP_MARKET,
    snapshot_status: "fresh",
    snapshot_updated_at: iso(0),
  }),
  search_mcp_market_local: (args) => {
    const q = String((args?.query as string) ?? "").toLowerCase();
    const data = q
      ? MCP_MARKET.filter((m) => m.name.toLowerCase().includes(q) || m.description.toLowerCase().includes(q))
      : MCP_MARKET;
    return { data, snapshot_status: "fresh", snapshot_updated_at: iso(0) };
  },
  get_mcp_market_server_detail_local: (args) => {
    const id = String((args?.id as string) ?? "");
    const entry = MCP_MARKET.find((m) => m.id === id);
    const detail = MCP_MARKET_DETAILS[id];
    return {
      data: entry && detail ? { ...entry, ...detail } : null,
      snapshot_status: "fresh",
      snapshot_updated_at: iso(0),
    };
  },
  list_mcp_servers_by_publisher_local: (args) => {
    const publisherId = String((args?.publisherId as string) ?? "").toLowerCase();
    const data =
      publisherId === "github" || publisherId === ""
        ? MCP_MARKET
        : MCP_MARKET.filter((m) => (m.source ?? "").toLowerCase() === publisherId);
    return {
      data,
      snapshot_status: "fresh",
      snapshot_updated_at: iso(0),
    };
  },
  sync_mcp_market_scope: () => undefined,
  get_mcp_market_sync_states: () => [
    {
      scope: "mcp_registry",
      last_success_at: iso(0),
      last_attempt_at: iso(0),
      last_error: null,
      next_refresh_at: iso(-0.5),
      schema_version: 8,
    },
  ],
  mcp_market_entry_to_draft: (args) => mcpMarketDraft(String((args?.id as string) ?? "")),
};

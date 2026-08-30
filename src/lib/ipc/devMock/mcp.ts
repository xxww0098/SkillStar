/**
 * Dev-mock fragment: MCP — managed servers, tool sync statuses, presets, health
 * probes, catalog sources, and the MCP marketplace. Sample data lives in
 * ./mcpData.ts.
 */

import {
  MCP_MARKET,
  MCP_MARKET_DETAILS,
  MCP_PRESETS,
  MCP_SOURCE_SYNC_STATES,
  MCP_SOURCES,
  MCP_STORE,
  MCP_TOOL_STATUSES,
  mcpInstallPlan,
  mcpMarketPage,
  mcpProbeReport,
  mcpRuntimeSelection,
} from "./mcpData";
import { type DevMockHandlers, iso } from "./shared";

const arg = (args: Record<string, unknown> | undefined, key: string) => String((args?.[key] as string) ?? "");

/** Sources are per-session in browser dev — mutated by the CRUD handlers. */
let sources = [...MCP_SOURCES];

export const MCP_HANDLERS: DevMockHandlers = {
  list_mcp_servers: () => MCP_STORE,
  mcp_tool_statuses: () => MCP_TOOL_STATUSES,
  get_mcp_presets: () => MCP_PRESETS,
  probe_mcp_server: (args) => mcpProbeReport(arg(args, "id")),

  // MCP marketplace
  query_mcp_market_servers_local: (args) => ({
    data: mcpMarketPage((args?.query as Record<string, unknown>) ?? {}),
    snapshot_status: "fresh",
    snapshot_updated_at: iso(0),
  }),
  get_mcp_market_server_detail_local: (args) => {
    const id = arg(args, "id");
    const entry = MCP_MARKET.find((m) => m.id === id);
    const detail = MCP_MARKET_DETAILS[id];
    return {
      data: entry && detail ? { ...entry, ...detail } : null,
      snapshot_status: "fresh",
      snapshot_updated_at: iso(0),
    };
  },
  sync_mcp_market_scope: () => undefined,
  get_mcp_source_sync_states: () => MCP_SOURCE_SYNC_STATES,

  // Catalog sources
  list_mcp_sources: () => sources,
  add_mcp_source: (args) => {
    const source = (args?.source as Record<string, unknown>) ?? {};
    const id = `custom:${String(source.id ?? "custom")}`;
    sources = [
      ...sources.filter((s) => s.id !== id),
      {
        ...(sources.find((s) => s.id === id) ?? sources[2]),
        ...source,
        id,
        builtin: false,
        license: "userProvided",
      },
    ];
    return sources;
  },
  remove_mcp_source: (args) => {
    const id = arg(args, "id");
    const full = id.startsWith("custom:") ? id : `custom:${id}`;
    sources = sources.filter((s) => s.id !== full);
    return sources;
  },
  set_mcp_source_enabled: (args) => {
    const id = arg(args, "id");
    const enabled = Boolean(args?.enabled);
    sources = sources.map((s) => (s.id === id ? { ...s, enabled } : s));
    return sources;
  },

  // Install path
  mcp_market_runtime_candidates: (args) => mcpRuntimeSelection(arg(args, "id")),
  mcp_market_install_plan: (args) => mcpInstallPlan(arg(args, "id"), args?.runtimeId as string | undefined),
};

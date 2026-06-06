import type {
  McpPreset,
  McpServerEntry,
  McpServerPatch,
  McpServerWithSync,
  McpStore,
  McpSyncResult,
  McpToolStatus,
} from "../../../types";

/**
 * MCP mode: unified MCP server store + projection into each agent tool's
 * native config. Backed by `skillstar_models::mcp` via `mcp_commands.rs`.
 */
export interface McpCommands {
  list_mcp_servers: { args: Record<string, never>; result: McpStore };
  mcp_tool_statuses: { args: Record<string, never>; result: McpToolStatus[] };
  create_mcp_server: { args: { entry: Partial<McpServerEntry> }; result: McpServerWithSync };
  update_mcp_server: { args: { id: string; patch: McpServerPatch }; result: McpServerWithSync };
  delete_mcp_server: { args: { id: string }; result: McpSyncResult[] };
  set_mcp_tool_enabled: {
    args: { id: string; toolId: string; enabled: boolean };
    result: McpSyncResult;
  };
  sync_mcp_server: { args: { id: string; force: boolean }; result: McpSyncResult[] };
  sync_all_mcp: { args: { force: boolean }; result: McpSyncResult[] };
  import_mcp_from_tool: { args: { toolId: string }; result: number };
  reorder_mcp_servers: { args: { orderedIds: string[] }; result: void };
  get_mcp_presets: { args: Record<string, never>; result: McpPreset[] };
}

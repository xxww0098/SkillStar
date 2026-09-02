import type {
  McpPasteParse,
  McpPreset,
  McpProbeReport,
  McpServerEntry,
  McpServerPatch,
  McpServerWithSync,
  McpStore,
  McpSyncResult,
  McpToolStatus,
  McpToolId,
} from "../../../types";

/**
 * MCP mode: unified MCP server store + projection into each agent tool's
 * native config. Backed by `skillstar_models::mcp` via `mcp_commands.rs`.
 */
export interface McpCommands {
  list_mcp_servers: { args: Record<string, never>; result: McpStore };
  /**
   * Per-tool MCP config target: is the tool installed, where is its config
   * file, and how many servers does it currently hold.
   */
  mcp_tool_statuses: { args: Record<string, never>; result: McpToolStatus[] };
  /**
   * Health-check one installed server. Dual-epoch: modern servers answer
   * `server/discover`, legacy ones `initialize`; both end at `tools/list`.
   *
   * Never rejects for an unhealthy server — read `status`. In particular
   * `authorization-required` (a remote server answering 401 with a
   * `WWW-Authenticate` challenge) is a correct response asking for OAuth, not
   * a failure, and must not render as one.
   */
  probe_mcp_server: { args: { id: string }; result: McpProbeReport };
  /**
   * Parse a pasted snippet or `skillstar://mcp` URL into drafts. Never writes
   * the store — catalog hits still open the install wizard, everything else
   * still opens the create form.
   */
  parse_mcp_paste: { args: { text: string }; result: McpPasteParse };
  create_mcp_server: { args: { entry: Partial<McpServerEntry> }; result: McpServerWithSync };
  update_mcp_server: { args: { id: string; patch: McpServerPatch }; result: McpServerWithSync };
  delete_mcp_server: { args: { id: string }; result: McpSyncResult[] };
  set_mcp_tool_enabled: {
    args: { id: string; toolId: McpToolId; enabled: boolean };
    result: McpSyncResult;
  };
  sync_mcp_server: { args: { id: string; force: boolean }; result: McpSyncResult[] };
  sync_all_mcp: { args: { force: boolean }; result: McpSyncResult[] };
  import_mcp_from_tool: { args: { toolId: McpToolId }; result: number };
  reorder_mcp_servers: { args: { orderedIds: string[] }; result: void };
  get_mcp_presets: { args: Record<string, never>; result: McpPreset[] };
}

/** Query-key factory for the MCP feature. Every TanStack Query key across the
 * unified server store, the local-first marketplace, publishers, and presets
 * must come from here so invalidation stays consistent. */
export const mcpKeys = {
  all: ["mcp"] as const,

  // Unified MCP server store (`useMcpServers`).
  servers: () => ["mcp-servers"] as const,
  toolStatuses: () => ["mcp-tool-statuses"] as const,

  // Local-first GitHub MCP registry browse (`useMcpMarketplace`,
  // `McpMarketBrowser`).
  market: () => ["mcp-market"] as const,
  marketList: () => [...mcpKeys.market(), "list"] as const,
  marketSearch: (query: string) => [...mcpKeys.market(), "search", query] as const,
  marketDetail: (id: string | null) => [...mcpKeys.market(), "detail", id] as const,

  // Curated publishers grid (`useMcpPublishers`).
  publishers: () => ["mcp-publishers"] as const,

  // Built-in presets (`useMcpPresets`).
  presets: () => ["mcp-presets"] as const,
};

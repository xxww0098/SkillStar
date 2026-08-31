/** Query-key factory for the MCP feature. Every TanStack Query key across the
 * unified server store, the local-first marketplace, publishers, and presets
 * must come from here so invalidation stays consistent. */
export const mcpKeys = {
  all: ["mcp"] as const,

  // Unified MCP server store (`useMcpServers`).
  servers: () => ["mcp-servers"] as const,
  /** Per-tool config target status (`useMcpToolStatuses`). */
  toolStatuses: () => ["mcp-tool-statuses"] as const,
  /** One server's health probe (`useMcpProbe`). */
  probe: (id: string | null) => ["mcp-probe", id] as const,

  // Local-first MCP catalog browse (`McpMarketBrowser` / `useMcpMarketPage`).
  market: () => ["mcp-market"] as const,
  marketList: () => [...mcpKeys.market(), "list"] as const,
  marketSearch: (query: string) => [...mcpKeys.market(), "search", query] as const,
  marketDetail: (id: string | null) => [...mcpKeys.market(), "detail", id] as const,
  marketByPublisher: (publisherId: string) => [...mcpKeys.market(), "by-publisher", publisherId] as const,
  /**
   * One page of the paginated catalog query. The serialized query *is* the key:
   * filters, sort and offset all change the result, and a coarser key would
   * serve a stale page while the next one loads.
   */
  marketPage: (query: unknown) => [...mcpKeys.market(), "page", query] as const,
  /** Latest catalog row for one installed server's registry name. */
  marketLatest: (registryName: string) => [...mcpKeys.market(), "latest", registryName] as const,

  // Install confirmation payload (`useMcpInstallPlan`).
  installPlan: (id: string | null, runtimeId: string | null) =>
    [...mcpKeys.market(), "install-plan", id, runtimeId] as const,

  // Catalog sources and their freshness (`useMcpSources`).
  sources: () => ["mcp-sources"] as const,
  sourceSyncStates: () => ["mcp-source-sync-states"] as const,
  marketSyncStates: () => ["mcp-market-sync-states"] as const,

  // Curated publishers grid (`useMcpPublishers`).
  publishers: () => ["mcp-publishers"] as const,

  // Built-in presets (`useMcpPresets`).
  presets: () => ["mcp-presets"] as const,
};

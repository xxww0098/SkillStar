import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { LocalFirstResult, McpMarketEntry, SnapshotStatus } from "../../../types";

const STALE_TIME_MS = 5 * 60_000;
const MARKET_KEY = ["mcp-market"] as const;
const SCOPE = "mcp_registry";
const SEARCH_DEBOUNCE_MS = 400;

/**
 * Local-first browse of the GitHub MCP Registry. Mirrors the skill marketplace
 * hook (`useMarketplace`): serves the cached snapshot immediately, searches via
 * the backend FTS, and triggers a background refresh once when the snapshot is
 * stale — without blocking the UI.
 */
export function useMcpMarketplace() {
  const queryClient = useQueryClient();
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const staleRefreshTriggered = useRef(false);

  useEffect(() => {
    const handle = setTimeout(() => setDebounced(query.trim()), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(handle);
  }, [query]);

  const listQuery = useQuery<LocalFirstResult<McpMarketEntry[]>>({
    queryKey: [...MARKET_KEY, "list"],
    queryFn: () => tauriInvoke("list_mcp_market_servers_local"),
    staleTime: STALE_TIME_MS,
  });

  const searchQuery = useQuery<LocalFirstResult<McpMarketEntry[]>>({
    queryKey: [...MARKET_KEY, "search", debounced],
    queryFn: () => tauriInvoke("search_mcp_market_local", { query: debounced, limit: 100 }),
    enabled: debounced.length > 0,
    staleTime: STALE_TIME_MS,
  });

  const active = debounced.length > 0 ? searchQuery : listQuery;
  const result = active.data;
  const status: SnapshotStatus | undefined = result?.snapshot_status;

  const syncMutation = useMutation({
    mutationFn: () => tauriInvoke("sync_mcp_market_scope", { scope: SCOPE }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: MARKET_KEY }),
  });

  // One background refresh per session when the cache is stale.
  useEffect(() => {
    if (status === "stale" && !staleRefreshTriggered.current && !syncMutation.isPending) {
      staleRefreshTriggered.current = true;
      syncMutation.mutate();
    }
  }, [status, syncMutation]);

  return {
    entries: result?.data ?? [],
    status,
    updatedAt: result?.snapshot_updated_at ?? null,
    isLoading: active.isLoading,
    isError: active.isError,
    query,
    setQuery,
    refresh: () => syncMutation.mutate(),
    refreshing: syncMutation.isPending,
  };
}

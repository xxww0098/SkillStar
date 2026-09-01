import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo, useState } from "react";
import { useDebouncedValue } from "../../../hooks/useDebouncedValue";
import { tauriInvoke } from "../../../lib/ipc";
import type { LocalFirstResult, McpServerPage } from "../../../types";
import { mcpKeys } from "../api/keys";
import {
  buildMcpServerQuery,
  clampMcpOffset,
  DEFAULT_MCP_MARKET_FILTERS,
  DEFAULT_MCP_PAGE_SIZE,
  type McpMarketFilterState,
  mcpPageWindow,
} from "../lib/marketQuery";

const MCP_MARKET_SCOPE = "mcp_registry";
const MARKET_STALE_TIME_MS = 60_000;
/** Catalog search hits SQLite FTS across ~21k rows. Wait out a keystroke burst. */
const SEARCH_DEBOUNCE_MS = 200;

export interface UseMcpMarketPageOptions {
  /** Scope to one publisher bucket; omit for the whole merged catalog. */
  publisherId?: string | null;
  pageSize?: number;
  enabled?: boolean;
}

/**
 * Paginated, filtered, sorted browse over the merged MCP catalog.
 *
 * The catalog is ~21k rows across all sources, so this is the only supported
 * browse path: every filter and the sort order compile into one
 * `query_mcp_market_servers_local` call, and the page carries the
 * pre-pagination `total` so "showing 1–60 of 21363" needs no second round trip.
 * The unpaginated `list_mcp_*` commands remain appropriate only for small
 * publisher buckets.
 *
 * `keepPreviousData` keeps the current page on screen while the next one
 * loads — paging through a catalog that blanks between pages reads as breakage.
 */
export function useMcpMarketPage({ publisherId = null, pageSize, enabled = true }: UseMcpMarketPageOptions = {}) {
  const limit = pageSize ?? DEFAULT_MCP_PAGE_SIZE;
  const [filters, setFilters] = useState<McpMarketFilterState>(DEFAULT_MCP_MARKET_FILTERS);
  const [offset, setOffset] = useState(0);
  const queryClient = useQueryClient();
  const debouncedSearch = useDebouncedValue(filters.search, SEARCH_DEBOUNCE_MS);
  const queryFilters = useMemo(
    () => (debouncedSearch === filters.search ? filters : { ...filters, search: debouncedSearch }),
    [filters, debouncedSearch],
  );

  const query = useMemo(
    () => buildMcpServerQuery({ filters: queryFilters, limit, offset, publisherId }),
    [queryFilters, limit, offset, publisherId],
  );

  const pageQuery = useQuery<LocalFirstResult<McpServerPage>>({
    queryKey: mcpKeys.marketPage(query),
    queryFn: () => tauriInvoke("query_mcp_market_servers_local", { query }),
    enabled,
    staleTime: MARKET_STALE_TIME_MS,
    placeholderData: keepPreviousData,
  });

  const page = pageQuery.data?.data;
  const items = page?.items ?? [];
  const total = page?.total ?? 0;
  const window = mcpPageWindow(total, offset, limit, items.length);

  /** Any filter change resets to page 1 — page 40 of the old result set is meaningless. */
  const updateFilters = useCallback(
    (next: McpMarketFilterState | ((prev: McpMarketFilterState) => McpMarketFilterState)) => {
      setFilters((prev) => (typeof next === "function" ? next(prev) : next));
      setOffset(0);
    },
    [],
  );

  const goToOffset = useCallback(
    (nextOffset: number) => setOffset(clampMcpOffset(nextOffset, total, limit)),
    [total, limit],
  );

  const refreshMutation = useMutation({
    mutationFn: () => tauriInvoke("sync_mcp_market_scope", { scope: MCP_MARKET_SCOPE }),
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: mcpKeys.market() });
      queryClient.invalidateQueries({ queryKey: mcpKeys.sourceSyncStates() });
      queryClient.invalidateQueries({ queryKey: mcpKeys.marketSyncStates() });
      queryClient.invalidateQueries({ queryKey: mcpKeys.publishers() });
    },
  });

  return {
    filters,
    setFilters: updateFilters,
    resetFilters: useCallback(() => updateFilters(DEFAULT_MCP_MARKET_FILTERS), [updateFilters]),
    items,
    total,
    window,
    limit,
    offset,
    goToOffset,
    nextPage: useCallback(() => goToOffset(offset + limit), [goToOffset, offset, limit]),
    prevPage: useCallback(() => goToOffset(offset - limit), [goToOffset, offset, limit]),
    snapshotStatus: pageQuery.data?.snapshot_status,
    snapshotUpdatedAt: pageQuery.data?.snapshot_updated_at ?? null,
    snapshotError: pageQuery.data?.error ?? null,
    isLoading: pageQuery.isLoading,
    isFetching: pageQuery.isFetching,
    error: pageQuery.error ?? null,
    refresh: useCallback(() => refreshMutation.mutateAsync(), [refreshMutation]),
    refreshing: refreshMutation.isPending,
  };
}

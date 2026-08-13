import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpCustomSource, McpSourceDescriptor, SyncStateEntry } from "../../../types";
import { mcpKeys } from "../api/keys";
import { buildMcpSourceStatuses, summarizeMcpCatalogHealth } from "../lib/sourceHealth";

const SOURCES_STALE_TIME_MS = 30_000;

/**
 * Catalog sources plus their per-source freshness.
 *
 * The two reads belong together: a source list without sync state cannot say
 * whether the catalog it produced is complete, and sync state without the list
 * cannot say which source a `mcp_registry:<id>` scope belongs to or whether the
 * user has since switched it off.
 *
 * Every mutation returns the full descriptor list, so the cache is written from
 * the response rather than refetched — and the catalog itself is invalidated,
 * because enabling or removing a source changes which rows the merged view
 * contains.
 */
export function useMcpSources(enabled = true) {
  const queryClient = useQueryClient();

  const sourcesQuery = useQuery<McpSourceDescriptor[]>({
    queryKey: mcpKeys.sources(),
    queryFn: () => tauriInvoke("list_mcp_sources"),
    enabled,
    staleTime: SOURCES_STALE_TIME_MS,
  });

  const statesQuery = useQuery<SyncStateEntry[]>({
    queryKey: mcpKeys.sourceSyncStates(),
    queryFn: () => tauriInvoke("get_mcp_source_sync_states"),
    enabled,
    staleTime: SOURCES_STALE_TIME_MS,
  });

  const sources = useMemo(() => sourcesQuery.data ?? [], [sourcesQuery.data]);
  const syncStates = useMemo(() => statesQuery.data ?? [], [statesQuery.data]);
  const statuses = useMemo(() => buildMcpSourceStatuses(syncStates, sources), [syncStates, sources]);
  const health = useMemo(() => summarizeMcpCatalogHealth(statuses), [statuses]);

  const onMutated = useCallback(
    (next: McpSourceDescriptor[]) => {
      queryClient.setQueryData(mcpKeys.sources(), next);
      queryClient.invalidateQueries({ queryKey: mcpKeys.market() });
      queryClient.invalidateQueries({ queryKey: mcpKeys.sourceSyncStates() });
      queryClient.invalidateQueries({ queryKey: mcpKeys.publishers() });
    },
    [queryClient],
  );

  const addMutation = useMutation({
    mutationFn: (source: McpCustomSource) => tauriInvoke("add_mcp_source", { source }),
    onSuccess: onMutated,
  });
  const removeMutation = useMutation({
    mutationFn: (id: string) => tauriInvoke("remove_mcp_source", { id }),
    onSuccess: onMutated,
  });
  const setEnabledMutation = useMutation({
    mutationFn: ({ id, enabled: on }: { id: string; enabled: boolean }) =>
      tauriInvoke("set_mcp_source_enabled", { id, enabled: on }),
    onSuccess: onMutated,
  });

  return {
    sources,
    statuses,
    health,
    isLoading: sourcesQuery.isLoading || statesQuery.isLoading,
    error: sourcesQuery.error ?? statesQuery.error ?? null,
    addSource: useCallback((source: McpCustomSource) => addMutation.mutateAsync(source), [addMutation]),
    removeSource: useCallback((id: string) => removeMutation.mutateAsync(id), [removeMutation]),
    setSourceEnabled: useCallback(
      (id: string, on: boolean) => setEnabledMutation.mutateAsync({ id, enabled: on }),
      [setEnabledMutation],
    ),
    mutating: addMutation.isPending || removeMutation.isPending || setEnabledMutation.isPending,
  };
}

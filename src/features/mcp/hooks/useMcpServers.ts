import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpServerEntry, McpServerPatch, McpServerWithSync, McpStore, McpToolStatus } from "../../../types";

const MCP_STALE_TIME_MS = 30_000;
const STORE_KEY = ["mcp-servers"] as const;
const STATUS_KEY = ["mcp-tool-statuses"] as const;

/**
 * Hook for managing the unified MCP server store.
 *
 * Provides CRUD, per-tool enable toggles, sync, and import — all serialized
 * through the backend write-lock. Mutations invalidate both the store and the
 * per-tool status query so the UI reflects live config changes.
 */
export function useMcpServers() {
  const queryClient = useQueryClient();

  const { data, isLoading, error } = useQuery<McpStore>({
    queryKey: STORE_KEY,
    queryFn: () => tauriInvoke("list_mcp_servers"),
    staleTime: MCP_STALE_TIME_MS,
  });

  const { data: toolStatuses } = useQuery<McpToolStatus[]>({
    queryKey: STATUS_KEY,
    queryFn: () => tauriInvoke("mcp_tool_statuses"),
    staleTime: MCP_STALE_TIME_MS,
  });

  const servers = useMemo(() => {
    if (!data) return [];
    return [...data.servers].sort((a, b) => a.sortIndex - b.sortIndex);
  }, [data]);

  const invalidate = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: STORE_KEY });
    queryClient.invalidateQueries({ queryKey: STATUS_KEY });
  }, [queryClient]);

  const createMutation = useMutation({
    mutationFn: (entry: Partial<McpServerEntry>) => tauriInvoke("create_mcp_server", { entry }),
    onSuccess: invalidate,
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: McpServerPatch }) =>
      tauriInvoke("update_mcp_server", { id, patch }),
    onSuccess: invalidate,
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => tauriInvoke("delete_mcp_server", { id }),
    onSuccess: invalidate,
  });

  const toggleMutation = useMutation({
    mutationFn: ({ id, toolId, enabled }: { id: string; toolId: string; enabled: boolean }) =>
      tauriInvoke("set_mcp_tool_enabled", { id, toolId, enabled }),
    onMutate: async ({ id, toolId, enabled }) => {
      await queryClient.cancelQueries({ queryKey: STORE_KEY });
      const previous = queryClient.getQueryData<McpStore>(STORE_KEY);
      if (previous) {
        queryClient.setQueryData<McpStore>(STORE_KEY, {
          ...previous,
          servers: previous.servers.map((s) =>
            s.id === id ? { ...s, enabled: { ...s.enabled, [toolId]: enabled } } : s,
          ),
        });
      }
      return { previous };
    },
    onError: (_e, _v, ctx) => {
      if (ctx?.previous) queryClient.setQueryData(STORE_KEY, ctx.previous);
    },
    onSettled: invalidate,
  });

  const syncAllMutation = useMutation({
    mutationFn: (force: boolean) => tauriInvoke("sync_all_mcp", { force }),
    onSuccess: invalidate,
  });

  const importMutation = useMutation({
    mutationFn: (toolId: string) => tauriInvoke("import_mcp_from_tool", { toolId }),
    onSuccess: invalidate,
  });

  const reorderMutation = useMutation({
    mutationFn: (orderedIds: string[]) => tauriInvoke("reorder_mcp_servers", { orderedIds }),
    onSettled: invalidate,
  });

  const createServer = useCallback(
    (entry: Partial<McpServerEntry>): Promise<McpServerWithSync> => createMutation.mutateAsync(entry),
    [createMutation],
  );
  const updateServer = useCallback(
    (id: string, patch: McpServerPatch): Promise<McpServerWithSync> => updateMutation.mutateAsync({ id, patch }),
    [updateMutation],
  );
  const deleteServer = useCallback((id: string) => deleteMutation.mutateAsync(id), [deleteMutation]);
  const toggleTool = useCallback(
    (id: string, toolId: string, enabled: boolean) => toggleMutation.mutateAsync({ id, toolId, enabled }),
    [toggleMutation],
  );
  const syncAll = useCallback((force = false) => syncAllMutation.mutateAsync(force), [syncAllMutation]);
  const importFromTool = useCallback((toolId: string) => importMutation.mutateAsync(toolId), [importMutation]);
  const reorder = useCallback((orderedIds: string[]) => reorderMutation.mutateAsync(orderedIds), [reorderMutation]);

  return {
    servers,
    toolStatuses: toolStatuses ?? [],
    isLoading,
    error: error ?? null,
    createServer,
    updateServer,
    deleteServer,
    toggleTool,
    syncAll,
    importFromTool,
    reorder,
    syncing: syncAllMutation.isPending,
    importing: importMutation.isPending,
  };
}

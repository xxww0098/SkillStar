import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import {
  isMcpToolId,
  type McpInstallAnswer,
  type McpInstallOutcome,
  type McpServerEntry,
  type McpServerPatch,
  type McpServerWithSync,
  type McpStore,
  type McpToolId,
  type McpToolStatus,
} from "../../../types";
import { mcpKeys } from "../api/keys";

/** Everything `mcp_market_install` needs to commit one wizard's worth of work. */
export interface McpMarketInstallSubmission {
  serverId: string;
  /** The shape the plan settled on — never the picker's transient state, which
   * starts null and would let the backend rank a different one. */
  runtimeId: string | null;
  answers: readonly McpInstallAnswer[];
  enabled: Record<string, boolean>;
  /**
   * `McpInstallPreview.approvalTarget` verbatim, unmasked — the backend's own
   * rendering of everything the confirmation step showed. Never rebuilt here.
   */
  approvedTarget: string;
}

const MCP_STALE_TIME_MS = 30_000;
const STORE_KEY = mcpKeys.servers();
type PublicMcpToolStatus = McpToolStatus & { toolId: McpToolId };

/**
 * Hook for managing the unified MCP server store.
 *
 * Provides CRUD, per-tool enable toggles, sync, and import — all serialized
 * through the backend write-lock. Host tool status is read only after the user
 * explicitly asks to import existing configurations.
 */
export function useMcpServers() {
  const queryClient = useQueryClient();

  const { data, isLoading, error } = useQuery<McpStore>({
    queryKey: STORE_KEY,
    queryFn: () => tauriInvoke("list_mcp_servers"),
    staleTime: MCP_STALE_TIME_MS,
  });

  const servers = useMemo(() => {
    if (!data) return [];
    return [...data.servers].sort((a, b) => a.sortIndex - b.sortIndex);
  }, [data]);

  const invalidate = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: STORE_KEY });
  }, [queryClient]);

  const createMutation = useMutation({
    mutationFn: (entry: Partial<McpServerEntry>) => tauriInvoke("create_mcp_server", { entry }),
    onSuccess: invalidate,
  });

  /**
   * Commit a catalog install. Not `createServer`: the backend re-derives the
   * entry from these answers and refuses unless it still renders the command
   * the user approved, so what crosses the wire is the answers, not a draft the
   * renderer assembled. `createServer` stays the manual form's path.
   *
   * The store is invalidated either way — a refusal wrote nothing, and one
   * extra read is cheaper than a branch.
   */
  const installFromMarketMutation = useMutation({
    mutationFn: (submission: McpMarketInstallSubmission): Promise<McpInstallOutcome> =>
      tauriInvoke("mcp_market_install", {
        id: submission.serverId,
        ...(submission.runtimeId ? { runtimeId: submission.runtimeId } : {}),
        answers: [...submission.answers],
        enabled: submission.enabled,
        approvedTarget: submission.approvedTarget,
      }),
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
    mutationFn: ({ id, toolId, enabled }: { id: string; toolId: McpToolId; enabled: boolean }) =>
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

  /**
   * Re-project one server into every tool it is enabled for.
   *
   * The command has existed since the beginning and was never wired up (audit
   * D.3-5); it is what makes a failed projection recoverable without editing
   * and re-saving the entry. `force` re-writes even when the live config
   * already matches, which is the point after a rollback.
   */
  const syncServerMutation = useMutation({
    mutationFn: ({ id, force }: { id: string; force: boolean }) => tauriInvoke("sync_mcp_server", { id, force }),
    onSuccess: invalidate,
  });

  const importMutation = useMutation({
    mutationFn: async () => {
      const statuses = await tauriInvoke("mcp_tool_statuses");
      const importable = statuses.filter(
        (status): status is PublicMcpToolStatus =>
          isMcpToolId(status.toolId) && status.installed && status.serverCount > 0,
      );
      let total = 0;
      for (const status of importable) {
        try {
          total += await tauriInvoke("import_mcp_from_tool", { toolId: status.toolId });
        } catch {
          // Best effort: an explicit bulk import continues past unreadable tools.
        }
      }
      return total;
    },
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
  const installFromMarket = useCallback(
    (submission: McpMarketInstallSubmission): Promise<McpInstallOutcome> =>
      installFromMarketMutation.mutateAsync(submission),
    [installFromMarketMutation],
  );
  const updateServer = useCallback(
    (id: string, patch: McpServerPatch): Promise<McpServerWithSync> => updateMutation.mutateAsync({ id, patch }),
    [updateMutation],
  );
  const deleteServer = useCallback((id: string) => deleteMutation.mutateAsync(id), [deleteMutation]);
  const toggleTool = useCallback(
    (id: string, toolId: McpToolId, enabled: boolean) => toggleMutation.mutateAsync({ id, toolId, enabled }),
    [toggleMutation],
  );
  const syncAll = useCallback((force = false) => syncAllMutation.mutateAsync(force), [syncAllMutation]);
  const syncServer = useCallback(
    (id: string, force = true) => syncServerMutation.mutateAsync({ id, force }),
    [syncServerMutation],
  );
  const importFromTools = useCallback(() => importMutation.mutateAsync(), [importMutation]);
  const reorder = useCallback((orderedIds: string[]) => reorderMutation.mutateAsync(orderedIds), [reorderMutation]);

  return {
    servers,
    isLoading,
    error: error ?? null,
    createServer,
    installFromMarket,
    updateServer,
    deleteServer,
    toggleTool,
    syncAll,
    syncServer,
    importFromTools,
    reorder,
    syncing: syncAllMutation.isPending,
    retrySyncing: syncServerMutation.isPending,
    importing: importMutation.isPending,
  };
}

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { AppId, ToolConfigTarget, ToolSyncResult } from "../../../types";

const TOOL_CONFIGS_QUERY_KEY = ["tool-configs"] as const;

/**
 * Hook for managing external tool configuration targets and syncing
 * provider settings to tools (Claude Code, Codex).
 *
 * Caches existence checks with staleTime: Infinity — re-check happens
 * on explicit refetch (e.g., when the ToolConfigPanel opens).
 */
export function useToolConfigs(appId: AppId, providerId: string) {
  const queryClient = useQueryClient();

  const {
    data: targets = [],
    isLoading,
    refetch,
  } = useQuery<ToolConfigTarget[]>({
    queryKey: [...TOOL_CONFIGS_QUERY_KEY, appId],
    queryFn: () => tauriInvoke("get_tool_config_targets", { app_id: appId }),
    staleTime: Number.POSITIVE_INFINITY,
  });

  /** Sync provider config to a single tool. Invalidates targets cache on success. */
  const syncToTool = useCallback(
    async (toolId: string): Promise<ToolSyncResult> => {
      const result = await tauriInvoke("sync_provider_to_tool", {
        app_id: appId,
        provider_id: providerId,
        tool_id: toolId,
      });
      // Refresh targets to update existence status after sync
      queryClient.invalidateQueries({ queryKey: [...TOOL_CONFIGS_QUERY_KEY, appId] });
      return result;
    },
    [appId, providerId, queryClient],
  );

  /** Sync provider config to all tools. Invalidates targets cache on completion. */
  const syncToAll = useCallback(async (): Promise<ToolSyncResult[]> => {
    const toolIds = targets.map((t) => t.tool_id);
    const results = await tauriInvoke("sync_provider_to_all_tools", {
      app_id: appId,
      provider_id: providerId,
      tool_ids: toolIds,
    });
    queryClient.invalidateQueries({ queryKey: [...TOOL_CONFIGS_QUERY_KEY, appId] });
    return results;
  }, [appId, providerId, targets, queryClient]);

  /** Re-check tool config existence (call when panel opens). */
  const recheckTargets = useCallback(() => {
    refetch();
  }, [refetch]);

  return {
    targets,
    isLoading,
    syncToTool,
    syncToAll,
    recheckTargets,
  };
}

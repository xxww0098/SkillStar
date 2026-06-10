/**
 * Agent CLI installation detection, cached per tool. Replaces the hand-rolled
 * detection effects that previously lived in ModelsHub and ToolActivationPanel.
 * Detection failure degrades to "installed" so a broken probe never blocks
 * activation.
 */
import { useQueries } from "@tanstack/react-query";
import { useMemo } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import { modelsKeys } from "./keys";

const INSTALL_STALE_TIME_MS = 5 * 60_000;

export interface ToolInstallStatus {
  installed: boolean;
  binary_found: boolean;
  config_dir_found: boolean;
}

async function detectInstallation(toolId: string): Promise<ToolInstallStatus> {
  try {
    return await tauriInvoke("detect_tool_installation", { toolId });
  } catch {
    // Optimistic fallback: never block the user because detection broke.
    return { installed: true, binary_found: false, config_dir_found: false };
  }
}

/** Install status for a set of tools, shared app-wide through the query cache. */
export function useToolInstallStatuses(toolIds: string[]) {
  const results = useQueries({
    queries: toolIds.map((toolId) => ({
      queryKey: modelsKeys.install(toolId),
      queryFn: () => detectInstallation(toolId),
      staleTime: INSTALL_STALE_TIME_MS,
      retry: false,
    })),
  });

  return useMemo(() => {
    const byTool: Record<string, ToolInstallStatus | undefined> = {};
    toolIds.forEach((toolId, i) => {
      byTool[toolId] = results[i]?.data;
    });
    return { byTool, isLoading: results.some((r) => r.isLoading) };
  }, [toolIds, results]);
}

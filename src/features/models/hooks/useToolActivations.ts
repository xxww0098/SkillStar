import { useCallback, useEffect, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { ToolActivationsMap, ToolSyncResult } from "../../../types";

/**
 * Hook for managing tool activation state and toggle operations.
 *
 * Fetches the current tool_activations map from the backend and provides
 * helpers to check activation status and toggle a tool on/off for a provider.
 *
 * @param providerId - The provider to check activations for (null = no provider selected)
 */
export function useToolActivations(providerId: string | null) {
  const [activations, setActivations] = useState<ToolActivationsMap>({});
  const [isLoading, setIsLoading] = useState(false);

  // Fetch current activations from backend
  const fetchActivations = useCallback(async () => {
    setIsLoading(true);
    try {
      const result = await tauriInvoke("get_tool_activations");
      setActivations(result);
    } catch {
      // Silently handle — activations default to empty map
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Re-fetch when providerId changes
  useEffect(() => {
    fetchActivations();
  }, [fetchActivations, providerId]);

  /**
   * Check if a specific tool is currently active for this provider.
   */
  const isActive = useCallback(
    (toolId: string): boolean => {
      if (!providerId) return false;
      const activation = activations[toolId];
      return activation?.provider_id === providerId;
    },
    [providerId, activations],
  );

  /**
   * Toggle a tool's activation for the current provider.
   *
   * - If the tool is currently active for this provider → deactivate it
   * - If the tool is inactive or active for another provider → activate this provider
   *
   * @param toolId - The tool to toggle (e.g., "claude-code", "codex")
   * @param model - Optional model override; uses provider's default_model if omitted
   */
  const toggle = useCallback(
    async (toolId: string, model?: string): Promise<ToolSyncResult | void> => {
      if (!providerId) return;

      const currentlyActive = activations[toolId]?.provider_id === providerId;

      if (currentlyActive) {
        // Deactivate
        await tauriInvoke("deactivate_tool", { toolId });
        setActivations((prev) => ({ ...prev, [toolId]: null }));
      } else {
        // Activate
        const result = await tauriInvoke("activate_tool", {
          providerId,
          toolId,
          model: model ?? null,
        });
        setActivations((prev) => ({
          ...prev,
          [toolId]: { provider_id: providerId, model: model ?? "" },
        }));
        return result;
      }
    },
    [providerId, activations],
  );

  return {
    activations,
    isActive,
    toggle,
    isLoading,
    refresh: fetchActivations,
  };
}

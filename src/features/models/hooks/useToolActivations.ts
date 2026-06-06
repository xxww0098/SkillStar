import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { tauriInvoke } from "../../../lib/ipc";
import type { CodexSettings, ToolActivationsMap, ToolSyncResult } from "../../../types";

function toolDisplayName(toolId: string): string {
  if (toolId === "claude-code") return "Claude Code";
  if (toolId === "codex") return "Codex";
  return toolId;
}

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
      setActivations(result || {});
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
   * @param settings - Optional per-tool settings (e.g. Codex wire_api/auth_mode)
   */
  const toggle = useCallback(
    async (
      toolId: string,
      model?: string,
      settings?: Record<string, unknown> | null,
    ): Promise<ToolSyncResult | void> => {
      if (!providerId) return;

      const currentlyActive = activations[toolId]?.provider_id === providerId;

      if (currentlyActive) {
        await tauriInvoke("deactivate_tool", { toolId });
        setActivations((prev) => ({ ...prev, [toolId]: null }));
        toast.success(`${toolDisplayName(toolId)} 已停用`);
      } else {
        const result = await tauriInvoke("activate_tool", {
          providerId,
          toolId,
          model: model ?? null,
          settings: settings ?? null,
        });
        setActivations((prev) => ({
          ...prev,
          [toolId]: { provider_id: providerId, model: model ?? "", settings: settings as CodexSettings | undefined },
        }));
        if (result.success) {
          toast.success(`${toolDisplayName(toolId)} 已同步到配置文件`);
        } else {
          toast.error(result.error ?? `${toolDisplayName(toolId)} 同步失败`);
        }
        return result;
      }
    },
    [providerId, activations],
  );

  /**
   * Update only the settings of an active tool (e.g. Codex wire_api / auth_mode).
   * Does not change provider or model — just updates config and re-syncs.
   */
  const updateSettings = useCallback(
    async (toolId: string, settings: Record<string, unknown>): Promise<ToolSyncResult> => {
      const result = await tauriInvoke("update_tool_settings", { toolId, settings });
      setActivations((prev) => {
        const existing = prev[toolId];
        if (!existing) return prev;
        return { ...prev, [toolId]: { ...existing, settings: settings as unknown as CodexSettings } };
      });
      if (result.success) {
        toast.success(`${toolDisplayName(toolId)} 配置已更新`);
      } else {
        toast.error(result.error ?? `${toolDisplayName(toolId)} 配置更新失败`);
      }
      return result;
    },
    [],
  );

  return {
    activations,
    isActive,
    toggle,
    updateSettings,
    isLoading,
    refresh: fetchActivations,
  };
}

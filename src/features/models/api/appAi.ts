import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";
import { invalidateAiConfigCache } from "../../../hooks/useAiConfig";
import { tauriInvoke } from "../../../lib/ipc";
import type { AiConfig, AiProviderRef } from "../../../types";

export type AppAiAppId = "claude" | "codex";

/**
 * Bind application AI (summarize / translate / skill pick) to a flat-store provider.
 */
export function useAppAiProvider() {
  const queryClient = useQueryClient();

  const setMutation = useMutation({
    mutationFn: ({ appId, providerId }: { appId: AppAiAppId; providerId: string }) =>
      tauriInvoke("set_app_ai_provider_ref", { appId, providerId }),
    onSuccess: () => {
      invalidateAiConfigCache();
      queryClient.invalidateQueries({ queryKey: ["ai-config"] });
    },
  });

  const clearMutation = useMutation({
    mutationFn: () => tauriInvoke("clear_app_ai_provider_ref"),
    onSuccess: () => {
      invalidateAiConfigCache();
      queryClient.invalidateQueries({ queryKey: ["ai-config"] });
    },
  });

  const setAppAiProvider = useCallback(
    async (appId: AppAiAppId, providerId: string, providerName?: string) => {
      try {
        await setMutation.mutateAsync({ appId, providerId });
        const label = appId === "claude" ? "Claude" : "Codex";
        toast.success(`已设为应用内 AI（${label}）${providerName ? `：${providerName}` : ""}`);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        toast.error(`设置失败：${message}`);
        throw err;
      }
    },
    [setMutation],
  );

  const clearAppAiProvider = useCallback(async () => {
    await clearMutation.mutateAsync();
    toast.success("已清除应用内 AI 供应商绑定");
  }, [clearMutation]);

  const matchesProviderRef = useCallback(
    (config: AiConfig | null | undefined, providerId: string): AiProviderRef | null => {
      if (!config?.provider_ref || config.provider_ref.provider_id !== providerId) return null;
      return config.provider_ref;
    },
    [],
  );

  return {
    setAppAiProvider,
    clearAppAiProvider,
    matchesProviderRef,
    isSetting: setMutation.isPending,
    isClearing: clearMutation.isPending,
  };
}

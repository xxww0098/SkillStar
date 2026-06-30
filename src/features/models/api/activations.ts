/**
 * Tool activation state + mutations. The activation map's single source of
 * truth is the providers-flat query cache (`get_providers_flat` already
 * returns `tool_activations`) — there is intentionally no separate
 * `get_tool_activations` fetch anymore, so cards, panels and the gallery can
 * never disagree.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";
import i18n from "../../../i18n";
import { tauriInvoke } from "../../../lib/ipc";
import type { FlatProvidersResponse, ToolActivationsMap, ToolSyncResult } from "../../../types";
import { getAgent } from "../lib/agentRegistry";
import { removeBindingEntry as removeEntryLocal, setActiveProvider, upsertBindingEntry } from "../lib/toolBinding";
import { modelsKeys } from "./keys";

/** Tool ids where this provider is bound in any binding entry. */
export function getProviderToolBadges(providerId: string, toolActivations: ToolActivationsMap): string[] {
  return Object.entries(toolActivations ?? {})
    .filter(([, binding]) => binding?.entries?.some((e) => e.provider_id === providerId))
    .map(([toolId]) => toolId);
}

function toolDisplayName(toolId: string): string {
  return getAgent(toolId)?.displayName ?? toolId;
}

/** Activation map selected straight from the providers-flat cache. */
export function useToolActivationsMap() {
  return useQuery<FlatProvidersResponse, Error, ToolActivationsMap>({
    queryKey: modelsKeys.providersFlat(),
    queryFn: () => tauriInvoke("get_providers_flat"),
    staleTime: 30_000,
    select: (data) => data.tool_activations ?? {},
  });
}

export function useActivationMutations() {
  const queryClient = useQueryClient();
  const queryKey = modelsKeys.providersFlat();

  const invalidate = useCallback(() => {
    queryClient.invalidateQueries({ queryKey });
  }, [queryClient, queryKey]);

  const activateMutation = useMutation({
    mutationFn: ({
      providerId,
      toolId,
      model,
      settings,
    }: {
      providerId: string;
      toolId: string;
      model?: string;
      settings?: Record<string, unknown> | null;
    }) => tauriInvoke("activate_tool", { providerId, toolId, model: model ?? null, settings: settings ?? null }),
    onMutate: async ({ providerId, toolId, model }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(queryKey);
      if (previous) {
        const provider = previous.providers.find((p) => p.id === providerId);
        const resolvedModel = model ?? provider?.default_model ?? "";
        const prevBinding = previous.tool_activations[toolId];
        const nextBinding = upsertBindingEntry(prevBinding, toolId, {
          provider_id: providerId,
          model: resolvedModel,
        });
        queryClient.setQueryData<FlatProvidersResponse>(queryKey, {
          ...previous,
          tool_activations: {
            ...previous.tool_activations,
            [toolId]: nextBinding,
          },
        });
      }
      return { previous };
    },
    onSuccess: (result, { toolId }) => {
      if (result?.success) {
        toast.success(i18n.t("models.toasts.syncedToConfig", { name: toolDisplayName(toolId) }));
      } else if (result) {
        toast.error(result.error ?? i18n.t("models.toasts.syncFailed", { name: toolDisplayName(toolId) }));
      }
    },
    onError: (err, { toolId }, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKey, context.previous);
      }
      toast.error(
        err instanceof Error ? err.message : i18n.t("models.toasts.syncFailed", { name: toolDisplayName(toolId) }),
      );
    },
    onSettled: invalidate,
  });

  const deactivateMutation = useMutation({
    mutationFn: (toolId: string) => tauriInvoke("deactivate_tool", { toolId }),
    onMutate: async (toolId) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(queryKey);
      if (previous) {
        queryClient.setQueryData<FlatProvidersResponse>(queryKey, {
          ...previous,
          tool_activations: { ...previous.tool_activations, [toolId]: { entries: [], active_index: 0 } },
        });
      }
      return { previous };
    },
    onSuccess: (_result, toolId) => {
      toast.success(i18n.t("models.toasts.deactivated", { name: toolDisplayName(toolId) }));
    },
    onError: (err, toolId, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKey, context.previous);
      }
      toast.error(
        err instanceof Error
          ? err.message
          : i18n.t("models.toasts.deactivateFailed", { name: toolDisplayName(toolId) }),
      );
    },
    onSettled: invalidate,
  });

  const updateSettingsMutation = useMutation({
    mutationFn: ({ toolId, settings }: { toolId: string; settings: Record<string, unknown> }) =>
      tauriInvoke("update_tool_settings", { toolId, settings }),
    onSuccess: (result, { toolId }) => {
      if (result?.success) {
        toast.success(i18n.t("models.toasts.settingsUpdated", { name: toolDisplayName(toolId) }));
      } else if (result) {
        toast.error(result.error ?? i18n.t("models.toasts.settingsUpdateFailed", { name: toolDisplayName(toolId) }));
      }
    },
    onError: (err, { toolId }) => {
      toast.error(
        err instanceof Error
          ? err.message
          : i18n.t("models.toasts.settingsUpdateFailed", { name: toolDisplayName(toolId) }),
      );
    },
    onSettled: invalidate,
  });

  const setActiveBindingMutation = useMutation({
    mutationFn: ({ toolId, providerId }: { toolId: string; providerId: string }) =>
      tauriInvoke("set_active_binding", { toolId, providerId }),
    onMutate: async ({ toolId, providerId }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(queryKey);
      if (previous) {
        queryClient.setQueryData<FlatProvidersResponse>(queryKey, {
          ...previous,
          tool_activations: {
            ...previous.tool_activations,
            [toolId]: setActiveProvider(previous.tool_activations[toolId], providerId),
          },
        });
      }
      return { previous };
    },
    onSuccess: (result, { toolId }) => {
      if (result && !result.success) {
        toast.error(result.error ?? i18n.t("models.toasts.syncFailed", { name: toolDisplayName(toolId) }));
      }
    },
    onError: (err, { toolId }, context) => {
      if (context?.previous) queryClient.setQueryData(queryKey, context.previous);
      toast.error(
        err instanceof Error ? err.message : i18n.t("models.toasts.syncFailed", { name: toolDisplayName(toolId) }),
      );
    },
    onSettled: invalidate,
  });

  const removeBindingEntryMutation = useMutation({
    mutationFn: ({ toolId, providerId }: { toolId: string; providerId: string }) =>
      tauriInvoke("remove_binding_entry", { toolId, providerId }),
    onMutate: async ({ toolId, providerId }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(queryKey);
      if (previous) {
        queryClient.setQueryData<FlatProvidersResponse>(queryKey, {
          ...previous,
          tool_activations: {
            ...previous.tool_activations,
            [toolId]: removeEntryLocal(previous.tool_activations[toolId], providerId),
          },
        });
      }
      return { previous };
    },
    onError: (err, { toolId }, context) => {
      if (context?.previous) queryClient.setQueryData(queryKey, context.previous);
      toast.error(
        err instanceof Error
          ? err.message
          : i18n.t("models.toasts.deactivateFailed", { name: toolDisplayName(toolId) }),
      );
    },
    onSettled: invalidate,
  });

  const activateTool = useCallback(
    (
      providerId: string,
      toolId: string,
      model?: string,
      settings?: Record<string, unknown> | null,
    ): Promise<ToolSyncResult> => activateMutation.mutateAsync({ providerId, toolId, model, settings }),
    [activateMutation],
  );

  const deactivateTool = useCallback(
    async (toolId: string): Promise<void> => {
      await deactivateMutation.mutateAsync(toolId);
    },
    [deactivateMutation],
  );

  const updateToolSettings = useCallback(
    (toolId: string, settings: Record<string, unknown>): Promise<ToolSyncResult> =>
      updateSettingsMutation.mutateAsync({ toolId, settings }),
    [updateSettingsMutation],
  );

  const setActiveBinding = useCallback(
    (toolId: string, providerId: string): Promise<ToolSyncResult> =>
      setActiveBindingMutation.mutateAsync({ toolId, providerId }),
    [setActiveBindingMutation],
  );

  const removeBindingEntry = useCallback(
    (toolId: string, providerId: string): Promise<ToolSyncResult> =>
      removeBindingEntryMutation.mutateAsync({ toolId, providerId }),
    [removeBindingEntryMutation],
  );

  return { activateTool, deactivateTool, updateToolSettings, setActiveBinding, removeBindingEntry };
}

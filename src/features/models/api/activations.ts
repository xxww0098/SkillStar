/**
 * Tool activation state + mutations. The activation map's single source of
 * truth is the providers-flat query cache (`get_providers_flat` already
 * returns `tool_activations`) — there is intentionally no separate
 * `get_tool_activations` fetch anymore, so cards, panels and the gallery can
 * never disagree.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";
import i18n from "../../../i18n";
import { tauriInvoke } from "../../../lib/ipc";
import type {
  DroppedRole,
  FlatProvidersResponse,
  ToolActivationsMap,
  ToolBindingSettings,
  ToolSyncResult,
} from "../../../types";
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

export function useActivationMutations() {
  const queryClient = useQueryClient();
  const queryKey = modelsKeys.providersFlat();

  const invalidate = useCallback(() => {
    queryClient.invalidateQueries({ queryKey });
  }, [queryClient, queryKey]);

  /**
   * Park what the write skipped, and say so once.
   *
   * Every mutation here can silently discard part of the role map — a role
   * pointing at a provider this agent is not bound to, a row with no model.
   * Before this, the panel kept showing the assignment and the file did not
   * have it, and only the file knew. The backend reports the difference; this
   * is the one place that remembers it and tells the user, so no call site can
   * forget to.
   */
  const recordDrops = useCallback(
    (toolId: string, result: ToolSyncResult | null | undefined) => {
      const dropped: DroppedRole[] = result?.dropped_roles ?? [];
      queryClient.setQueryData<DroppedRole[]>(modelsKeys.roleDrops(toolId), dropped);
      if (dropped.length > 0) {
        toast.warning(
          i18n.t("models.roleDrops.toastTitle", {
            count: dropped.length,
            name: toolDisplayName(toolId),
          }),
          {
            description: dropped
              .map((drop) => `${drop.role}: ${i18n.t(`models.roleDrops.reason.${drop.reason}`)}`)
              .join("\n"),
          },
        );
      }
    },
    [queryClient],
  );

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
    }) => tauriInvoke("bind_provider", { providerId, toolId, model: model ?? null, settings: settings ?? null }),
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
      recordDrops(toolId, result);
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
    mutationFn: (toolId: string) => tauriInvoke("unbind_agent", { toolId }),
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
      tauriInvoke("update_binding_entry_settings", { toolId, settings }),
    onSuccess: (result, { toolId }) => {
      recordDrops(toolId, result);
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

  // Binding-level settings (OMP model roles). Optimistic so the role panel
  // reflects the new assignment immediately; the single providers cache entry is
  // the only place binding state lives (see the note at the top of this file).
  const updateBindingSettingsMutation = useMutation({
    mutationFn: ({ toolId, settings }: { toolId: string; settings: ToolBindingSettings }) =>
      tauriInvoke("update_agent_settings", { toolId, settings }),
    onMutate: async ({ toolId, settings }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(queryKey);
      const binding = previous?.tool_activations[toolId];
      if (previous && binding) {
        queryClient.setQueryData<FlatProvidersResponse>(queryKey, {
          ...previous,
          tool_activations: {
            ...previous.tool_activations,
            [toolId]: { ...binding, settings },
          },
        });
      }
      return { previous };
    },
    onSuccess: (result, { toolId }) => {
      recordDrops(toolId, result);
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
      recordDrops(toolId, result);
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
      tauriInvoke("unbind_provider", { toolId, providerId }),
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

  const updateToolBindingSettings = useCallback(
    (toolId: string, settings: ToolBindingSettings): Promise<ToolSyncResult> =>
      updateBindingSettingsMutation.mutateAsync({ toolId, settings }),
    [updateBindingSettingsMutation],
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

  return {
    activateTool,
    deactivateTool,
    updateToolSettings,
    updateToolBindingSettings,
    setActiveBinding,
    removeBindingEntry,
  };
}

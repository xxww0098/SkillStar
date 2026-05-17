import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type {
  FlatProvidersResponse,
  ProviderEntryFlat,
  ProviderPatchFlat,
  ToolActivationsMap,
  ToolSyncResult,
} from "../../../types";

const PROVIDERS_FLAT_STALE_TIME_MS = 30_000;
const QUERY_KEY = ["providers-flat"] as const;

/**
 * Compute which tool badges a provider should display.
 *
 * Returns the set of tool_ids where this provider is the active provider.
 */
export function getProviderToolBadges(providerId: string, toolActivations: ToolActivationsMap): string[] {
  return Object.entries(toolActivations ?? {})
    .filter(([, activation]) => activation?.provider_id === providerId)
    .map(([toolId]) => toolId);
}

/**
 * Hook for managing the flat provider store (v2 architecture).
 *
 * Provides CRUD operations, reorder, tool activation/deactivation,
 * loading/error state, and optimistic updates via TanStack Query.
 */
export function useProvidersFlat() {
  const queryClient = useQueryClient();

  const { data, isLoading, error } = useQuery<FlatProvidersResponse>({
    queryKey: QUERY_KEY,
    queryFn: () => tauriInvoke("get_providers_flat"),
    staleTime: PROVIDERS_FLAT_STALE_TIME_MS,
  });

  const providers = useMemo(() => {
    if (!data) return [];
    return [...data.providers].sort((a, b) => a.sort_index - b.sort_index);
  }, [data]);

  const toolActivations: ToolActivationsMap = useMemo(() => {
    return data?.tool_activations ?? {};
  }, [data]);

  const invalidate = useCallback(() => {
    queryClient.invalidateQueries({ queryKey: QUERY_KEY });
  }, [queryClient]);

  // ── Create ──────────────────────────────────────────────────────────

  const createMutation = useMutation({
    mutationFn: (entry: Partial<ProviderEntryFlat>) => tauriInvoke("create_provider_flat", { entry }),
    onSuccess: invalidate,
  });

  const createProvider = useCallback(
    async (entry: Partial<ProviderEntryFlat>): Promise<ProviderEntryFlat> => {
      return createMutation.mutateAsync(entry);
    },
    [createMutation],
  );

  // ── Update ──────────────────────────────────────────────────────────

  const updateMutation = useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: ProviderPatchFlat }) =>
      tauriInvoke("update_provider_flat", { id, patch }),
    onMutate: async ({ id, patch }) => {
      // Optimistic update: apply patch to local cache immediately
      await queryClient.cancelQueries({ queryKey: QUERY_KEY });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(QUERY_KEY);

      if (previous) {
        queryClient.setQueryData<FlatProvidersResponse>(QUERY_KEY, {
          ...previous,
          providers: previous.providers.map((p) => (p.id === id ? ({ ...p, ...patch } as ProviderEntryFlat) : p)),
        });
      }

      return { previous };
    },
    onError: (_err, _vars, context) => {
      // Revert optimistic update on error
      if (context?.previous) {
        queryClient.setQueryData(QUERY_KEY, context.previous);
      }
    },
    onSettled: invalidate,
  });

  const updateProvider = useCallback(
    async (id: string, patch: ProviderPatchFlat): Promise<ProviderEntryFlat> => {
      return updateMutation.mutateAsync({ id, patch });
    },
    [updateMutation],
  );

  // ── Delete ──────────────────────────────────────────────────────────

  const deleteMutation = useMutation({
    mutationFn: (id: string) => tauriInvoke("delete_provider_flat", { id }),
    onMutate: async (id) => {
      // Optimistic update: remove from local cache immediately
      await queryClient.cancelQueries({ queryKey: QUERY_KEY });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(QUERY_KEY);

      if (previous) {
        queryClient.setQueryData<FlatProvidersResponse>(QUERY_KEY, {
          ...previous,
          providers: previous.providers.filter((p) => p.id !== id),
          tool_activations: Object.fromEntries(
            Object.entries(previous.tool_activations ?? {}).map(([toolId, activation]) => [
              toolId,
              activation?.provider_id === id ? null : activation,
            ]),
          ),
        });
      }

      return { previous };
    },
    onError: (_err, _id, context) => {
      if (context?.previous) {
        queryClient.setQueryData(QUERY_KEY, context.previous);
      }
    },
    onSettled: invalidate,
  });

  const deleteProvider = useCallback(
    async (id: string): Promise<void> => {
      await deleteMutation.mutateAsync(id);
    },
    [deleteMutation],
  );

  // ── Reorder ─────────────────────────────────────────────────────────

  const reorderMutation = useMutation({
    mutationFn: (orderedIds: string[]) => tauriInvoke("reorder_providers", { orderedIds }),
    onMutate: async (orderedIds) => {
      // Optimistic update: reorder providers locally
      await queryClient.cancelQueries({ queryKey: QUERY_KEY });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(QUERY_KEY);

      if (previous) {
        const reordered = orderedIds
          .map((id, index) => {
            const provider = previous.providers.find((p) => p.id === id);
            return provider ? { ...provider, sort_index: index } : null;
          })
          .filter((p): p is ProviderEntryFlat => p !== null);

        // Append any providers not in orderedIds (shouldn't happen, but safe)
        const reorderedIds = new Set(orderedIds);
        const remaining = previous.providers
          .filter((p) => !reorderedIds.has(p.id))
          .map((p, i) => ({ ...p, sort_index: reordered.length + i }));

        queryClient.setQueryData<FlatProvidersResponse>(QUERY_KEY, {
          ...previous,
          providers: [...reordered, ...remaining],
        });
      }

      return { previous };
    },
    onError: (_err, _ids, context) => {
      if (context?.previous) {
        queryClient.setQueryData(QUERY_KEY, context.previous);
      }
    },
    onSettled: invalidate,
  });

  const reorderProviders = useCallback(
    async (orderedIds: string[]): Promise<void> => {
      await reorderMutation.mutateAsync(orderedIds);
    },
    [reorderMutation],
  );

  // ── Activate Tool ───────────────────────────────────────────────────

  const activateMutation = useMutation({
    mutationFn: ({ providerId, toolId, model }: { providerId: string; toolId: string; model?: string }) =>
      tauriInvoke("activate_tool", { providerId, toolId, model }),
    onMutate: async ({ providerId, toolId, model }) => {
      // Optimistic update: set tool activation locally
      await queryClient.cancelQueries({ queryKey: QUERY_KEY });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(QUERY_KEY);

      if (previous) {
        const provider = previous.providers.find((p) => p.id === providerId);
        const resolvedModel = model ?? provider?.default_model ?? "";

        queryClient.setQueryData<FlatProvidersResponse>(QUERY_KEY, {
          ...previous,
          tool_activations: {
            ...previous.tool_activations,
            [toolId]: { provider_id: providerId, model: resolvedModel },
          },
        });
      }

      return { previous };
    },
    onError: (_err, _vars, context) => {
      if (context?.previous) {
        queryClient.setQueryData(QUERY_KEY, context.previous);
      }
    },
    onSettled: invalidate,
  });

  const activateTool = useCallback(
    async (providerId: string, toolId: string, model?: string): Promise<ToolSyncResult> => {
      return activateMutation.mutateAsync({ providerId, toolId, model });
    },
    [activateMutation],
  );

  // ── Deactivate Tool ─────────────────────────────────────────────────

  const deactivateMutation = useMutation({
    mutationFn: (toolId: string) => tauriInvoke("deactivate_tool", { toolId }),
    onMutate: async (toolId) => {
      // Optimistic update: clear tool activation locally
      await queryClient.cancelQueries({ queryKey: QUERY_KEY });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(QUERY_KEY);

      if (previous) {
        queryClient.setQueryData<FlatProvidersResponse>(QUERY_KEY, {
          ...previous,
          tool_activations: {
            ...previous.tool_activations,
            [toolId]: null,
          },
        });
      }

      return { previous };
    },
    onError: (_err, _toolId, context) => {
      if (context?.previous) {
        queryClient.setQueryData(QUERY_KEY, context.previous);
      }
    },
    onSettled: invalidate,
  });

  const deactivateTool = useCallback(
    async (toolId: string): Promise<void> => {
      await deactivateMutation.mutateAsync(toolId);
    },
    [deactivateMutation],
  );

  // ── Refresh ─────────────────────────────────────────────────────────

  const refresh = useCallback(async () => {
    await queryClient.invalidateQueries({ queryKey: QUERY_KEY });
  }, [queryClient]);

  return {
    providers,
    toolActivations,
    isLoading,
    error: error ?? null,
    createProvider,
    updateProvider,
    deleteProvider,
    reorderProviders,
    activateTool,
    deactivateTool,
    refresh,
  };
}

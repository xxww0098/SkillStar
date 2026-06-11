/**
 * Provider store queries + mutations (flat v2 store). The single IPC surface
 * for provider CRUD — optimistic updates, rollback and user-facing toasts all
 * live here, not in components.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { toast } from "sonner";
import i18n from "../../../i18n";
import { tauriInvoke } from "../../../lib/ipc";
import type { FlatProvidersResponse, ProviderEntryFlat, ProviderPatchFlat } from "../../../types";
import { modelsKeys } from "./keys";

const PROVIDERS_FLAT_STALE_TIME_MS = 30_000;

export function useProvidersQuery() {
  return useQuery<FlatProvidersResponse>({
    queryKey: modelsKeys.providersFlat(),
    queryFn: () => tauriInvoke("get_providers_flat"),
    staleTime: PROVIDERS_FLAT_STALE_TIME_MS,
  });
}

/**
 * CRUD mutations over the flat provider store. All mutations follow the same
 * convention: optimistic cache write in onMutate, rollback + toast in onError,
 * invalidate in onSettled. `create` seeds the cache from the returned entity
 * instead of inserting a fake id (the create flow needs the real id at once).
 */
export function useProviderMutations() {
  const queryClient = useQueryClient();
  const queryKey = modelsKeys.providersFlat();

  const invalidate = useCallback(() => {
    queryClient.invalidateQueries({ queryKey });
  }, [queryClient, queryKey]);

  const createMutation = useMutation({
    mutationFn: (entry: Partial<ProviderEntryFlat>) => tauriInvoke("create_provider_flat", { entry }),
    onSuccess: (created) => {
      if (created?.id) {
        queryClient.setQueryData<FlatProvidersResponse>(queryKey, (prev) =>
          prev ? { ...prev, providers: [...prev.providers, created] } : prev,
        );
      }
      invalidate();
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: ProviderPatchFlat }) =>
      tauriInvoke("update_provider_flat", { id, patch }),
    onSuccess: (result) => {
      const failed = (result?.tool_sync_results ?? []).filter((r) => !r.success);
      if (failed.length > 0) {
        const names = failed.map((r) => r.tool_id).join("、");
        toast.warning(i18n.t("models.toasts.savedButSyncFailed", { names }));
      }
    },
    onMutate: async ({ id, patch }) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(queryKey);
      if (previous) {
        queryClient.setQueryData<FlatProvidersResponse>(queryKey, {
          ...previous,
          providers: previous.providers.map((p) => (p.id === id ? ({ ...p, ...patch } as ProviderEntryFlat) : p)),
        });
      }
      return { previous };
    },
    onError: (_err, _vars, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKey, context.previous);
      }
    },
    onSettled: invalidate,
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => tauriInvoke("delete_provider_flat", { id }),
    onMutate: async (id) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(queryKey);
      if (previous) {
        queryClient.setQueryData<FlatProvidersResponse>(queryKey, {
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
        queryClient.setQueryData(queryKey, context.previous);
      }
    },
    onSettled: invalidate,
  });

  const reorderMutation = useMutation({
    mutationFn: (orderedIds: string[]) => tauriInvoke("reorder_providers", { orderedIds }),
    onMutate: async (orderedIds) => {
      await queryClient.cancelQueries({ queryKey });
      const previous = queryClient.getQueryData<FlatProvidersResponse>(queryKey);
      if (previous) {
        const reordered = orderedIds
          .map((id, index) => {
            const provider = previous.providers.find((p) => p.id === id);
            return provider ? { ...provider, sort_index: index } : null;
          })
          .filter((p): p is ProviderEntryFlat => p !== null);
        const reorderedIds = new Set(orderedIds);
        const remaining = previous.providers
          .filter((p) => !reorderedIds.has(p.id))
          .map((p, i) => ({ ...p, sort_index: reordered.length + i }));
        queryClient.setQueryData<FlatProvidersResponse>(queryKey, {
          ...previous,
          providers: [...reordered, ...remaining],
        });
      }
      return { previous };
    },
    onError: (_err, _ids, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKey, context.previous);
      }
    },
    onSettled: invalidate,
  });

  const createProvider = useCallback(
    (entry: Partial<ProviderEntryFlat>): Promise<ProviderEntryFlat> => createMutation.mutateAsync(entry),
    [createMutation],
  );

  const updateProvider = useCallback(
    async (id: string, patch: ProviderPatchFlat): Promise<ProviderEntryFlat> => {
      const result = await updateMutation.mutateAsync({ id, patch });
      return result.provider;
    },
    [updateMutation],
  );

  const deleteProvider = useCallback(
    async (id: string): Promise<void> => {
      await deleteMutation.mutateAsync(id);
    },
    [deleteMutation],
  );

  const reorderProviders = useCallback(
    async (orderedIds: string[]): Promise<void> => {
      await reorderMutation.mutateAsync(orderedIds);
    },
    [reorderMutation],
  );

  return { createProvider, updateProvider, deleteProvider, reorderProviders };
}

/**
 * Shallow meta patch for agent-side writes (claude tier mapping, codex
 * settings). Merges over the cached provider and submits through the same
 * update mutation as the drawer autosave, so concurrent writes serialize via
 * cancelQueries instead of clobbering each other.
 */
export function useProviderMetaPatch() {
  const queryClient = useQueryClient();
  const { updateProvider } = useProviderMutations();

  return useCallback(
    async (providerId: string, metaPatch: Record<string, unknown>, patch: ProviderPatchFlat = {}) => {
      const data = queryClient.getQueryData<FlatProvidersResponse>(modelsKeys.providersFlat());
      const provider = data?.providers.find((p) => p.id === providerId);
      if (!provider) throw new Error(i18n.t("models.toasts.providerMissing"));
      return updateProvider(providerId, {
        ...patch,
        meta: { ...(provider.meta ?? {}), ...metaPatch },
      });
    },
    [queryClient, updateProvider],
  );
}

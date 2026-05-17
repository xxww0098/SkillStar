import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { AppId, AppProviders, ProviderEntry, SwitchResult } from "../../../types";

const PROVIDERS_STALE_TIME_MS = 30_000;

function providersQueryKey(appId: AppId) {
  return ["providers", appId] as const;
}

/**
 * Mask an API key for safe display in the UI.
 *
 * - For strings ≥ 8 chars: show first 3 + "..." + last 4 chars
 * - For shorter strings: show "***"
 */
export function maskApiKey(key: string): string {
  if (key.length >= 8) {
    return `${key.slice(0, 3)}...${key.slice(-4)}`;
  }
  return "***";
}

/**
 * Hook for managing providers for a given AppId (claude or codex).
 *
 * Uses TanStack Query for caching with 30s stale time.
 * Exposes CRUD operations and provider switching with automatic cache invalidation.
 */
export function useProviders(appId: AppId) {
  const queryClient = useQueryClient();

  const queryKey = providersQueryKey(appId);

  const { data, isLoading, error } = useQuery<AppProviders>({
    queryKey,
    queryFn: () => tauriInvoke("get_app_providers", { appId }),
    staleTime: PROVIDERS_STALE_TIME_MS,
  });

  const providers = useMemo(() => {
    if (!data) return [];
    return Object.values(data.providers).sort((a, b) => (a.sort_index ?? 0) - (b.sort_index ?? 0));
  }, [data]);

  const current = useMemo(() => {
    if (!data || !data.current) return null;
    return data.providers[data.current] ?? null;
  }, [data]);

  const invalidate = useCallback(() => {
    queryClient.invalidateQueries({ queryKey });
  }, [queryClient, queryKey]);

  const createMutation = useMutation({
    mutationFn: (entry: Omit<ProviderEntry, "id" | "created_at">) => tauriInvoke("create_provider", { appId, entry }),
    onSuccess: invalidate,
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: Partial<ProviderEntry> }) =>
      tauriInvoke("update_provider", { appId, id, patch }),
    onSuccess: invalidate,
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => tauriInvoke("delete_provider", { appId, id }),
    onSuccess: invalidate,
  });

  const switchMutation = useMutation({
    mutationFn: ({ id, syncTools }: { id: string; syncTools?: string[] }) =>
      tauriInvoke("switch_active_provider", {
        appId,
        providerId: id,
        syncTools: syncTools ?? [],
      }),
    onSuccess: invalidate,
  });

  const createProvider = useCallback(
    async (entry: Omit<ProviderEntry, "id" | "created_at">) => {
      await createMutation.mutateAsync(entry);
    },
    [createMutation],
  );

  const updateProvider = useCallback(
    async (id: string, patch: Partial<ProviderEntry>) => {
      await updateMutation.mutateAsync({ id, patch });
    },
    [updateMutation],
  );

  const deleteProvider = useCallback(
    async (id: string) => {
      await deleteMutation.mutateAsync(id);
    },
    [deleteMutation],
  );

  const switchProvider = useCallback(
    async (id: string, syncTools?: string[]): Promise<SwitchResult> => {
      return switchMutation.mutateAsync({ id, syncTools });
    },
    [switchMutation],
  );

  return {
    providers,
    current,
    isLoading,
    error: error ?? null,
    createProvider,
    updateProvider,
    deleteProvider,
    switchProvider,
  };
}

/**
 * Composition hook over the api layer — the historical one-stop interface for
 * the flat provider store. New code can also use the finer-grained hooks in
 * `api/providers.ts` / `api/activations.ts` directly.
 */
import { useMemo } from "react";
import type { ToolActivationsMap } from "../../../types";
import { useActivationMutations } from "../api/activations";
import { useProviderMutations, useProvidersQuery } from "../api/providers";

export { getProviderToolBadges } from "../api/activations";

export function useProvidersFlat() {
  const { data, isLoading, error, refetch } = useProvidersQuery();
  const { createProvider, updateProvider, deleteProvider, reorderProviders } = useProviderMutations();
  const { activateTool, deactivateTool, updateToolSettings } = useActivationMutations();

  const providers = useMemo(() => {
    if (!data) return [];
    return [...data.providers].sort((a, b) => a.sort_index - b.sort_index);
  }, [data]);

  const toolActivations: ToolActivationsMap = useMemo(() => {
    return data?.tool_activations ?? {};
  }, [data]);

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
    updateToolSettings,
    refresh: refetch,
  };
}

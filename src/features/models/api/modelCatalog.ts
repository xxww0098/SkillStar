import { useCallback, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { ModelCatalogFetchResult } from "../../../types";

/**
 * Hook for fetching available models from a provider's unique models URL.
 *
 * Every agent config (Claude, Codex, …) shares one `models_url` per provider,
 * so this hook takes the already-resolved URL directly instead of deriving it
 * from an agent-specific base URL. The backend sends `GET <url>` with the API
 * key and parses the OpenAI-compatible `{ data: [{ id }] }` response.
 */
export function useModelFetch() {
  const [models, setModels] = useState<string[] | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  /**
   * Fetch available models from the provider's discovery endpoint.
   *
   * Takes the provider id rather than a URL and key: the backend resolves both
   * from the stored row, so the key never crosses IPC and the `models_url`
   * fallback rule has one implementation instead of two.
   *
   * @param providerId - The stored provider's id
   * @returns Array of discovered model IDs
   */
  const fetchModels = useCallback(async (providerId: string): Promise<string[]> => {
    setIsLoading(true);
    setError(null);

    try {
      const result = await tauriInvoke("fetch_provider_models", {
        providerId,
        timeoutMs: 15000,
      });
      setModels(result);
      return result;
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      throw error;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const fetchModelCatalog = useCallback(async (providerId: string): Promise<ModelCatalogFetchResult> => {
    setIsLoading(true);
    setError(null);

    try {
      const result = await tauriInvoke("fetch_provider_model_catalog", {
        providerId,
        timeoutMs: 15000,
      });
      setModels(result.models);
      return result;
    } catch (err) {
      const error = err instanceof Error ? err : new Error(String(err));
      setError(error);
      throw error;
    } finally {
      setIsLoading(false);
    }
  }, []);

  return {
    fetchModels,
    fetchModelCatalog,
    isLoading,
    models,
    error,
  };
}

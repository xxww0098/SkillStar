import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";

export interface ModelListEntry {
  id: string;
  ownedBy?: string;
}

interface UseModelFetchReturn {
  models: ModelListEntry[];
  loading: boolean;
  error: string | null;
  fetchModels: (baseUrl: string, apiKey?: string, isFullUrl?: boolean) => Promise<void>;
  clear: () => void;
}

/**
 * Hook to fetch available models from an OpenAI-compatible endpoint.
 * Calls the `fetch_endpoint_models` Tauri command which sends GET to /v1/models.
 */
export function useModelFetch(): UseModelFetchReturn {
  const [models, setModels] = useState<ModelListEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchModels = useCallback(async (baseUrl: string, apiKey?: string, isFullUrl?: boolean) => {
    if (!baseUrl.trim()) {
      setError("请先填写端点 URL");
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const result = await invoke<ModelListEntry[]>("fetch_endpoint_models", {
        baseUrl: baseUrl.trim(),
        apiKey: apiKey?.trim() || null,
        isFullUrl: isFullUrl ?? false,
      });
      setModels(result);
      if (result.length === 0) {
        setError("端点未返回任何模型");
      }
    } catch (e) {
      setError(String(e));
      setModels([]);
    } finally {
      setLoading(false);
    }
  }, []);

  const clear = useCallback(() => {
    setModels([]);
    setError(null);
  }, []);

  return { models, loading, error, fetchModels, clear };
}

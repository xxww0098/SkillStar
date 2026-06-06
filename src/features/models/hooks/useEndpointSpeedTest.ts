import { useCallback, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { EndpointLatencyResult } from "../../../types";

/**
 * Batch-probe provider endpoint URLs (GET `/models` or the URL itself).
 */
export function useEndpointSpeedTest() {
  const [results, setResults] = useState<EndpointLatencyResult[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const testEndpoints = useCallback(async (urls: string[], apiKey?: string) => {
    const trimmed = urls.map((u) => u.trim()).filter(Boolean);
    if (trimmed.length === 0) return [];

    setIsLoading(true);
    try {
      const probeResults = await tauriInvoke("test_endpoints_latency", {
        urls: trimmed,
        apiKey: apiKey?.trim() || null,
        timeoutMs: 10_000,
      });
      setResults(probeResults);
      return probeResults;
    } finally {
      setIsLoading(false);
    }
  }, []);

  const clearResults = useCallback(() => setResults([]), []);

  return { testEndpoints, clearResults, results, isLoading };
}

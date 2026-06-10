import { useCallback, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { ConnectionTestResult, EndpointLatencyResult } from "../../../types";

/**
 * Hook for testing a single provider's connection latency.
 *
 * Sends a minimal chat completion request (1-token response) to verify
 * connectivity and measure round-trip latency. Uses a 10-second timeout.
 *
 * Results are stored in component state (session-only, not persisted).
 */
export function useLatencyTest() {
  const [result, setResult] = useState<ConnectionTestResult | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  /**
   * Test connection to a provider endpoint.
   *
   * @param baseUrl - The provider's base URL
   * @param apiKey - The API key for authentication
   * @param model - The model to test with
   * @param format - The API format ("openai" or "anthropic")
   * @returns Connection test result with status and latency
   */
  const testConnection = useCallback(
    async (
      baseUrl: string,
      apiKey: string,
      model: string,
      format: "openai" | "anthropic",
    ): Promise<ConnectionTestResult> => {
      setIsLoading(true);
      try {
        const testResult = await tauriInvoke("test_provider_connection", {
          baseUrl,
          apiKey,
          model,
          format,
        });
        setResult(testResult);
        return testResult;
      } catch (err) {
        // If invoke itself fails (e.g., command not found), treat as network error
        const errorResult: ConnectionTestResult = {
          status: "network_error",
          error: err instanceof Error ? err.message : String(err),
        };
        setResult(errorResult);
        return errorResult;
      } finally {
        setIsLoading(false);
      }
    },
    [],
  );

  return {
    testConnection,
    isLoading,
    result,
  };
}

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

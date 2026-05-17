import { useCallback, useRef, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { AppId, LatencyResult } from "../../../types";

/**
 * Legacy hook for testing provider latency (per-app architecture).
 *
 * @deprecated Use `useLatencyTest` for the flat provider store (v2) architecture.
 *
 * - `testOne` tests a single provider via the `test_provider_latency` Tauri command.
 * - `testAll` calls `test_all_providers_latency` which runs tests sequentially on the backend
 *   to avoid network contention.
 * - Results are stored in component state (session-only, not persisted to disk).
 * - `isTesting` is true while any test is in progress.
 * - `lastTestedAt` is the ISO timestamp of the most recent completed test.
 */
export function useLatencyTestLegacy() {
  const [results, setResults] = useState<Map<string, LatencyResult>>(new Map());
  const [isTesting, setIsTesting] = useState(false);
  const [lastTestedAt, setLastTestedAt] = useState<string | null>(null);

  // Track active test count so overlapping testOne/testAll calls
  // keep isTesting=true until all finish.
  const activeCount = useRef(0);

  const beginTest = useCallback(() => {
    activeCount.current += 1;
    setIsTesting(true);
  }, []);

  const endTest = useCallback(() => {
    activeCount.current -= 1;
    if (activeCount.current <= 0) {
      activeCount.current = 0;
      setIsTesting(false);
    }
  }, []);

  const testOne = useCallback(
    async (appId: AppId, providerId: string, baseUrl: string, apiKey: string): Promise<LatencyResult> => {
      beginTest();
      try {
        const result = await tauriInvoke("test_provider_latency", {
          app_id: appId,
          provider_id: providerId,
          base_url: baseUrl,
          api_key: apiKey,
        });

        const key = `${appId}:${providerId}`;
        setResults((prev) => {
          const next = new Map(prev);
          next.set(key, result);
          return next;
        });
        setLastTestedAt(result.tested_at);
        return result;
      } finally {
        endTest();
      }
    },
    [beginTest, endTest],
  );

  const testAll = useCallback(
    async (appId: AppId): Promise<LatencyResult[]> => {
      beginTest();
      try {
        const allResults = await tauriInvoke("test_all_providers_latency", {
          app_id: appId,
        });

        setResults((prev) => {
          const next = new Map(prev);
          for (const result of allResults) {
            const key = `${result.app_id}:${result.provider_id}`;
            next.set(key, result);
          }
          return next;
        });

        if (allResults.length > 0) {
          const latest = allResults[allResults.length - 1];
          setLastTestedAt(latest.tested_at);
        }

        return allResults;
      } finally {
        endTest();
      }
    },
    [beginTest, endTest],
  );

  return {
    results,
    testOne,
    testAll,
    isTesting,
    lastTestedAt,
  };
}

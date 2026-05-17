import { useCallback, useEffect, useRef, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { BalanceInfo } from "../../../types";

const BALANCE_TIMEOUT_MS = 10_000;

/**
 * Parse raw balance API response into normalized BalanceInfo based on preset.
 *
 * Each provider returns a different JSON shape:
 * - DeepSeek: { balance_infos: [{ total_balance: "1.23", currency: "CNY" }] }
 * - Kimi: { data: { available_balance: 1.23, total_balance: 5.0, currency: "CNY" } }
 * - OpenRouter: { data: { total_credits: 5.0, usage: 3.5 } } (USD)
 * - SiliconFlow: { data: { balance: "1.23" } } (CNY)
 */
function parseBalanceResponse(presetId: string, raw: unknown): BalanceInfo | null {
  if (!raw || typeof raw !== "object") return null;

  const now = Date.now();
  // biome-ignore lint/suspicious/noExplicitAny: raw JSON parsing
  const data = raw as any;

  switch (presetId) {
    case "deepseek": {
      const info = data?.balance_infos?.[0];
      if (!info) return null;
      const available = Number.parseFloat(info.total_balance ?? info.available_balance ?? "0");
      return {
        available: Number.isFinite(available) ? available : 0,
        currency: info.currency ?? "CNY",
        updated_at: now,
      };
    }
    case "kimi": {
      const d = data?.data ?? data;
      const available = Number.parseFloat(String(d?.available_balance ?? d?.balance ?? "0"));
      const total = d?.total_balance != null ? Number.parseFloat(String(d.total_balance)) : undefined;
      return {
        available: Number.isFinite(available) ? available : 0,
        total: total != null && Number.isFinite(total) ? total : undefined,
        currency: d?.currency ?? "CNY",
        updated_at: now,
      };
    }
    case "openrouter": {
      const d = data?.data ?? data;
      const totalCredits = Number.parseFloat(String(d?.total_credits ?? "0"));
      const usage = Number.parseFloat(String(d?.usage ?? "0"));
      const available = totalCredits - usage;
      return {
        available: Number.isFinite(available) ? available : 0,
        total: Number.isFinite(totalCredits) ? totalCredits : undefined,
        currency: "USD",
        updated_at: now,
      };
    }
    case "siliconflow": {
      const d = data?.data ?? data;
      const available = Number.parseFloat(String(d?.balance ?? "0"));
      return {
        available: Number.isFinite(available) ? available : 0,
        currency: "CNY",
        updated_at: now,
      };
    }
    default:
      return null;
  }
}

/**
 * Hook for querying provider balance/quota asynchronously.
 *
 * Auto-fetches when presetId and apiKey are both non-empty.
 * Uses a 10-second timeout and does not block panel rendering.
 *
 * @param presetId - The provider's preset ID (determines which balance API to use)
 * @param apiKey - The provider's API key
 * @param baseUrl - The provider's base URL
 */
export function useBalanceQuery(presetId: string | null, apiKey: string, baseUrl: string) {
  const [balance, setBalance] = useState<BalanceInfo | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  // Track the latest request to avoid stale updates
  const requestIdRef = useRef(0);

  const fetchBalance = useCallback(async () => {
    if (!presetId || !apiKey) {
      setBalance(null);
      setError(null);
      return;
    }

    const currentRequestId = ++requestIdRef.current;
    setIsLoading(true);
    setError(null);

    try {
      const result = await Promise.race([
        tauriInvoke("query_provider_balance", {
          presetId,
          apiKey,
          baseUrl,
        }),
        new Promise<never>((_, reject) => setTimeout(() => reject(new Error("查询超时")), BALANCE_TIMEOUT_MS)),
      ]);

      // Only update state if this is still the latest request
      if (currentRequestId === requestIdRef.current) {
        const parsed = parseBalanceResponse(presetId, result);
        if (parsed) {
          setBalance(parsed);
        } else {
          setError(new Error("无法解析余额数据"));
          setBalance(null);
        }
      }
    } catch (err) {
      if (currentRequestId === requestIdRef.current) {
        const error = err instanceof Error ? err : new Error(String(err));
        setError(error);
        setBalance(null);
      }
    } finally {
      if (currentRequestId === requestIdRef.current) {
        setIsLoading(false);
      }
    }
  }, [presetId, apiKey, baseUrl]);

  // Auto-fetch when presetId + apiKey are non-empty
  useEffect(() => {
    if (presetId && apiKey) {
      fetchBalance();
    } else {
      setBalance(null);
      setError(null);
      setIsLoading(false);
    }
  }, [presetId, apiKey, fetchBalance]);

  return {
    balance,
    isLoading,
    error,
    refresh: fetchBalance,
  };
}

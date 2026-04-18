import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import type { ProviderEntry } from "./useModelProviders";

export type HealthStatus = "unknown" | "checking" | "healthy" | "degraded" | "down";

export interface ProviderHealth {
  status: HealthStatus;
  /** Latency in milliseconds; null when status is 'down' or 'unknown' */
  latencyMs: number | null;
  /** Optional quota percentage (0–100) from Codex-style quota data */
  quotaPct: number | null;
  /** Human-readable error string when status is 'down' */
  error?: string;
}

export interface UseProviderHealthReturn {
  health: ProviderHealth;
  check: () => Promise<void>;
}

const HEALTH_TIMEOUT_MS = 6000;

/**
 * Derives a base URL and API key from a ProviderEntry for health checking.
 * Returns null if insufficient config to perform a check.
 */
function extractHealthConfig(provider: ProviderEntry): { baseUrl: string; apiKey: string } | null {
  const cfg = provider.settingsConfig as Record<string, unknown>;
  const env = cfg?.env as Record<string, unknown> | undefined;

  let baseUrl = "";
  let apiKey = "";

  if (env) {
    // Claude
    if (env.ANTHROPIC_BASE_URL) baseUrl = (env.ANTHROPIC_BASE_URL as string).trim();
    if (env.ANTHROPIC_AUTH_TOKEN) apiKey = env.ANTHROPIC_AUTH_TOKEN as string;
    else if (env.ANTHROPIC_API_KEY) apiKey = env.ANTHROPIC_API_KEY as string;

    // Gemini
    if (!baseUrl && env.GOOGLE_GEMINI_BASE_URL) baseUrl = (env.GOOGLE_GEMINI_BASE_URL as string).trim();
    if (!apiKey && env.GEMINI_API_KEY) apiKey = env.GEMINI_API_KEY as string;
  }

  // Codex TOML config
  if (!baseUrl) {
    const configStr = cfg?.config as string | undefined;
    if (configStr) {
      const match = configStr.match(/base_url\s*=\s*"([^"]+)"/);
      if (match) baseUrl = match[1].trim();
    }
  }

  // Direct options (custom providers)
  if (!baseUrl) {
    const options = cfg?.options as Record<string, unknown> | undefined;
    if (options?.baseURL) baseUrl = (options.baseURL as string).trim();
    if (!apiKey && options?.apiKey) apiKey = options.apiKey as string;
  }

  // OpenCode native auth
  if (!baseUrl && provider.meta?.baseURL) {
    baseUrl = provider.meta.baseURL;
  }

  const auth = cfg?.auth as Record<string, unknown> | undefined;
  if (!apiKey && auth) {
    if (auth.key) apiKey = auth.key as string;
  }

  if (!baseUrl) return null;
  return { baseUrl, apiKey };
}

/**
 * Performs a lightweight health probe (GET /v1/models with a short timeout).
 * Returns latency in ms and a rough status.
 */
async function probeEndpoint(baseUrl: string, apiKey: string): Promise<{ latencyMs: number; ok: boolean }> {
  const url = `${baseUrl.replace(/\/$/, "")}/v1/models`;
  const headers: Record<string, string> = {};
  if (apiKey) headers.Authorization = `Bearer ${apiKey}`;

  const start = performance.now();
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), HEALTH_TIMEOUT_MS);

  try {
    const res = await fetch(url, {
      method: "GET",
      headers,
      signal: controller.signal,
      cache: "no-store",
    });
    clearTimeout(timeoutId);
    return { latencyMs: Math.round(performance.now() - start), ok: res.ok };
  } catch {
    clearTimeout(timeoutId);
    // AbortError means timeout
    return { latencyMs: Math.round(performance.now() - start), ok: false };
  }
}

/**
 * Hook that tracks health, latency and quota for a single provider.
 *
 * Usage:
 *   const { health, check } = useProviderHealth(provider, appId, provider.id);
 *
 * Call `check()` to trigger a live probe. The hook:
 * 1. Loads cached quota immediately (fast, no loading state)
 * 2. Probes health endpoint concurrently
 * 3. Refreshes quota from backend in background
 *
 * quotaPct is populated from backend quota API (OpenAI / Gemini / etc.)
 */

interface QuotaResponse {
  providerId: string;
  appId: string;
  usagePercent: number | null;
  remaining: string | null;
  resetTime: string | null;
  planName: string | null;
  fetchedAt: number;
  error: string | null;
}

export function useProviderHealth(
  provider: ProviderEntry,
  appId: string,
  _providerId: string,
): UseProviderHealthReturn {
  const [health, setHealth] = useState<ProviderHealth>({
    status: "unknown",
    latencyMs: null,
    quotaPct: null,
  });

  const check = useCallback(async () => {
    if (health.status === "checking") return;

    const config = extractHealthConfig(provider);
    if (!config) {
      setHealth((prev) => ({ ...prev, status: "unknown", latencyMs: null }));
      return;
    }

    setHealth((prev) => ({ ...prev, status: "checking" }));

    // Load cached quota immediately (no loading transition — fast path)
    invoke<QuotaResponse | null>("get_cached_provider_quota", {
      providerId: provider.id,
      appId,
    })
      .then((cached) => {
        if (cached && cached.usagePercent !== null) {
          setHealth((prev) => ({ ...prev, quotaPct: cached.usagePercent }));
        }
      })
      .catch(() => {
        /* ignore */
      });

    // Probe health + refresh quota concurrently
    const [probeResult, _quotaResult] = await Promise.all([
      probeEndpoint(config.baseUrl, config.apiKey).catch(() => ({
        latencyMs: null,
        ok: false,
      })),
      invoke<QuotaResponse>("check_provider_quota", {
        providerId: provider.id,
        appId,
      }).catch(() => null),
    ]);

    const { latencyMs, ok } = probeResult;

    let status: ProviderHealth["status"];
    if (!ok) {
      status = "down";
    } else if (latencyMs !== null && latencyMs > 2000) {
      status = "degraded";
    } else {
      status = "healthy";
    }

    setHealth((prev) => ({
      ...prev,
      status,
      latencyMs,
      quotaPct: _quotaResult?.usagePercent ?? prev.quotaPct,
    }));
  }, [provider, appId, health.status]);

  return { health, check };
}

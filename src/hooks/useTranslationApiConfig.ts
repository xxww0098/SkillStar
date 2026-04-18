import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { TranslationApiConfig } from "../types";

const DEFAULT_CONFIG: TranslationApiConfig = {
  deepl_key: "",
  deeplx_key: "",
  deeplx_url: "",
};

const CONFIG_CACHE_TTL_MS = 3_000;

let _cachedConfig: TranslationApiConfig | null = null;
let _cachedAt = 0;
let _inflight: Promise<TranslationApiConfig> | null = null;

export async function getTranslationConfigCached(): Promise<TranslationApiConfig> {
  const now = Date.now();
  if (_cachedConfig && now - _cachedAt < CONFIG_CACHE_TTL_MS) {
    return _cachedConfig;
  }
  if (_inflight) return _inflight;
  _inflight = invoke<TranslationApiConfig>("get_translation_api_config")
    .then((cfg) => {
      _cachedConfig = cfg;
      _cachedAt = Date.now();
      return cfg;
    })
    .catch(() => {
      return DEFAULT_CONFIG;
    })
    .finally(() => {
      _inflight = null;
    });
  return _inflight;
}

export function invalidateTranslationConfigCache() {
  _cachedConfig = null;
  _cachedAt = 0;
}

export function useTranslationApiConfig() {
  const [config, setConfig] = useState<TranslationApiConfig>(DEFAULT_CONFIG);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getTranslationConfigCached()
      .then((cfg) => setConfig({ ...DEFAULT_CONFIG, ...cfg }))
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  const saveConfig = useCallback(async (newConfig: TranslationApiConfig) => {
    await invoke("save_translation_api_config", { config: newConfig });
    invalidateTranslationConfigCache();
    setConfig(newConfig);
  }, []);

  const testProvider = useCallback(
    async (provider: string): Promise<{ ok: boolean; latency: number | null; error: string | null }> => {
      try {
        const latency = await invoke<number>("test_translation_provider", { provider });
        return { ok: true, latency, error: null };
      } catch (e) {
        return { ok: false, latency: null, error: String(e) };
      }
    },
    [],
  );

  return {
    config,
    loading,
    saveConfig,
    testProvider,
  };
}

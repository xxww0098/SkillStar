import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { TranslationApiConfig } from "../types";

const DEFAULT_CONFIG: TranslationApiConfig = {
  deepl_key: "",
  deeplx_url: "",
  google_key: "",
  azure_key: "",
  azure_region: "eastasia",
  gtx_api_key: "",
  deepseek_key: "",
  claude_key: "",
  openai_key: "",
  gemini_key: "",
  perplexity_key: "",
  azure_openai_key: "",
  siliconflow_key: "",
  groq_key: "",
  openrouter_key: "",
  nvidia_key: "",
  custom_llm_key: "",
  custom_llm_base_url: "http://127.0.0.1:11434/v1",
  deepseek_settings: { api_key: "", model: "deepseek-chat", temperature: 0.7 },
  claude_settings: { api_key: "", model: "claude-sonnet-4-6", temperature: 0.7 },
  openai_settings: { api_key: "", model: "gpt-5.4", temperature: 1 },
  gemini_settings: { api_key: "", model: "gemini-2.0-flash", temperature: 0.7 },
  perplexity_settings: { api_key: "", model: "sonar", temperature: 0.7 },
  azure_openai_settings: { api_key: "", model: "gpt-5-mini", temperature: 0.7 },
  siliconflow_settings: { api_key: "", model: "deepseek-ai/DeepSeek-V3", temperature: 0.7 },
  groq_settings: { api_key: "", model: "openai/gpt-oss-20b", temperature: 0.7 },
  openrouter_settings: { api_key: "", model: "nvidia/nemotron-3-super-120b-a12b:free", temperature: 0.7 },
  nvidia_settings: { api_key: "", model: "deepseek-ai/deepseek-v3.2", temperature: 0.7 },
  custom_llm_settings: { api_key: "", model: "llama3.2", temperature: 0.7 },
  enabled_providers: [],
  default_provider: "deepl",
  default_skill_provider: "deepseek",
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

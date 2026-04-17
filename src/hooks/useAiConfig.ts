import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { AiConfig } from "../types";

const DEFAULT_CONFIG: AiConfig = {
  enabled: false,
  api_format: "openai",
  base_url: "",
  api_key: "",
  model: "gpt-5.4",
  target_language: "zh-CN",
  short_text_priority: "ai_first",
  context_window_k: 128,
  max_concurrent_requests: 4,
  chunk_char_limit: 0,
  scan_max_response_tokens: 0,
  security_scan_telemetry_enabled: false,
  openai_preset: { base_url: "", api_key: "", model: "" },
  anthropic_preset: { base_url: "", api_key: "", model: "" },
  local_preset: { base_url: "http://127.0.0.1:11434/v1", api_key: "", model: "llama3.1:8b" },
};

// ── Module-level config singleton ───────────────────────────────────
//
// Deduplicates concurrent `get_ai_config` IPC calls from multiple hooks
// (e.g. 2× useAiStream + AiPickSkillsModal mounting at the same time).
// Cached for 3 seconds — long enough to cover a single render cycle,
// short enough to always reflect saves within the same session.

const CONFIG_CACHE_TTL_MS = 3_000;

let _cachedConfig: AiConfig | null = null;
let _cachedAt = 0;
let _inflight: Promise<AiConfig> | null = null;

/**
 * Get AI config with deduplication and short TTL cache.
 *
 * Multiple callers within the same ~3s window share a single IPC call.
 * Call `invalidateAiConfigCache()` after saving to force a fresh read.
 */
export async function getAiConfigCached(): Promise<AiConfig> {
  const now = Date.now();
  if (_cachedConfig && now - _cachedAt < CONFIG_CACHE_TTL_MS) {
    return _cachedConfig;
  }
  if (_inflight) return _inflight;
  _inflight = invoke<AiConfig>("get_ai_config")
    .then((cfg) => {
      _cachedConfig = cfg;
      _cachedAt = Date.now();
      return cfg;
    })
    .finally(() => {
      _inflight = null;
    });
  return _inflight;
}

/** Invalidate the module-level config cache (call after save). */
export function invalidateAiConfigCache() {
  _cachedConfig = null;
  _cachedAt = 0;
}

export function useAiConfig() {
  const [config, setConfig] = useState<AiConfig>(DEFAULT_CONFIG);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getAiConfigCached()
      .then((cfg) => setConfig({ ...DEFAULT_CONFIG, ...cfg }))
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  const saveConfig = useCallback(async (newConfig: AiConfig) => {
    await invoke("save_ai_config", { config: newConfig });
    invalidateAiConfigCache();
    setConfig(newConfig);
  }, []);

  const translateSkill = useCallback(async (content: string): Promise<string> => {
    return invoke<string>("ai_translate_skill", { content });
  }, []);

  const summarizeSkill = useCallback(async (content: string): Promise<string> => {
    return invoke<string>("ai_summarize_skill", { content });
  }, []);

  const testConnection = useCallback(async (): Promise<number> => {
    return invoke<number>("ai_test_connection");
  }, []);

  return {
    config,
    loading,
    saveConfig,
    translateSkill,
    summarizeSkill,
    testConnection,
  };
}

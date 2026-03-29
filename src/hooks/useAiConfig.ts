import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AiConfig } from "../types";

const DEFAULT_CONFIG: AiConfig = {
  enabled: false,
  api_format: "openai",
  base_url: "https://api.openai.com/v1",
  api_key: "",
  model: "gpt-5.4",
  target_language: "zh-CN",
};

export function useAiConfig() {
  const [config, setConfig] = useState<AiConfig>(DEFAULT_CONFIG);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke<AiConfig>("get_ai_config")
      .then((cfg) => setConfig({ ...DEFAULT_CONFIG, ...cfg }))
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  const saveConfig = useCallback(async (newConfig: AiConfig) => {
    await invoke("save_ai_config", { config: newConfig });
    setConfig(newConfig);
  }, []);

  const translateSkill = useCallback(async (content: string): Promise<string> => {
    return invoke<string>("ai_translate_skill", { content });
  }, []);

  const summarizeSkill = useCallback(async (content: string): Promise<string> => {
    return invoke<string>("ai_summarize_skill", { content });
  }, []);

  const testConnection = useCallback(async (): Promise<string> => {
    return invoke<string>("ai_test_connection");
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

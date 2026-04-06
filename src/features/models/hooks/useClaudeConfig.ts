import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

interface ClaudeEnv {
  ANTHROPIC_AUTH_TOKEN?: string;
  ANTHROPIC_API_KEY?: string;
  ANTHROPIC_BASE_URL?: string;
  ANTHROPIC_MODEL?: string;
  ANTHROPIC_REASONING_MODEL?: string;
  ANTHROPIC_DEFAULT_HAIKU_MODEL?: string;
  ANTHROPIC_DEFAULT_SONNET_MODEL?: string;
  ANTHROPIC_DEFAULT_OPUS_MODEL?: string;
  [key: string]: string | undefined;
}

interface ClaudeConfigState {
  loading: boolean;
  saving: boolean;
  exists: boolean;
  env: ClaudeEnv;
  raw: Record<string, unknown> | null;
}

export function useClaudeConfig() {
  const [state, setState] = useState<ClaudeConfigState>({
    loading: true,
    saving: false,
    exists: false,
    env: {},
    raw: null,
  });

  const load = useCallback(async () => {
    setState((s) => ({ ...s, loading: true }));
    try {
      const config = await invoke<Record<string, unknown> | null>("get_claude_model_config");
      if (config && typeof config === "object") {
        const env = (config.env as ClaudeEnv) || {};
        setState({ loading: false, saving: false, exists: true, env, raw: config });
      } else {
        setState({ loading: false, saving: false, exists: false, env: {}, raw: null });
      }
    } catch {
      setState({ loading: false, saving: false, exists: false, env: {}, raw: null });
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const save = useCallback(
    async (env: ClaudeEnv) => {
      setState((s) => ({ ...s, saving: true }));
      try {
        // Merge env into existing config, preserving other fields
        const config = { ...(state.raw || {}), env };
        await invoke("save_claude_model_config", { config });
        setState((s) => ({ ...s, saving: false, exists: true, env, raw: config }));
        toast.success("Claude Code 配置已保存");
      } catch (e) {
        setState((s) => ({ ...s, saving: false }));
        toast.error(`保存失败: ${e}`);
      }
    },
    [state.raw],
  );

  const updateEnv = useCallback((key: string, value: string) => {
    setState((s) => ({
      ...s,
      env: { ...s.env, [key]: value },
    }));
  }, []);

  const applyPreset = useCallback((env: Record<string, string>) => {
    setState((s) => ({
      ...s,
      env: { ...env, ANTHROPIC_AUTH_TOKEN: s.env.ANTHROPIC_AUTH_TOKEN || "" },
    }));
  }, []);

  return { ...state, load, save, updateEnv, applyPreset };
}

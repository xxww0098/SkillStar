import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

interface OpenCodeConfigState {
  loading: boolean;
  saving: boolean;
  exists: boolean;
  config: Record<string, unknown> | null;
}

export function useOpenCodeConfig() {
  const [state, setState] = useState<OpenCodeConfigState>({
    loading: true,
    saving: false,
    exists: false,
    config: null,
  });

  const load = useCallback(async () => {
    setState((s) => ({ ...s, loading: true }));
    try {
      const config = await invoke<Record<string, unknown> | null>("get_opencode_model_config");
      if (config && typeof config === "object") {
        setState({ loading: false, saving: false, exists: true, config });
      } else {
        setState({ loading: false, saving: false, exists: false, config: null });
      }
    } catch {
      setState({ loading: false, saving: false, exists: false, config: null });
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const save = useCallback(async (config: Record<string, unknown>) => {
    setState((s) => ({ ...s, saving: true }));
    try {
      await invoke("save_opencode_model_config", { config });
      setState((s) => ({ ...s, saving: false, exists: true, config }));
      toast.success("OpenCode 配置已保存");
    } catch (e) {
      setState((s) => ({ ...s, saving: false }));
      toast.error(`保存失败: ${e}`);
    }
  }, []);

  const updateConfig = useCallback((updater: (prev: Record<string, unknown>) => Record<string, unknown>) => {
    setState((s) => ({
      ...s,
      config: updater(s.config || {}),
    }));
  }, []);

  return { ...state, load, save, updateConfig };
}

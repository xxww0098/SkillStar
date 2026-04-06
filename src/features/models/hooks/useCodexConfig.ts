import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

interface AuthStatus {
  hasChatgptSession: boolean;
  configuredKeys: Record<string, boolean>;
}

interface CodexConfigState {
  loading: boolean;
  saving: boolean;
  exists: boolean;
  configText: string;
  /** Full auth.json for reading API key values into input fields */
  authJson: Record<string, string>;
  /** Structured auth status (OAuth + key presence) */
  authStatus: AuthStatus;
}

export function useCodexConfig() {
  const [state, setState] = useState<CodexConfigState>({
    loading: true,
    saving: false,
    exists: false,
    configText: "",
    authJson: {},
    authStatus: { hasChatgptSession: false, configuredKeys: {} },
  });

  // Track which auth fields were modified this session (for merge-only save)
  const dirtyAuthFields = useRef<Set<string>>(new Set());

  const load = useCallback(async () => {
    setState((s) => ({ ...s, loading: true }));
    try {
      const [configText, authJson, authStatus] = await Promise.all([
        invoke<string>("get_codex_model_config"),
        invoke<Record<string, string>>("get_codex_auth").catch(() => ({})),
        invoke<AuthStatus>("get_codex_auth_status").catch(() => ({
          hasChatgptSession: false,
          configuredKeys: {},
        })),
      ]);
      dirtyAuthFields.current.clear();
      setState({
        loading: false,
        saving: false,
        exists: !!configText,
        configText: configText || "",
        authJson: authJson || {},
        authStatus,
      });
    } catch {
      setState({
        loading: false,
        saving: false,
        exists: false,
        configText: "",
        authJson: {},
        authStatus: { hasChatgptSession: false, configuredKeys: {} },
      });
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const save = useCallback(async (configText: string, authJson: Record<string, string>) => {
    setState((s) => ({ ...s, saving: true }));
    try {
      // Only send modified auth fields (merge-not-overwrite to protect OAuth tokens)
      const dirtyFields: Record<string, string> = {};
      for (const key of dirtyAuthFields.current) {
        dirtyFields[key] = authJson[key] ?? "";
      }

      const promises: Promise<unknown>[] = [invoke("save_codex_model_config", { configText })];
      // Only call save_codex_auth if there are dirty fields
      if (Object.keys(dirtyFields).length > 0) {
        promises.push(invoke("save_codex_auth", { fields: dirtyFields }));
      }
      await Promise.all(promises);

      dirtyAuthFields.current.clear();
      setState((s) => ({ ...s, saving: false, exists: true, configText, authJson }));
      toast.success("Codex 配置已保存");
    } catch (e) {
      setState((s) => ({ ...s, saving: false }));
      toast.error(`保存失败: ${e}`);
    }
  }, []);

  const setConfigText = useCallback((text: string) => {
    setState((s) => ({ ...s, configText: text }));
  }, []);

  const updateAuthField = useCallback((key: string, value: string) => {
    dirtyAuthFields.current.add(key);
    setState((s) => ({
      ...s,
      authJson: {
        ...s.authJson,
        [key]: value,
      },
    }));
  }, []);

  const applyPreset = useCallback((config: string) => {
    setState((s) => ({
      ...s,
      configText: config,
    }));
  }, []);

  return { ...state, load, save, setConfigText, applyPreset, updateAuthField };
}

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import type { ModelAppId } from "../components/AppCapsuleSwitcher";
import { opencodePresets } from "../presets/opencodePresets";

/** Flexible metadata for provider-specific extensions */
export interface ProviderMeta {
  /** Claude: which API wire format the provider expects */
  apiFormat?: "anthropic" | "openai_chat" | "openai_responses";
  /** Claude: which env key holds the API key */
  apiKeyField?: "ANTHROPIC_AUTH_TOKEN" | "ANTHROPIC_API_KEY";
  /** Whether the base URL is a full URL (skip appending /v1/messages etc.) */
  isFullUrl?: boolean;
  /** Extracted baseURL used for API testing */
  baseURL?: string;
}

/** A named provider configuration entry */
export interface ProviderEntry {
  id: string;
  name: string;
  category: "official" | "cn_official" | "cloud_provider" | "aggregator" | "third_party" | "custom";
  /** The config payload written to the app's config file when this provider is activated */
  settingsConfig: Record<string, unknown>;
  websiteUrl?: string;
  apiKeyUrl?: string;
  iconColor?: string;
  notes?: string;
  createdAt?: number;
  sortIndex?: number;
  /** Extensible metadata bag */
  meta?: ProviderMeta;
}

export interface ProvidersState {
  loading: boolean;
  saving: boolean;
  providers: Record<string, ProviderEntry>;
  currentId: string | null;
}

/**
 * Core hook that manages a list of named provider configurations for one app.
 * Providers and current selection are persisted via backend commands.
 */
export function useModelProviders(appId: ModelAppId) {
  const [state, setState] = useState<ProvidersState>({
    loading: true,
    saving: false,
    providers: {},
    currentId: null,
  });

  // ── Load ─────────────────────────────────────────
  const load = useCallback(
    async (showLoading: boolean = true) => {
      if (showLoading) {
        setState((s) => ({ ...s, loading: true }));
      }
      try {
        const result = await invoke<{
          providers: Record<string, ProviderEntry>;
          current: string | null;
        }>("get_model_providers", { appId });
        setState((s) => ({
          ...s,
          loading: false,
          saving: false,
          providers: result.providers || {},
          currentId: result.current,
        }));
      } catch {
        setState((s) => ({ ...s, loading: false, saving: false, providers: {}, currentId: null }));
      }
    },
    [appId],
  );

  useEffect(() => {
    load();

    const unlisten = listen<{ appId: string; providerId: string }>("model-config://switched", (event) => {
      if (event.payload.appId === appId) {
        toast.success(
          "托盘快速切换成功",
          appId === "codex"
            ? {
                description: "如 Codex 正在运行，请手动重启以使新 API 配置生效",
                duration: 5000,
              }
            : undefined,
        );
        load(false); // Background reload to avoid flicker
      }
    });

    // Listen for cross-component refresh (e.g. OAuth account switch clears provider current)
    const handleProvidersRefresh = () => load(false);
    window.addEventListener("model-providers-refresh", handleProvidersRefresh);

    return () => {
      unlisten.then((f) => f());
      window.removeEventListener("model-providers-refresh", handleProvidersRefresh);
    };
  }, [load, appId]);

  // ── Switch ─────────────────────────────────────────
  const switchTo = useCallback(
    async (providerId: string) => {
      setState((s) => ({ ...s, saving: true }));
      try {
        await invoke("switch_model_provider", { appId, providerId });
        setState((s) => ({ ...s, saving: false, currentId: providerId }));
        const provider = state.providers[providerId];
        toast.success(
          `已切换到 ${provider?.name || providerId}`,
          appId === "codex"
            ? {
                description: "如 Codex 正在运行，请手动重启以使新 API 配置生效",
                duration: 5000,
              }
            : undefined,
        );
        // For Codex: refresh OAuth account list since current_account_id was cleared
        if (appId === "codex") {
          window.dispatchEvent(new CustomEvent("codex-accounts-refresh"));
        }
        window.dispatchEvent(new CustomEvent("skillstar_config_changed"));
      } catch (e) {
        setState((s) => ({ ...s, saving: false }));
        toast.error(`切换失败: ${e}`);
      }
    },
    [appId, state.providers],
  );

  // ── Add ─────────────────────────────────────────
  const addProvider = useCallback(
    async (entry: ProviderEntry) => {
      setState((s) => ({ ...s, saving: true }));
      try {
        await invoke("add_model_provider", { appId, provider: entry });
        setState((s) => ({
          ...s,
          saving: false,
          providers: { ...s.providers, [entry.id]: entry },
        }));
        toast.success(`已添加 ${entry.name}`);
        window.dispatchEvent(new CustomEvent("skillstar_config_changed"));
      } catch (e) {
        setState((s) => ({ ...s, saving: false }));
        toast.error(`添加失败: ${e}`);
      }
    },
    [appId],
  );

  // ── Delete ─────────────────────────────────────────
  const deleteProvider = useCallback(
    async (providerId: string) => {
      setState((s) => ({ ...s, saving: true }));
      try {
        await invoke("delete_model_provider", { appId, providerId });
        setState((s) => {
          const next = { ...s.providers };
          delete next[providerId];
          return {
            ...s,
            saving: false,
            providers: next,
            currentId: s.currentId === providerId ? null : s.currentId,
          };
        });
        toast.success("已删除");
        window.dispatchEvent(new CustomEvent("skillstar_config_changed"));
      } catch (e) {
        setState((s) => ({ ...s, saving: false }));
        toast.error(`删除失败: ${e}`);
      }
    },
    [appId],
  );

  // ── Update ─────────────────────────────────────────
  const updateProvider = useCallback(
    async (entry: ProviderEntry) => {
      setState((s) => ({ ...s, saving: true }));
      try {
        await invoke("update_model_provider", { appId, provider: entry });
        setState((s) => ({
          ...s,
          saving: false,
          providers: { ...s.providers, [entry.id]: entry },
        }));
        toast.success(`已更新 ${entry.name}`);
        window.dispatchEvent(new CustomEvent("skillstar_config_changed"));
      } catch (e) {
        setState((s) => ({ ...s, saving: false }));
        toast.error(`更新失败: ${e}`);
      }
    },
    [appId],
  );

  // ── Reorder ─────────────────────────────────────────
  const reorderProviders = useCallback(
    async (providerIds: string[]) => {
      // Optimistic update
      setState((s) => {
        const newProviders = { ...s.providers };
        providerIds.forEach((id, index) => {
          if (newProviders[id]) {
            newProviders[id] = { ...newProviders[id], sortIndex: index };
          }
        });
        return { ...s, providers: newProviders };
      });

      try {
        await invoke("reorder_model_providers", { appId, providerIds });
      } catch (e) {
        toast.error(`排序失败: ${e}`);
        load(false); // Reload on failure without flicker
      }
    },
    [appId, load],
  );

  // ── Sorted list ─────────────────────────────────────────
  const sortedProviders = useMemo(() => {
    return Object.values(state.providers).sort((a, b) => (a.sortIndex ?? 999) - (b.sortIndex ?? 999));
  }, [state.providers]);

  return {
    ...state,
    sortedProviders,
    load,
    switchTo,
    addProvider,
    deleteProvider,
    updateProvider,
    reorderProviders,
  };
}

/**
 * Native hook for OpenCode that completely bypasses the local `model_providers.json` system
 * and reads/writes directly to `auth.json` (OpenCode CLI Native).
 */
export function useOpenCodeNativeProviders() {
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [authProviders, setAuthProviders] = useState<Record<string, unknown>>({});

  const load = useCallback(async (showLoadingSpinner: boolean = true) => {
    if (showLoadingSpinner) setLoading(true);
    try {
      const data = await invoke<Record<string, unknown>>("get_opencode_auth_providers");
      setAuthProviders(data);
    } catch (e) {
      console.error(e);
      setAuthProviders({});
    } finally {
      if (showLoadingSpinner) setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  // Convert authProviders into ProviderEntry shapes for the UI
  const fetchedProviders = useMemo(() => {
    const result: Record<string, ProviderEntry> = {};
    let sortIndex = 0;

    for (const [key, val] of Object.entries(authProviders)) {
      // Find preset for metadata
      const presetNameLower = key.toLowerCase();

      // Map native CLI vendor names back to UI preset names
      const nativeToUiMap: Record<string, string> = {
        alibaba: "bailian",
        bytedance: "doubaoseed",
        moonshot: "kimi_k2_5",
        zhipu: "zhipu_glm",
        "bailian-coding-plan": "bailian",
        minimax_cn: "minimax",
      };

      const searchName = nativeToUiMap[presetNameLower] || presetNameLower;

      // Try to match preset by ID or Name
      const preset = opencodePresets.find(
        (p) =>
          p.name.toLowerCase().replace(/[^a-z0-9]/g, "_") === searchName ||
          p.name.toLowerCase() === searchName ||
          p.name.toLowerCase().replace(/[^a-z0-9]/g, "_") === presetNameLower,
      );

      result[key] = {
        id: key,
        name: preset?.name || key,
        category: preset?.category || "custom",
        websiteUrl: preset?.websiteUrl,
        iconColor: preset?.iconColor,
        settingsConfig: { auth: val }, // Just store raw auth data here for viewing
        meta: {
          baseURL: preset ? (preset.settingsConfig.options?.baseURL as string) || "" : "",
        },
        createdAt: Date.now(),
        sortIndex: sortIndex++,
      };
    }
    return result;
  }, [authProviders]);

  const providers = fetchedProviders;

  const sortedProviders = useMemo(() => {
    return Object.values(providers).sort((a, b) => (a.sortIndex ?? 999) - (b.sortIndex ?? 999));
  }, [providers]);

  // For OpenCode, there is no "Current Provider" active switch. It pools all models.
  const currentId = null;

  const switchTo = useCallback(async (_providerId: string) => {
    // No-op for OpenCode native
    toast.success("已标记。OpenCode 使用模型选择器选择具体模型。");
  }, []);

  const addProvider = useCallback(async (_entry: ProviderEntry) => {
    // OpenCode no longer supports adding providers from the UI
    toast.info("请通过配置文件或 OpenCode CLI 管理供应商授权");
  }, []);

  const deleteProvider = useCallback(
    async (providerId: string) => {
      setSaving(true);
      try {
        const provider = providers[providerId];
        const isEnv = (provider?.settingsConfig?.auth as Record<string, unknown>)?.type === "env";
        const isCustom = (provider?.settingsConfig?.auth as Record<string, unknown>)?.type === "custom";

        let finalProviderId = providerId;
        if (!isCustom) {
          const providerIdCleaned = providerId.split("_")[0];
          const uiToNativeMap: Record<string, string> = {
            bailian: "alibaba",
            doubaoseed: "bytedance",
            doubao: "bytedance",
            kimi: "moonshot",
            zhipu: "zhipu",
            minimax: "minimax",
            "bailian-coding-plan": "bailian",
          };
          finalProviderId = uiToNativeMap[providerIdCleaned] || providerIdCleaned;
        }

        await invoke("remove_opencode_auth_provider", {
          provider: finalProviderId,
          isEnv,
          isCustom,
        });
        await load(false);
        toast.success(
          isEnv
            ? "成功屏蔽系统环境变量配置"
            : isCustom
              ? "成功移除本地第三方代理端点"
              : "成功移除系统内的内置服务商授权",
        );
      } catch (e) {
        toast.error(`移除失败: ${e}`);
      } finally {
        setSaving(false);
      }
    },
    [load, providers],
  );

  const updateProvider = useCallback(
    async (entry: ProviderEntry) => {
      // The user is updating key/settings on an existing provider card
      const key = (entry.settingsConfig as Record<string, unknown>).tempKey as string;
      const authData = entry.settingsConfig?.auth as Record<string, string> | undefined;
      const actualKey = key || authData?.key;

      if (!actualKey || actualKey.trim() === "") {
        toast.error("未配置 API Key，无法保存");
        return;
      }

      setSaving(true);
      try {
        // Native mapping for common providers in OpenCode CLI
        let providerId = entry.id;
        if (providerId.includes("_")) {
          providerId = providerId.split("_")[0];
        }

        const uiToNativeMap: Record<string, string> = {
          bailian: "alibaba",
          doubaoseed: "bytedance",
          doubao: "bytedance",
          kimi: "moonshot",
          zhipu: "zhipu",
        };
        const nativeProviderId = uiToNativeMap[providerId];

        if (nativeProviderId) {
          await invoke("add_opencode_auth_provider", { provider: nativeProviderId, key: actualKey.trim() });
        } else if (providerId === "minimax" || providerId === "minimax_custom") {
          const providerConfig = entry.settingsConfig?.provider as Record<string, unknown> | undefined;
          const baseURL = "https://api.minimaxi.com/v1";

          const models: Record<string, { name: string }> = {};
          try {
            const fetched = await invoke<{ id: string; owned_by?: string }[]>("fetch_endpoint_models", {
              baseUrl: baseURL,
              apiKey: actualKey.trim(),
              isFullUrl: false,
            });
            for (const m of fetched) {
              models[m.id] = { name: m.id };
            }
          } catch {
            // API fetch failed — leave models empty
          }

          let configToSave: any = {
            npm: "@ai-sdk/openai-compatible",
            name: entry.name,
            options: { baseURL, apiKey: actualKey.trim() },
            models,
          };

          if (providerConfig && providerConfig.minimax) {
            configToSave = JSON.parse(JSON.stringify(providerConfig.minimax));
            if (!configToSave.options) configToSave.options = {};
            configToSave.options.apiKey = actualKey.trim();
          }

          await invoke("set_opencode_setting", { key: "provider.minimax_cn", value: configToSave });

          const firstModelId = Object.keys(configToSave.models || {})[0];
          if (firstModelId) {
            await invoke("set_opencode_setting", {
              key: "model",
              value: `minimax_cn/${firstModelId}`,
            });
          }
        } else {
          await invoke("add_opencode_auth_provider", { provider: providerId, key: actualKey.trim() });
        }

        await load(false);
        toast.success(`已配置 ${entry.name} 授权`);
      } catch (e) {
        toast.error(`配置失败: ${e}`);
      } finally {
        setSaving(false);
      }
    },
    [load],
  );

  const reorderProviders = useCallback(async (_providerIds: string[]) => {
    // OpenCode auth.json order doesn't matter, ignore.
  }, []);

  return {
    loading,
    saving,
    providers,
    sortedProviders,
    currentId,
    load,
    switchTo,
    addProvider,
    deleteProvider,
    updateProvider,
    reorderProviders,
  };
}

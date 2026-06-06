import { Loader2, RefreshCw, Sparkles } from "lucide-react";
import { memo, useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { cn } from "../../../lib/utils";
import type { AiConfig, ProviderEntryFlat, ProviderPatchFlat } from "../../../types";
import { ProviderBrandIcon } from "../../models/components/ProviderBrandIcon";
import {
  buildModelCatalog,
  CLAUDE_MODEL_META_KEYS,
  getMetaString,
} from "../../models/components/providerForm/useProviderFormState";
import { useAppAiProvider, type AppAiAppId } from "../../models/hooks/useAppAiProvider";
import { useModelFetch } from "../../models/hooks/useModelFetch";
import { useProvidersFlat } from "../../models/hooks/useProvidersFlat";

export interface AppAiModelsPickerProps {
  config: AiConfig;
  disabled?: boolean;
  onConfigChange: (next: AiConfig) => void;
}

function AppAiModelsPickerInner({ config, disabled, onConfigChange }: AppAiModelsPickerProps) {
  const { t } = useTranslation();
  const { providers, isLoading, updateProvider } = useProvidersFlat();
  const { setAppAiProvider, isSetting } = useAppAiProvider();
  const { fetchModels, isLoading: isFetchingModels } = useModelFetch();

  const selectedId = config.provider_ref?.provider_id ?? "";
  const selectedApp = (
    config.provider_ref?.app_id === "codex" || (!config.provider_ref && config.api_format === "openai")
      ? "codex"
      : "claude"
  ) as AppAiAppId;

  const eligible = useMemo(
    () => providers.filter((p) => p.api_key.trim() && (p.base_url_openai.trim() || p.base_url_anthropic.trim())),
    [providers],
  );

  const selectedProvider = useMemo(() => eligible.find((p) => p.id === selectedId) ?? null, [eligible, selectedId]);

  const selectedModel = useMemo(() => {
    if (!selectedProvider) return config.model;
    if (selectedApp === "claude") {
      const configured =
        getMetaString(selectedProvider.meta, CLAUDE_MODEL_META_KEYS.main) || selectedProvider.default_model;
      if (configured) return configured;
      return selectedProvider.models.includes(config.model) ? config.model : selectedProvider.models[0] || "";
    }
    if (selectedProvider.default_model) return selectedProvider.default_model;
    return selectedProvider.models.includes(config.model) ? config.model : selectedProvider.models[0] || "";
  }, [config.model, selectedApp, selectedProvider]);

  const modelOptions = useMemo(() => {
    if (!selectedProvider) return buildModelCatalog([config.model]);
    const configModel = selectedProvider.models.includes(config.model) ? config.model : "";
    return buildModelCatalog([selectedModel, selectedProvider.default_model, ...selectedProvider.models, configModel]);
  }, [config.model, selectedModel, selectedProvider]);

  const resolveProviderModel = useCallback((provider: ProviderEntryFlat | undefined, appId: AppAiAppId) => {
    if (!provider) return "";
    if (appId === "claude") {
      return (
        getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.main) || provider.default_model || provider.models[0] || ""
      );
    }
    return provider.default_model || provider.models[0] || "";
  }, []);

  const buildModelPatch = useCallback(
    (provider: ProviderEntryFlat, model: string, fetchedModels: string[] = []): ProviderPatchFlat => {
      const catalog = buildModelCatalog([...fetchedModels, ...provider.models, provider.default_model, model]);
      return {
        models: catalog,
        default_model: model.trim() || provider.default_model,
        meta:
          selectedApp === "claude" && model.trim()
            ? {
                ...(provider.meta ?? {}),
                [CLAUDE_MODEL_META_KEYS.main]: model.trim(),
              }
            : provider.meta,
      };
    },
    [selectedApp],
  );

  const handleSelectProvider = useCallback(
    async (providerId: string) => {
      if (!providerId) return;
      const provider = providers.find((p) => p.id === providerId);
      await setAppAiProvider(selectedApp, providerId, provider?.name);
      const model = resolveProviderModel(provider, selectedApp);
      onConfigChange({
        ...config,
        enabled: true,
        api_format: selectedApp === "claude" ? "anthropic" : "openai",
        model,
        provider_ref: { app_id: selectedApp, provider_id: providerId },
      });
    },
    [config, onConfigChange, providers, resolveProviderModel, selectedApp, setAppAiProvider],
  );

  const handleAppChange = useCallback(
    (appId: AppAiAppId) => {
      if (!selectedId) {
        onConfigChange({
          ...config,
          api_format: appId === "claude" ? "anthropic" : "openai",
          provider_ref: null,
        });
        return;
      }
      const provider = providers.find((p) => p.id === selectedId);
      const model = resolveProviderModel(provider, appId);
      void setAppAiProvider(appId, selectedId).then(() => {
        onConfigChange({
          ...config,
          api_format: appId === "claude" ? "anthropic" : "openai",
          model,
          provider_ref: { app_id: appId, provider_id: selectedId },
        });
      });
    },
    [config, onConfigChange, providers, resolveProviderModel, selectedId, setAppAiProvider],
  );

  const handleFetchProviderModels = useCallback(async () => {
    if (!selectedProvider) return;
    if (!selectedProvider.models_url.trim()) {
      toast.error("请先在 Models 供应商里配置获取模型 URL");
      return;
    }
    if (!selectedProvider.api_key.trim()) {
      toast.error("请先在 Models 供应商里配置 API Key");
      return;
    }

    try {
      const fetched = await fetchModels(selectedProvider.models_url, selectedProvider.api_key);
      const nextModel = selectedModel || fetched[0] || "";
      await updateProvider(selectedProvider.id, buildModelPatch(selectedProvider, nextModel, fetched));
      if (nextModel && nextModel !== config.model) {
        onConfigChange({
          ...config,
          model: nextModel,
          api_format: selectedApp === "claude" ? "anthropic" : "openai",
        });
      }
      toast.success(`已获取 ${fetched.length} 个模型`);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(`获取模型失败：${message}`);
    }
  }, [
    buildModelPatch,
    config,
    fetchModels,
    onConfigChange,
    selectedApp,
    selectedModel,
    selectedProvider,
    updateProvider,
  ]);

  const handleModelChange = useCallback(
    async (model: string) => {
      const trimmed = model.trim();
      if (!selectedProvider || !trimmed) return;
      try {
        await updateProvider(selectedProvider.id, buildModelPatch(selectedProvider, trimmed));
        onConfigChange({
          ...config,
          model: trimmed,
          api_format: selectedApp === "claude" ? "anthropic" : "openai",
        });
        toast.success(`已选择模型：${trimmed}`);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        toast.error(`模型保存失败：${message}`);
      }
    },
    [buildModelPatch, config, onConfigChange, selectedApp, selectedProvider, updateProvider],
  );

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        {t("common.loading", { defaultValue: "加载中…" })}
      </div>
    );
  }

  if (eligible.length === 0) {
    return (
      <p className="rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
        {t("settings.noModelsProviders", {
          defaultValue: "请先在 Models 模式添加并配置供应商，再选择用于应用内 AI。",
        })}
      </p>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Sparkles className="h-3.5 w-3.5 text-primary" />
        <span>
          {t("settings.appAiModelsHint", {
            defaultValue: "从 Models 供应商库选择，用于摘要、翻译与技能推荐。",
          })}
        </span>
      </div>

      <div className="flex gap-2">
        {(["claude", "codex"] as const).map((appId) => (
          <button
            key={appId}
            type="button"
            aria-pressed={selectedApp === appId}
            disabled={disabled || isSetting}
            onClick={() => handleAppChange(appId)}
            className={cn(
              "min-h-10 flex-1 rounded-xl border px-3 text-sm font-semibold transition-colors disabled:cursor-not-allowed disabled:opacity-60",
              selectedApp === appId
                ? "border-primary/60 bg-primary/10 text-primary"
                : "border-border/60 text-muted-foreground hover:border-border hover:text-foreground",
            )}
          >
            {appId === "claude" ? "Claude 协议" : "OpenAI 协议"}
          </button>
        ))}
      </div>

      <div className="grid gap-2 max-h-48 overflow-y-auto pr-1">
        {eligible.map((p) => {
          const isSelected = p.id === selectedId;
          return (
            <button
              key={p.id}
              type="button"
              disabled={disabled || isSetting}
              onClick={() => handleSelectProvider(p.id)}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-lg border px-3 py-2 text-left transition-colors",
                isSelected
                  ? "border-primary/40 bg-primary/10"
                  : "border-border/55 bg-background/30 hover:border-primary/25 hover:bg-card/60",
              )}
            >
              <ProviderBrandIcon presetId={p.preset_id} providerName={p.name} iconColor={p.icon_color} size="sm" />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">{p.name}</span>
              {isSelected && <span className="text-[10px] text-primary">当前</span>}
            </button>
          );
        })}
      </div>

      {selectedProvider && (
        <div className="rounded-xl border border-border/55 bg-background/25 p-3">
          <div className="mb-2 flex items-center justify-between gap-3">
            <label htmlFor="app-ai-provider-model" className="text-xs font-medium text-muted-foreground">
              {t("settings.model", { defaultValue: "模型" })}
            </label>
            <button
              type="button"
              disabled={disabled || isSetting || isFetchingModels}
              onClick={() => void handleFetchProviderModels()}
              className="inline-flex min-h-8 items-center justify-center gap-1.5 rounded-lg border border-border/60 px-3 text-xs font-medium text-muted-foreground transition-colors hover:border-primary/35 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isFetchingModels ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5" />
              )}
              {t("models.fetchModels", { defaultValue: "获取模型" })}
            </button>
          </div>
          <select
            id="app-ai-provider-model"
            value={selectedModel}
            disabled={disabled || isSetting || modelOptions.length === 0}
            onChange={(e) => void handleModelChange(e.target.value)}
            className="h-10 w-full cursor-pointer rounded-lg border border-input-border bg-input px-3 text-sm text-foreground shadow-sm transition focus:border-primary/60 focus:outline-none focus:ring-2 focus:ring-primary/35 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {modelOptions.length === 0 ? (
              <option value="">{t("models.noModels", { defaultValue: "暂无模型" })}</option>
            ) : (
              modelOptions.map((model) => (
                <option key={model} value={model}>
                  {model}
                </option>
              ))
            )}
          </select>
        </div>
      )}
    </div>
  );
}

export const AppAiModelsPicker = memo(AppAiModelsPickerInner);

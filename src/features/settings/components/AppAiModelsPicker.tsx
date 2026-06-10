import {
  AlertCircle,
  CheckCircle2,
  Eye,
  EyeOff,
  Globe2,
  KeyRound,
  Loader2,
  RefreshCw,
  Save,
  Sparkles,
} from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { cn } from "../../../lib/utils";
import type { AiConfig, ProviderEntryFlat, ProviderPatchFlat } from "../../../types";
import { ProviderBrandIcon } from "../../models";
import { buildModelCatalog, CLAUDE_MODEL_META_KEYS, getMetaString } from "../../models";
import { useAppAiProvider, type AppAiAppId } from "../../models";
import { useModelFetch } from "../../models";
import { useProvidersFlat } from "../../models";

export interface AppAiModelsPickerProps {
  config: AiConfig;
  disabled?: boolean;
  onConfigChange: (next: AiConfig) => void;
}

type ConnectionSaveState = "idle" | "saving" | "saved" | "error";

function initialAppFromConfig(config: AiConfig): AppAiAppId {
  return config.provider_ref?.app_id === "codex" || (!config.provider_ref && config.api_format === "openai")
    ? "codex"
    : "claude";
}

function providerModel(provider: ProviderEntryFlat | undefined, appId: AppAiAppId, fallback = "") {
  if (!provider) return fallback;
  if (appId === "claude") {
    return (
      getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.main) || provider.default_model || provider.models[0] || ""
    );
  }
  return provider.default_model || provider.models[0] || fallback;
}

function activeBaseUrl(provider: ProviderEntryFlat, appId: AppAiAppId) {
  if (appId === "claude") return provider.base_url_anthropic.trim() || provider.base_url_openai.trim();
  return provider.base_url_openai.trim();
}

function hasUsableCredentials(provider: ProviderEntryFlat, appId: AppAiAppId) {
  return Boolean(provider.api_key.trim() && activeBaseUrl(provider, appId));
}

function isValidHttpUrl(value: string) {
  const trimmed = value.trim();
  if (!trimmed) return true;
  try {
    const parsed = new URL(trimmed);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

function AppAiModelsPickerInner({ config, disabled, onConfigChange }: AppAiModelsPickerProps) {
  const { t } = useTranslation();
  const { providers, isLoading, updateProvider } = useProvidersFlat();
  const { setAppAiProvider, isSetting } = useAppAiProvider();
  const { fetchModels, isLoading: isFetchingModels } = useModelFetch();

  const [draftApp, setDraftApp] = useState<AppAiAppId>(() => initialAppFromConfig(config));
  const [draftProviderId, setDraftProviderId] = useState(config.provider_ref?.provider_id ?? "");
  const [draftBaseUrlOpenai, setDraftBaseUrlOpenai] = useState("");
  const [draftBaseUrlAnthropic, setDraftBaseUrlAnthropic] = useState("");
  const [draftModelsUrl, setDraftModelsUrl] = useState("");
  const [draftApiKey, setDraftApiKey] = useState("");
  const [draftModel, setDraftModel] = useState("");
  const [showApiKey, setShowApiKey] = useState(false);
  const [connectionState, setConnectionState] = useState<ConnectionSaveState>("idle");

  const selectedId = draftProviderId || config.provider_ref?.provider_id || "";
  const selectedApp = draftApp;
  const firstProviderId = providers[0]?.id ?? "";

  const selectedProvider = useMemo(() => providers.find((p) => p.id === selectedId) ?? null, [providers, selectedId]);

  const selectedModel = useMemo(() => {
    return providerModel(selectedProvider ?? undefined, selectedApp, config.model);
  }, [config.model, selectedApp, selectedProvider]);

  const modelOptions = useMemo(() => {
    if (!selectedProvider) return buildModelCatalog([draftModel, config.model]);
    return buildModelCatalog([draftModel, selectedModel, selectedProvider.default_model, ...selectedProvider.models]);
  }, [config.model, draftModel, selectedModel, selectedProvider]);

  useEffect(() => {
    setDraftApp(initialAppFromConfig(config));
    setDraftProviderId(config.provider_ref?.provider_id ?? "");
  }, [config.api_format, config.provider_ref]);

  useEffect(() => {
    if (!draftProviderId && firstProviderId) {
      setDraftProviderId(firstProviderId);
    }
  }, [draftProviderId, firstProviderId]);

  useEffect(() => {
    if (!selectedProvider) {
      setDraftBaseUrlOpenai("");
      setDraftBaseUrlAnthropic("");
      setDraftModelsUrl("");
      setDraftApiKey("");
      setDraftModel(config.model);
      setConnectionState("idle");
      return;
    }

    setDraftBaseUrlOpenai(selectedProvider.base_url_openai);
    setDraftBaseUrlAnthropic(selectedProvider.base_url_anthropic);
    setDraftModelsUrl(selectedProvider.models_url ?? "");
    setDraftApiKey(selectedProvider.api_key);
    setDraftModel(providerModel(selectedProvider, selectedApp, config.model));
    setConnectionState("idle");
  }, [config.model, selectedApp, selectedProvider]);

  const hasConnectionChanges = useMemo(() => {
    if (!selectedProvider) return false;
    const providerSelectedModel = providerModel(selectedProvider, selectedApp, config.model);
    return (
      draftBaseUrlOpenai !== selectedProvider.base_url_openai ||
      draftBaseUrlAnthropic !== selectedProvider.base_url_anthropic ||
      draftModelsUrl !== (selectedProvider.models_url ?? "") ||
      draftApiKey !== selectedProvider.api_key ||
      draftModel.trim() !== providerSelectedModel
    );
  }, [
    config.model,
    draftApiKey,
    draftBaseUrlAnthropic,
    draftBaseUrlOpenai,
    draftModel,
    draftModelsUrl,
    selectedApp,
    selectedProvider,
  ]);

  const providerReadiness = useMemo(() => {
    if (!selectedProvider) return { label: "", tone: "muted" as const };
    if (hasConnectionChanges) return { label: "待保存", tone: "warn" as const };
    if (!selectedProvider.api_key.trim()) return { label: "缺 API Key", tone: "warn" as const };
    if (!activeBaseUrl(selectedProvider, selectedApp)) return { label: "缺 Base URL", tone: "warn" as const };
    return { label: "可用于应用内 AI", tone: "ready" as const };
  }, [hasConnectionChanges, selectedApp, selectedProvider]);

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

  const buildConnectionPatch = useCallback(
    (provider: ProviderEntryFlat, model: string, fetchedModels: string[] = []): ProviderPatchFlat => {
      const trimmedModel = model.trim();
      return {
        api_key: draftApiKey,
        base_url_openai: draftBaseUrlOpenai.trim(),
        base_url_anthropic: draftBaseUrlAnthropic.trim(),
        models_url: draftModelsUrl.trim(),
        ...buildModelPatch(provider, trimmedModel || providerModel(provider, selectedApp, config.model), fetchedModels),
      };
    },
    [
      buildModelPatch,
      config.model,
      draftApiKey,
      draftBaseUrlAnthropic,
      draftBaseUrlOpenai,
      draftModelsUrl,
      selectedApp,
    ],
  );

  const bindProviderForApp = useCallback(
    async (provider: ProviderEntryFlat, model: string) => {
      await setAppAiProvider(selectedApp, provider.id, provider.name);
      onConfigChange({
        ...config,
        enabled: true,
        api_format: selectedApp === "claude" ? "anthropic" : "openai",
        model,
        provider_ref: { app_id: selectedApp, provider_id: provider.id },
      });
    },
    [config, onConfigChange, selectedApp, setAppAiProvider],
  );

  const handleSelectProvider = useCallback(
    async (providerId: string) => {
      if (!providerId) return;
      const provider = providers.find((p) => p.id === providerId);
      setDraftProviderId(providerId);
      setConnectionState("idle");
      if (!provider || !hasUsableCredentials(provider, selectedApp)) return;
      await bindProviderForApp(provider, providerModel(provider, selectedApp, config.model));
    },
    [bindProviderForApp, config.model, providers, selectedApp],
  );

  const handleAppChange = useCallback(
    (appId: AppAiAppId) => {
      setDraftApp(appId);
      const provider = providers.find((p) => p.id === selectedId);
      if (!provider || !hasUsableCredentials(provider, appId)) return;
      const model = providerModel(provider, appId, config.model);
      void setAppAiProvider(appId, selectedId, provider.name).then(() => {
        onConfigChange({
          ...config,
          enabled: true,
          api_format: appId === "claude" ? "anthropic" : "openai",
          model,
          provider_ref: { app_id: appId, provider_id: selectedId },
        });
      });
    },
    [config, onConfigChange, providers, selectedId, setAppAiProvider],
  );

  const handleFetchProviderModels = useCallback(async () => {
    if (!selectedProvider) return;
    if (!draftModelsUrl.trim()) {
      toast.error("请先填写获取模型 URL");
      return;
    }
    if (!draftApiKey.trim()) {
      toast.error("请先填写 API Key");
      return;
    }

    try {
      const fetched = await fetchModels(draftModelsUrl.trim(), draftApiKey.trim());
      const nextModel = draftModel.trim() || selectedModel || fetched[0] || "";
      const updated = await updateProvider(
        selectedProvider.id,
        buildConnectionPatch(selectedProvider, nextModel, fetched),
      );
      setDraftModel(nextModel);
      if (draftApiKey.trim() && activeBaseUrl(updated, selectedApp)) {
        await bindProviderForApp(updated, providerModel(updated, selectedApp, nextModel));
      } else if (nextModel && nextModel !== config.model) {
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
    buildConnectionPatch,
    bindProviderForApp,
    config,
    draftApiKey,
    draftModel,
    draftModelsUrl,
    fetchModels,
    onConfigChange,
    selectedApp,
    selectedModel,
    selectedProvider,
    updateProvider,
  ]);

  const handleSaveConnection = useCallback(async () => {
    if (!selectedProvider) return;
    if (!isValidHttpUrl(draftBaseUrlOpenai)) {
      toast.error("OpenAI Base URL 格式无效");
      setConnectionState("error");
      return;
    }
    if (!isValidHttpUrl(draftBaseUrlAnthropic)) {
      toast.error("Anthropic Base URL 格式无效");
      setConnectionState("error");
      return;
    }
    if (!isValidHttpUrl(draftModelsUrl)) {
      toast.error("获取模型 URL 格式无效");
      setConnectionState("error");
      return;
    }

    const nextModel = draftModel.trim() || providerModel(selectedProvider, selectedApp, config.model);
    setConnectionState("saving");
    try {
      const updated = await updateProvider(selectedProvider.id, buildConnectionPatch(selectedProvider, nextModel));
      setConnectionState("saved");
      if (draftApiKey.trim() && activeBaseUrl(updated, selectedApp)) {
        await bindProviderForApp(updated, providerModel(updated, selectedApp, nextModel));
      } else {
        onConfigChange({
          ...config,
          api_format: selectedApp === "claude" ? "anthropic" : "openai",
          model: nextModel,
          provider_ref: null,
        });
      }
      toast.success("连接配置已保存");
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setConnectionState("error");
      toast.error(`保存失败：${message}`);
    }
  }, [
    bindProviderForApp,
    buildConnectionPatch,
    config,
    draftApiKey,
    draftBaseUrlAnthropic,
    draftBaseUrlOpenai,
    draftModel,
    draftModelsUrl,
    onConfigChange,
    selectedApp,
    selectedProvider,
    updateProvider,
  ]);

  const primaryBaseLabel = selectedApp === "claude" ? "Claude Base URL" : "OpenAI Base URL";
  const primaryBaseValue = selectedApp === "claude" ? draftBaseUrlAnthropic : draftBaseUrlOpenai;
  const setPrimaryBaseValue = selectedApp === "claude" ? setDraftBaseUrlAnthropic : setDraftBaseUrlOpenai;
  const secondaryBaseLabel = selectedApp === "claude" ? "OpenAI 兼容 Base URL" : "Anthropic Base URL";
  const secondaryBaseValue = selectedApp === "claude" ? draftBaseUrlOpenai : draftBaseUrlAnthropic;
  const setSecondaryBaseValue = selectedApp === "claude" ? setDraftBaseUrlOpenai : setDraftBaseUrlAnthropic;
  const draftActiveBaseUrl =
    selectedApp === "claude" ? draftBaseUrlAnthropic.trim() || draftBaseUrlOpenai.trim() : draftBaseUrlOpenai.trim();
  const hasDraftCredentials = Boolean(draftApiKey.trim() && draftActiveBaseUrl);
  const isBoundToCurrentProvider =
    Boolean(selectedProvider) &&
    config.provider_ref?.provider_id === selectedProvider?.id &&
    config.provider_ref?.app_id === selectedApp;
  const canUsePrimaryAction = hasConnectionChanges || (!isBoundToCurrentProvider && hasDraftCredentials);

  const saveLabel = useMemo(
    () =>
      connectionState === "saving"
        ? "保存中"
        : connectionState === "saved"
          ? "已保存"
          : connectionState === "error"
            ? "重试保存"
            : "保存连接配置",
    [connectionState],
  );
  const primaryActionLabel =
    !isBoundToCurrentProvider && !hasConnectionChanges && hasDraftCredentials ? "设为应用内 AI" : saveLabel;

  if (isLoading) {
    return (
      <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        {t("common.loading", { defaultValue: "加载中…" })}
      </div>
    );
  }

  if (providers.length === 0) {
    return (
      <div className="rounded-xl border border-dashed border-border/70 bg-muted/15 px-3.5 py-3">
        <div className="flex items-start gap-2.5">
          <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
          <div className="min-w-0 space-y-1">
            <p className="text-sm font-medium text-foreground">还没有 Models 供应商</p>
            <p className="text-xs leading-5 text-muted-foreground">
              先在 Models 工作台新增一个供应商；新增后这里可以直接编辑 Base URL、API Key 和应用内 AI 模型。
            </p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
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

      <div className="grid max-h-48 gap-2 overflow-y-auto pr-1">
        {providers.map((p) => {
          const isSelected = p.id === selectedId;
          const ready = hasUsableCredentials(p, selectedApp);
          const missingLabel = !p.api_key.trim() ? "缺 Key" : !activeBaseUrl(p, selectedApp) ? "缺 URL" : "";
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
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium text-foreground">{p.name}</span>
                <span className="mt-0.5 block truncate text-[11px] text-muted-foreground">
                  {ready ? p.default_model || p.models[0] || "已配置" : missingLabel}
                </span>
              </span>
              {isSelected ? (
                <span className="rounded-md bg-primary/12 px-1.5 py-0.5 text-[10px] text-primary">当前</span>
              ) : null}
            </button>
          );
        })}
      </div>

      {selectedProvider && (
        <div className="rounded-xl border border-border/60 bg-background/25 p-3.5">
          <div className="mb-3 flex items-center justify-between gap-3">
            <div className="flex min-w-0 items-center gap-2.5">
              <ProviderBrandIcon
                presetId={selectedProvider.preset_id}
                providerName={selectedProvider.name}
                iconColor={selectedProvider.icon_color}
                size="sm"
              />
              <div className="min-w-0">
                <h3 className="truncate text-sm font-semibold text-foreground">{selectedProvider.name}</h3>
                <p className="text-[11px] text-muted-foreground">应用内 AI 会复用这里的连接配置</p>
              </div>
            </div>
            <span
              className={cn(
                "inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-[11px]",
                providerReadiness.tone === "ready"
                  ? "bg-success/10 text-success"
                  : providerReadiness.tone === "warn"
                    ? "bg-warning/10 text-warning"
                    : "bg-muted/40 text-muted-foreground",
              )}
            >
              {providerReadiness.tone === "ready" ? (
                <CheckCircle2 className="h-3 w-3" />
              ) : (
                <AlertCircle className="h-3 w-3" />
              )}
              {providerReadiness.label}
            </span>
          </div>

          <div className="grid gap-3">
            <div className="space-y-1.5">
              <div className="flex items-center justify-between gap-2">
                <label htmlFor="app-ai-provider-api-key" className="text-xs font-medium text-muted-foreground">
                  API Key
                </label>
                <span className="text-[11px] text-muted-foreground">仅保存在本机 provider 配置</span>
              </div>
              <div className="relative">
                <KeyRound className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
                <Input
                  id="app-ai-provider-api-key"
                  type={showApiKey ? "text" : "password"}
                  value={draftApiKey}
                  onChange={(e) => {
                    setDraftApiKey(e.target.value);
                    setConnectionState("idle");
                  }}
                  placeholder="sk-..."
                  disabled={disabled || isSetting || connectionState === "saving"}
                  className="pl-9 pr-10 font-mono"
                />
                <button
                  type="button"
                  onClick={() => setShowApiKey((value) => !value)}
                  className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted-foreground transition hover:text-foreground"
                  aria-label={showApiKey ? "隐藏 API Key" : "显示 API Key"}
                >
                  {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                </button>
              </div>
            </div>

            <div className="grid gap-3 sm:grid-cols-2">
              <label className="space-y-1.5">
                <span className="text-xs font-medium text-muted-foreground">{primaryBaseLabel}</span>
                <div className="relative">
                  <Globe2 className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
                  <Input
                    value={primaryBaseValue}
                    onChange={(e) => {
                      setPrimaryBaseValue(e.target.value);
                      setConnectionState("idle");
                    }}
                    placeholder={
                      selectedApp === "claude" ? "https://api.example.com/anthropic" : "https://api.example.com/v1"
                    }
                    disabled={disabled || isSetting || connectionState === "saving"}
                    className="pl-9 font-mono"
                  />
                </div>
              </label>

              <label className="space-y-1.5">
                <span className="text-xs font-medium text-muted-foreground">{secondaryBaseLabel}</span>
                <div className="relative">
                  <Globe2 className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
                  <Input
                    value={secondaryBaseValue}
                    onChange={(e) => {
                      setSecondaryBaseValue(e.target.value);
                      setConnectionState("idle");
                    }}
                    placeholder={
                      selectedApp === "claude" ? "https://api.example.com/v1" : "https://api.example.com/anthropic"
                    }
                    disabled={disabled || isSetting || connectionState === "saving"}
                    className="pl-9 font-mono"
                  />
                </div>
              </label>
            </div>

            <div className="grid gap-3 sm:grid-cols-[1fr_1fr_auto]">
              <label className="space-y-1.5">
                <span className="text-xs font-medium text-muted-foreground">获取模型 URL</span>
                <Input
                  value={draftModelsUrl}
                  onChange={(e) => {
                    setDraftModelsUrl(e.target.value);
                    setConnectionState("idle");
                  }}
                  placeholder="https://api.example.com/v1/models"
                  disabled={disabled || isSetting || connectionState === "saving"}
                  className="font-mono"
                />
              </label>

              <label className="space-y-1.5">
                <span className="text-xs font-medium text-muted-foreground">
                  {t("settings.model", { defaultValue: "模型" })}
                </span>
                <Input
                  id="app-ai-provider-model"
                  value={draftModel}
                  onChange={(e) => {
                    setDraftModel(e.target.value);
                    setConnectionState("idle");
                  }}
                  placeholder={t("models.noModels", { defaultValue: "暂无模型" })}
                  list="app-ai-provider-model-options"
                  disabled={disabled || isSetting || connectionState === "saving"}
                  className="font-mono"
                />
                <datalist id="app-ai-provider-model-options">
                  {modelOptions.map((model) => (
                    <option key={model} value={model} />
                  ))}
                </datalist>
              </label>

              <div className="flex items-end">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={disabled || isSetting || isFetchingModels || connectionState === "saving"}
                  onClick={() => void handleFetchProviderModels()}
                  className="h-9 w-full rounded-lg px-3 text-xs sm:w-auto"
                >
                  {isFetchingModels ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <RefreshCw className="h-3.5 w-3.5" />
                  )}
                  {t("models.fetchModels", { defaultValue: "获取模型" })}
                </Button>
              </div>
            </div>

            <div className="flex items-center justify-between gap-3 border-t border-border/45 pt-3">
              <p className="min-w-0 text-xs text-muted-foreground">
                {selectedApp === "claude"
                  ? "Claude 协议优先使用 Anthropic Base URL，缺省时回退到 OpenAI 兼容端点。"
                  : "OpenAI 协议会使用 OpenAI Base URL 和默认模型。"}
              </p>
              <Button
                type="button"
                size="sm"
                onClick={() => void handleSaveConnection()}
                disabled={disabled || isSetting || connectionState === "saving" || !canUsePrimaryAction}
                className="h-9 min-w-[118px] rounded-lg px-3 text-xs"
              >
                {connectionState === "saving" ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Save className="h-3.5 w-3.5" />
                )}
                {primaryActionLabel}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export const AppAiModelsPicker = memo(AppAiModelsPickerInner);

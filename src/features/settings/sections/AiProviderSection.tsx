import { CheckCircle, ChevronDown, Loader2, Sparkles, XCircle, Zap } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { Switch } from "../../../components/ui/switch";
import { useNavigation } from "../../../hooks/useNavigation";
import { cn } from "../../../lib/utils";
import type { AiConfig } from "../../../types";
import type { ProviderEntry } from "../../models/hooks/useModelProviders";
import { useModelProviders } from "../../models/hooks/useModelProviders";

interface AiProviderSectionProps {
  localAiConfig: AiConfig;
  ready: boolean;
  aiExpanded: boolean;
  aiSaving: boolean;
  aiSaved: boolean;
  aiTesting: boolean;
  aiTestResult: "success" | "error" | null;
  aiTestLatency: number | null;
  onToggleExpanded: () => void;
  onEnabledChange: (enabled: boolean) => void;
  onConfigChange: (next: AiConfig) => void;
  onTestConnection: () => void;
}

const LOCAL_PROVIDER_VALUE = "__local__";
const DEFAULT_LOCAL_BASE_URL = "http://127.0.0.1:11434/v1";
const DEFAULT_LOCAL_MODEL = "llama3.1:8b";

function hasClaudeApiKey(provider: ProviderEntry) {
  const env = (provider.settingsConfig?.env as Record<string, unknown> | undefined) ?? undefined;
  return (
    (typeof env?.ANTHROPIC_AUTH_TOKEN === "string" && env.ANTHROPIC_AUTH_TOKEN.trim().length > 0) ||
    (typeof env?.ANTHROPIC_API_KEY === "string" && env.ANTHROPIC_API_KEY.trim().length > 0)
  );
}

function hasCodexApiKey(provider: ProviderEntry) {
  const auth = (provider.settingsConfig?.auth as Record<string, unknown> | undefined) ?? undefined;
  return typeof auth?.OPENAI_API_KEY === "string" && auth.OPENAI_API_KEY.trim().length > 0;
}

function currentLocalPreset(config: AiConfig) {
  return {
    ...config.local_preset,
    base_url: config.api_format === "local" ? config.base_url : config.local_preset.base_url,
    api_key: "",
    model: config.api_format === "local" ? config.model : config.local_preset.model,
  };
}

export function AiProviderSection({
  localAiConfig,
  ready,
  aiExpanded,
  aiSaving,
  aiSaved,
  aiTesting,
  aiTestResult,
  aiTestLatency,
  onToggleExpanded,
  onEnabledChange,
  onConfigChange,
  onTestConnection,
}: AiProviderSectionProps) {
  const { t } = useTranslation();
  const { navigateToModels } = useNavigation();
  const claudeProviders = useModelProviders("claude");
  const codexProviders = useModelProviders("codex");

  const providerCandidates = useMemo(() => {
    const items: { appId: "claude" | "codex"; providerId: string; label: string; hasKey: boolean }[] = [];

    for (const provider of Object.values(claudeProviders.providers)) {
      items.push({
        appId: "claude",
        providerId: provider.id,
        label: `Claude · ${provider.name}`,
        hasKey: hasClaudeApiKey(provider),
      });
    }

    for (const provider of Object.values(codexProviders.providers)) {
      items.push({
        appId: "codex",
        providerId: provider.id,
        label: `Codex · ${provider.name}`,
        hasKey: hasCodexApiKey(provider),
      });
    }

    return items;
  }, [claudeProviders.providers, codexProviders.providers]);

  const isLocalMode = localAiConfig.api_format === "local" && !localAiConfig.provider_ref;
  const selectedProviderValue = isLocalMode
    ? LOCAL_PROVIDER_VALUE
    : localAiConfig.provider_ref
      ? `${localAiConfig.provider_ref.app_id}:${localAiConfig.provider_ref.provider_id}`
      : "";
  const selectedProvider = providerCandidates.find(
    (candidate) => `${candidate.appId}:${candidate.providerId}` === selectedProviderValue,
  );
  const hasResolvedProvider = !!selectedProvider?.hasKey;
  const clampConcurrency = (value: number) => Math.min(20, Math.max(1, value || 1));
  const formControlClass =
    "flex h-9 w-full rounded-xl border border-input-border bg-input backdrop-blur-sm px-3 text-sm text-foreground shadow-sm transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:border-primary/60";

  const handleProviderChange = (value: string) => {
    const nextLocalPreset = currentLocalPreset(localAiConfig);

    if (value === LOCAL_PROVIDER_VALUE) {
      onConfigChange({
        ...localAiConfig,
        api_format: "local",
        provider_ref: null,
        base_url: nextLocalPreset.base_url || DEFAULT_LOCAL_BASE_URL,
        api_key: "",
        model: nextLocalPreset.model || DEFAULT_LOCAL_MODEL,
        local_preset: nextLocalPreset,
      });
      return;
    }

    if (!value) {
      onConfigChange({
        ...localAiConfig,
        provider_ref: null,
        local_preset: nextLocalPreset,
      });
      return;
    }

    const [appId, providerId] = value.split(":");
    if (!appId || !providerId) return;

    onConfigChange({
      ...localAiConfig,
      api_format: appId === "claude" ? "anthropic" : "openai",
      provider_ref: { app_id: appId, provider_id: providerId },
      local_preset: nextLocalPreset,
    });
  };

  const handleLocalBaseUrlChange = (value: string) => {
    onConfigChange({
      ...localAiConfig,
      api_format: "local",
      provider_ref: null,
      base_url: value,
      api_key: "",
      local_preset: {
        ...localAiConfig.local_preset,
        base_url: value,
        api_key: "",
        model: localAiConfig.model,
      },
    });
  };

  const handleLocalModelChange = (value: string) => {
    onConfigChange({
      ...localAiConfig,
      api_format: "local",
      provider_ref: null,
      api_key: "",
      model: value,
      local_preset: {
        ...localAiConfig.local_preset,
        base_url: localAiConfig.base_url,
        api_key: "",
        model: value,
      },
    });
  };

  const badgeLabel =
    isLocalMode && localAiConfig.enabled
      ? `${t("settings.localOllama", { defaultValue: "Local Model (Ollama)" })} · ${localAiConfig.model}`
      : selectedProvider && localAiConfig.enabled
        ? selectedProvider.label
        : null;

  return (
    <section>
      <div className="flex items-center justify-between mb-3 px-1">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-emerald-500/10 flex items-center justify-center shrink-0 border border-emerald-500/20">
            <Sparkles className="w-4 h-4 text-emerald-500" />
          </div>
          <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.aiProvider")}</h2>
          {badgeLabel && (
            <span className="text-xs text-muted-foreground ml-2 px-2 py-0.5 rounded-md bg-muted/50 border border-border">
              {badgeLabel}
            </span>
          )}
        </div>

        {ready ? (
          <Switch checked={localAiConfig.enabled} onCheckedChange={onEnabledChange} disabled={aiSaving} />
        ) : (
          <div className="h-5 w-9 rounded-full border border-border bg-muted/60" />
        )}
      </div>

      <div
        className={cn(
          "rounded-xl border border-border overflow-hidden transition-colors",
          localAiConfig.enabled ? "bg-card" : "bg-card/50",
        )}
      >
        <button
          type="button"
          onClick={onToggleExpanded}
          className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/30 transition-colors cursor-pointer"
        >
          <span className="text-sm font-medium text-foreground">
            {t("settings.aiConfigTitle", { defaultValue: "AI Summary & Scan" })}
          </span>
          <ChevronDown
            className={cn(
              "w-4 h-4 text-muted-foreground transition-transform duration-200",
              !aiExpanded && "-rotate-90",
            )}
          />
        </button>

        {aiExpanded && (
          <div className="px-4 pb-4 pt-1 border-t border-border space-y-3">
            <div className="rounded-lg border border-emerald-500/15 bg-emerald-500/[0.06] px-3 py-2.5 flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <p className="text-xs text-muted-foreground leading-relaxed">{t("settings.modelAgentsHint")}</p>
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="shrink-0 h-8 text-xs border-emerald-500/25 hover:bg-emerald-500/10"
                onClick={() => navigateToModels()}
              >
                {t("settings.modelAgentsCta")}
              </Button>
            </div>

            <div>
              <label htmlFor="ai-provider-select" className="text-xs text-muted-foreground block mb-1">
                {t("settings.selectProvider", { defaultValue: "Provider" })}
              </label>
              <select
                id="ai-provider-select"
                value={selectedProviderValue}
                onChange={(e) => handleProviderChange(e.target.value)}
                className={`${formControlClass} pr-8`}
                disabled={claudeProviders.loading || codexProviders.loading}
              >
                <option value="">
                  {t("settings.providerNone", { defaultValue: "None — choose from Models" })}
                </option>
                <option value={LOCAL_PROVIDER_VALUE}>
                  {t("settings.localOllama", { defaultValue: "Local Model (Ollama)" })}
                </option>
                {providerCandidates.map((candidate) => (
                  <option
                    key={`${candidate.appId}:${candidate.providerId}`}
                    value={`${candidate.appId}:${candidate.providerId}`}
                    disabled={!candidate.hasKey}
                  >
                    {candidate.label}
                    {!candidate.hasKey ? ` (${t("settings.noApiKey", { defaultValue: "no key" })})` : ""}
                  </option>
                ))}
              </select>
            </div>

            {isLocalMode ? (
              <div className="grid grid-cols-1 gap-3">
                <div>
                  <label htmlFor="ai-provider-base-url" className="text-xs text-muted-foreground block mb-1">
                    {t("settings.baseUrl")}
                  </label>
                  <Input
                    id="ai-provider-base-url"
                    type="text"
                    value={localAiConfig.base_url}
                    onChange={(e) => handleLocalBaseUrlChange(e.target.value)}
                    placeholder={DEFAULT_LOCAL_BASE_URL}
                    className="font-mono"
                  />
                </div>

                <div>
                  <label htmlFor="ai-provider-model" className="text-xs text-muted-foreground block mb-1">
                    {t("settings.model")}
                  </label>
                  <Input
                    id="ai-provider-model"
                    type="text"
                    value={localAiConfig.model}
                    onChange={(e) => handleLocalModelChange(e.target.value)}
                    placeholder={DEFAULT_LOCAL_MODEL}
                  />
                </div>
              </div>
            ) : (
              <div className="rounded-xl border border-border bg-card/40 px-3.5 py-3 space-y-1.5">
                <div className="text-xs font-medium text-foreground">
                  {selectedProvider?.label ??
                    t("settings.providerNone", { defaultValue: "None — choose from Models" })}
                </div>
                <div className="text-[11px] leading-relaxed text-muted-foreground">
                  {selectedProvider
                    ? selectedProvider.hasKey
                      ? t("settings.aiProviderManagedHint", {
                          defaultValue: "Base URL, API key, and model are reused from the Models provider.",
                        })
                      : t("settings.qualityEngineMissingHint", {
                          defaultValue: "Connect a provider in Models before using the quality lane.",
                        })
                    : providerCandidates.length === 0
                      ? t("settings.noQualityProviders", { defaultValue: "No Models providers connected yet" })
                      : t("settings.selectProviderHint", {
                          defaultValue: "Choose a Claude or Codex provider from Models for summary and scan.",
                        })}
                </div>
              </div>
            )}

            <div className="pt-2 border-t border-border/40">
              <div className="flex items-center gap-1.5 mb-2.5">
                <span className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                  {t("settings.scanOptimization", { defaultValue: "Security Scan" })}
                </span>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label htmlFor="ai-provider-context-window" className="text-xs text-muted-foreground block mb-1">
                    {t("settings.contextWindow", { defaultValue: "Context Window" })}
                  </label>
                  <div className="flex items-center gap-2.5">
                    <Input
                      id="ai-provider-context-window"
                      type="number"
                      min={1}
                      max={2048}
                      step={1}
                      value={localAiConfig.context_window_k}
                      onChange={(e) => {
                        const val = Math.min(2048, Math.max(1, Number(e.target.value) || 128));
                        onConfigChange({ ...localAiConfig, context_window_k: val });
                      }}
                      className="w-24 font-mono tabular-nums"
                    />
                    <span className="text-xs font-mono text-foreground tabular-nums shrink-0">K</span>
                    <span className="text-[10px] text-muted-foreground">tokens</span>
                  </div>
                  <p className="text-[10px] text-muted-foreground/60 mt-1">
                    {t("settings.contextWindowHint", { defaultValue: "Your model's max context window." })}
                  </p>
                </div>

                <div>
                  <label htmlFor="ai-provider-concurrency" className="text-xs text-muted-foreground block mb-1">
                    {t("settings.aiConcurrency", { defaultValue: "AI Concurrency" })}
                  </label>
                  <div className="flex items-center gap-2.5">
                    <Input
                      id="ai-provider-concurrency"
                      type="number"
                      min={1}
                      max={20}
                      step={1}
                      value={localAiConfig.max_concurrent_requests}
                      onChange={(e) =>
                        onConfigChange({
                          ...localAiConfig,
                          max_concurrent_requests: clampConcurrency(Number(e.target.value)),
                        })
                      }
                      className="w-20 font-mono tabular-nums"
                    />
                  </div>
                  <p className="text-[10px] text-muted-foreground/60 mt-1">
                    {t("settings.aiConcurrencyOverride", {
                      defaultValue: "Adjust down if you encounter API rate limits.",
                    })}
                  </p>
                </div>
              </div>
            </div>

            <div className="flex items-center justify-end gap-3 pt-1">
              <div className="flex items-center min-h-5">
                {aiSaving ? (
                  <span className="text-xs text-muted-foreground">{t("common.saving")}</span>
                ) : aiSaved ? (
                  <span className="text-xs text-success">{t("common.saved")}</span>
                ) : null}
              </div>
              <Button
                size="sm"
                variant="outline"
                onClick={onTestConnection}
                disabled={aiSaving || aiTesting || !localAiConfig.enabled || (!isLocalMode && !hasResolvedProvider)}
                className="min-w-[112px] px-3 relative"
              >
                <div className="flex items-center justify-center gap-1.5 min-w-max">
                  {aiTesting && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
                  {!aiTesting && aiTestResult === "success" && <CheckCircle className="w-3.5 h-3.5 text-success" />}
                  {!aiTesting && aiTestResult === "error" && <XCircle className="w-3.5 h-3.5 text-destructive" />}
                  {!aiTesting && !aiTestResult && <Zap className="w-3.5 h-3.5" />}

                  <span>
                    {aiTesting
                      ? t("common.testing")
                      : aiTestResult === "success" && typeof aiTestLatency === "number"
                        ? `${t("common.connected")} (${aiTestLatency}ms)`
                        : aiTestResult === "success"
                          ? t("common.connected")
                          : aiTestResult === "error"
                            ? t("common.failed")
                            : t("settings.testConnection")}
                  </span>
                </div>
              </Button>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

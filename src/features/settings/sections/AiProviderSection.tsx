import { CheckCircle, ChevronDown, Loader2, Sparkles, XCircle, Zap } from "lucide-react";
import { memo, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { Switch } from "../../../components/ui/switch";
import { cn } from "../../../lib/utils";
import type { AiConfig } from "../../../types";
import { AppAiModelsPicker } from "../components/AppAiModelsPicker";

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

const DEFAULT_LOCAL_BASE_URL = "http://127.0.0.1:11434/v1";
const DEFAULT_LOCAL_MODEL = "llama3.1:8b";

type AiSourceMode = "models" | "local";

function resolveAiSource(config: AiConfig): AiSourceMode {
  if (config.provider_ref || config.api_format !== "local") return "models";
  return "local";
}

export const AiProviderSection = memo(function AiProviderSection({
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
  const aiSource = resolveAiSource(localAiConfig);
  const canTestConnection = localAiConfig.enabled && (aiSource === "local" || Boolean(localAiConfig.provider_ref));

  const clampConcurrency = (value: number) => Math.min(20, Math.max(1, value || 1));

  const badgeLabel = useMemo(() => {
    if (!localAiConfig.enabled) return null;
    if (aiSource === "models") {
      const proto =
        localAiConfig.provider_ref?.app_id === "codex" || localAiConfig.api_format === "openai" ? "OpenAI" : "Claude";
      if (!localAiConfig.provider_ref) {
        return `${t("settings.modelsProvider", { defaultValue: "Models 供应商" })} · ${t("common.none")}`;
      }
      return `${t("settings.modelsProvider", { defaultValue: "Models 供应商" })} · ${proto}`;
    }
    return `${t("settings.localOllama", { defaultValue: "本地 Ollama" })} · ${localAiConfig.model}`;
  }, [aiSource, localAiConfig, t]);

  const setSource = (source: AiSourceMode) => {
    if (source === "local") {
      onConfigChange({
        ...localAiConfig,
        api_format: "local",
        provider_ref: null,
        base_url: localAiConfig.local_preset.base_url || DEFAULT_LOCAL_BASE_URL,
        api_key: "",
        model: localAiConfig.local_preset.model || DEFAULT_LOCAL_MODEL,
      });
    } else {
      onConfigChange({
        ...localAiConfig,
        provider_ref: localAiConfig.provider_ref,
        api_format:
          localAiConfig.provider_ref?.app_id === "codex"
            ? "openai"
            : localAiConfig.api_format === "openai"
              ? "openai"
              : "anthropic",
      });
    }
  };

  return (
    <section>
      <div className="mb-3 flex items-center justify-between px-1">
        <div className="flex items-center gap-2">
          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-emerald-500/20 bg-emerald-500/10">
            <Sparkles className="h-4 w-4 text-emerald-500" />
          </div>
          <h2 className="text-sm font-semibold tracking-tight text-foreground">{t("settings.aiProvider")}</h2>
          {badgeLabel && (
            <span className="ml-2 rounded-md border border-border bg-muted/50 px-2 py-0.5 text-xs text-muted-foreground">
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
          "overflow-hidden rounded-xl border border-border transition-colors",
          localAiConfig.enabled ? "bg-card" : "bg-card/50",
        )}
      >
        <button
          type="button"
          onClick={onToggleExpanded}
          className="flex w-full cursor-pointer items-center justify-between px-4 py-3 transition-colors hover:bg-muted/30"
        >
          <span className="text-sm font-medium text-foreground">
            {t("settings.aiConfigTitle", { defaultValue: "AI Summary & Scan" })}
          </span>
          <ChevronDown
            className={cn(
              "h-4 w-4 text-muted-foreground transition-transform duration-200",
              !aiExpanded && "-rotate-90",
            )}
          />
        </button>

        {aiExpanded && (
          <div className="space-y-3 border-t border-border px-4 pb-4 pt-1">
            <div className="grid grid-cols-1 gap-2 pt-2 sm:grid-cols-2">
              {(["models", "local"] as const).map((source) => (
                <button
                  key={source}
                  type="button"
                  aria-pressed={aiSource === source}
                  disabled={aiSaving}
                  onClick={() => setSource(source)}
                  className={cn(
                    "min-h-11 rounded-xl border px-4 text-sm font-semibold transition-colors disabled:cursor-not-allowed disabled:opacity-60",
                    aiSource === source
                      ? "border-primary/60 bg-primary/10 text-primary"
                      : "border-border/60 text-muted-foreground hover:border-border hover:text-foreground",
                  )}
                >
                  {source === "models"
                    ? t("settings.aiSourceModels", { defaultValue: "Models 供应商" })
                    : t("settings.aiSourceLocal", { defaultValue: "本地 Ollama" })}
                </button>
              ))}
            </div>

            {aiSource === "models" ? (
              <AppAiModelsPicker config={localAiConfig} disabled={aiSaving} onConfigChange={onConfigChange} />
            ) : (
              <div className="grid grid-cols-1 gap-3">
                <div>
                  <label htmlFor="ai-provider-base-url" className="mb-1 block text-xs text-muted-foreground">
                    {t("settings.baseUrl")}
                  </label>
                  <Input
                    id="ai-provider-base-url"
                    type="text"
                    value={localAiConfig.base_url}
                    onChange={(e) =>
                      onConfigChange({
                        ...localAiConfig,
                        api_format: "local",
                        provider_ref: null,
                        base_url: e.target.value,
                        api_key: "",
                        local_preset: {
                          ...localAiConfig.local_preset,
                          base_url: e.target.value,
                          api_key: "",
                          model: localAiConfig.model,
                        },
                      })
                    }
                    placeholder={DEFAULT_LOCAL_BASE_URL}
                    className="font-mono"
                  />
                </div>

                <div>
                  <label htmlFor="ai-provider-model" className="mb-1 block text-xs text-muted-foreground">
                    {t("settings.model")}
                  </label>
                  <Input
                    id="ai-provider-model"
                    type="text"
                    value={localAiConfig.model}
                    onChange={(e) =>
                      onConfigChange({
                        ...localAiConfig,
                        api_format: "local",
                        provider_ref: null,
                        api_key: "",
                        model: e.target.value,
                        local_preset: {
                          ...localAiConfig.local_preset,
                          base_url: localAiConfig.base_url,
                          api_key: "",
                          model: e.target.value,
                        },
                      })
                    }
                    placeholder={DEFAULT_LOCAL_MODEL}
                  />
                </div>
              </div>
            )}

            <div className="border-t border-border/40 pt-2">
              <div className="mb-2.5 flex items-center gap-1.5">
                <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                  {t("settings.scanOptimization", { defaultValue: "AI Optimization" })}
                </span>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label htmlFor="ai-provider-context-window" className="mb-1 block text-xs text-muted-foreground">
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
                    <span className="shrink-0 font-mono text-xs tabular-nums text-foreground">K</span>
                    <span className="text-[10px] text-muted-foreground">tokens</span>
                  </div>
                </div>

                <div>
                  <label htmlFor="ai-provider-concurrency" className="mb-1 block text-xs text-muted-foreground">
                    {t("settings.aiConcurrency", { defaultValue: "AI Concurrency" })}
                  </label>
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
              </div>
            </div>

            <div className="flex items-center justify-end gap-3 pt-1">
              <div className="flex min-h-5 items-center">
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
                disabled={aiSaving || aiTesting || !canTestConnection}
                className="relative h-10 min-w-[132px] rounded-xl px-4 text-sm"
              >
                <div className="flex min-w-max items-center justify-center gap-1.5">
                  {aiTesting && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                  {!aiTesting && aiTestResult === "success" && <CheckCircle className="h-3.5 w-3.5 text-success" />}
                  {!aiTesting && aiTestResult === "error" && <XCircle className="h-3.5 w-3.5 text-destructive" />}
                  {!aiTesting && !aiTestResult && <Zap className="h-3.5 w-3.5" />}

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
});

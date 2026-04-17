import { CheckCircle, ChevronDown, Eye, EyeOff, Loader2, Sparkles, XCircle, Zap } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { Switch } from "../../../components/ui/switch";
import { useNavigation } from "../../../hooks/useNavigation";
import { cn } from "../../../lib/utils";
import type { AiConfig, FormatPreset } from "../../../types";

interface AiProviderSectionProps {
  localAiConfig: AiConfig;

  ready: boolean;
  aiExpanded: boolean;
  aiSaving: boolean;
  aiSaved: boolean;
  aiTesting: boolean;
  aiTestResult: "success" | "error" | null;
  aiTestLatency: number | null;
  showApiKey: boolean;
  onToggleExpanded: () => void;
  onEnabledChange: (enabled: boolean) => void;
  onConfigChange: (next: AiConfig) => void;
  onToggleShowApiKey: () => void;
  onTestConnection: () => void;
}

/** Get the preset key for a given api_format */
function presetKeyFor(format: AiConfig["api_format"]): "openai_preset" | "anthropic_preset" | "local_preset" {
  switch (format) {
    case "anthropic":
      return "anthropic_preset";
    case "local":
      return "local_preset";
    default:
      return "openai_preset";
  }
}

/** Build a FormatPreset from the active fields of config */
function activeToPreset(config: AiConfig): FormatPreset {
  return { base_url: config.base_url, api_key: config.api_key, model: config.model };
}

export function AiProviderSection({
  localAiConfig,

  ready,
  aiExpanded,
  aiSaving,
  aiSaved,
  aiTesting,
  aiTestResult,
  showApiKey,
  onToggleExpanded,
  onEnabledChange,
  onConfigChange,
  onToggleShowApiKey,
  onTestConnection,
  aiTestLatency,
}: AiProviderSectionProps) {
  const { t } = useTranslation();
  const { navigateToModels } = useNavigation();
  const isAnthropicFormat = localAiConfig.api_format === "anthropic";
  const isLocalFormat = localAiConfig.api_format === "local";
  const aiApiKeyPlaceholder = isLocalFormat
    ? t("settings.localApiKeyOptional", { defaultValue: "Optional — most local models don't need this" })
    : isAnthropicFormat
      ? "sk-ant-..."
      : "sk-...";
  const aiBaseUrlPlaceholder = isLocalFormat
    ? "http://127.0.0.1:11434/v1"
    : isAnthropicFormat
      ? "https://api.anthropic.com"
      : "https://api.openai.com/v1";
  const aiModelPlaceholder = isLocalFormat ? "llama3.1:8b" : isAnthropicFormat ? "claude-sonnet-4-20250514" : "gpt-5.4";
  const clampConcurrency = (value: number) => Math.min(20, Math.max(1, value || 1));
  const formControlClass =
    "flex h-9 w-full rounded-xl border border-input-border bg-input backdrop-blur-sm px-3 text-sm text-foreground shadow-sm transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:border-primary/60";

  return (
    <section>
      <div className="flex items-center justify-between mb-3 px-1">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-emerald-500/10 flex items-center justify-center shrink-0 border border-emerald-500/20">
            <Sparkles className="w-4 h-4 text-emerald-500" />
          </div>
          <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.aiProvider")}</h2>
          {localAiConfig.enabled && (localAiConfig.api_key || isLocalFormat) && (
            <span className="text-xs text-muted-foreground ml-2 px-2 py-0.5 rounded-md bg-muted/50 border border-border">
              {isLocalFormat
                ? t("settings.localModel", { defaultValue: "Local" })
                : localAiConfig.api_format === "anthropic"
                  ? "Anthropic"
                  : "OpenAI"}{" "}
              · {localAiConfig.model}
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
            <div className="grid grid-cols-1 gap-3">
              <div>
                <label htmlFor="ai-provider-api-format" className="text-xs text-muted-foreground block mb-1">
                  {t("settings.apiFormat")}
                </label>
                <select
                  id="ai-provider-api-format"
                  value={localAiConfig.api_format}
                  onChange={(e) => {
                    const nextFormat = e.target.value as "openai" | "anthropic" | "local";
                    const currentFormat = localAiConfig.api_format;

                    if (nextFormat === currentFormat) return;

                    // Save current active values to the current format's preset
                    const currentPresetKey = presetKeyFor(currentFormat);
                    const savedPresets = {
                      ...localAiConfig,
                      [currentPresetKey]: activeToPreset(localAiConfig),
                    };

                    // Load the target format's preset values
                    const nextPresetKey = presetKeyFor(nextFormat);
                    const nextPreset = savedPresets[nextPresetKey] as FormatPreset;

                    // Default model fallbacks for empty presets
                    const defaultModel =
                      nextFormat === "anthropic"
                        ? "claude-sonnet-4-20250514"
                        : nextFormat === "local"
                          ? "llama3.1:8b"
                          : "gpt-5.4";

                    onConfigChange({
                      ...savedPresets,
                      api_format: nextFormat,
                      base_url: nextPreset.base_url,
                      api_key: nextPreset.api_key,
                      model: nextPreset.model || defaultModel,
                    });
                  }}
                  className={`${formControlClass} pr-8`}
                >
                  <option value="openai">{t("settings.openaiCompatible")}</option>
                  <option value="anthropic">{t("settings.anthropicMessages")}</option>
                  <option value="local">{t("settings.localModel", { defaultValue: "Local Model (Ollama)" })}</option>
                </select>
              </div>
            </div>

            <div>
              <label htmlFor="ai-provider-base-url" className="text-xs text-muted-foreground block mb-1">
                {t("settings.baseUrl")}
              </label>
              <Input
                id="ai-provider-base-url"
                type="text"
                value={localAiConfig.base_url}
                onChange={(e) => onConfigChange({ ...localAiConfig, base_url: e.target.value })}
                placeholder={aiBaseUrlPlaceholder}
                className="font-mono"
              />
            </div>

            <div>
              <label htmlFor="ai-provider-api-key" className="text-xs text-muted-foreground block mb-1">
                {t("settings.apiKey")}
                {isLocalFormat && (
                  <span className="ml-1.5 text-[10px] text-muted-foreground/60">({t("common.optional")})</span>
                )}
              </label>
              <div className="relative">
                <Input
                  id="ai-provider-api-key"
                  type={showApiKey ? "text" : "password"}
                  value={localAiConfig.api_key}
                  onChange={(e) => onConfigChange({ ...localAiConfig, api_key: e.target.value })}
                  placeholder={aiApiKeyPlaceholder}
                  className="pr-9 font-mono"
                />
                <button
                  type="button"
                  onClick={onToggleShowApiKey}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors cursor-pointer p-1.5 rounded-md focus-ring"
                >
                  {showApiKey ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                </button>
              </div>
            </div>

            <div>
              <label htmlFor="ai-provider-model" className="text-xs text-muted-foreground block mb-1">
                {t("settings.model")}
              </label>
              <Input
                id="ai-provider-model"
                type="text"
                value={localAiConfig.model}
                onChange={(e) => onConfigChange({ ...localAiConfig, model: e.target.value })}
                placeholder={aiModelPlaceholder}
                list="ai-model-suggestions"
              />
              <datalist id="ai-model-suggestions">
                {isAnthropicFormat ? (
                  <>
                    <option value="claude-sonnet-4-20250514" />
                    <option value="claude-opus-4-20250514" />
                    <option value="claude-3-7-sonnet-20250219" />
                    <option value="claude-3-5-sonnet-20241022" />
                  </>
                ) : isLocalFormat ? (
                  <>
                    <option value="llama3.1:8b" />
                    <option value="llama3.1:70b" />
                    <option value="qwen2.5:7b" />
                    <option value="qwen2.5:32b" />
                    <option value="deepseek-r1:7b" />
                    <option value="deepseek-r1:32b" />
                    <option value="gemma2:9b" />
                    <option value="mistral:7b" />
                    <option value="phi3:mini" />
                  </>
                ) : (
                  <>
                    <option value="gpt-5.4" />
                    <option value="gpt-4o" />
                    <option value="gpt-4.1-mini" />
                    <option value="gpt-4.1-nano" />
                    <option value="deepseek-chat" />
                    <option value="qwen-plus" />
                    <option value="claude-sonnet-4-20250514" />
                  </>
                )}
              </datalist>
            </div>

            {/* ── Context & Concurrency ─── */}
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
                disabled={
                  aiSaving || aiTesting || !localAiConfig.enabled || (!localAiConfig.api_key.trim() && !isLocalFormat)
                }
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

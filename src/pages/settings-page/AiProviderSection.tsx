import { useTranslation } from "react-i18next";
import {
  CheckCircle,
  ChevronDown,
  Eye,
  EyeOff,
  Loader2,
  Sparkles,
  XCircle,
  Zap,
} from "lucide-react";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Switch } from "../../components/ui/switch";
import { cn } from "../../lib/utils";
import type { AiConfig } from "../../types";

interface AiProviderSectionProps {
  localAiConfig: AiConfig;
  ready: boolean;
  aiExpanded: boolean;
  aiSaving: boolean;
  aiSaved: boolean;
  aiTesting: boolean;
  aiTestResult: "success" | "error" | null;
  showApiKey: boolean;
  onToggleExpanded: () => void;
  onEnabledChange: (enabled: boolean) => void;
  onConfigChange: (next: AiConfig) => void;
  onToggleShowApiKey: () => void;
  onTestConnection: () => void;
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
}: AiProviderSectionProps) {
  const { t } = useTranslation();
  const isAnthropicFormat = localAiConfig.api_format === "anthropic";
  const aiApiKeyPlaceholder = isAnthropicFormat ? "sk-ant-..." : "sk-...";
  const aiBaseUrlPlaceholder = isAnthropicFormat ? "https://api.anthropic.com" : "https://api.openai.com/v1";
  const aiModelPlaceholder = isAnthropicFormat ? "claude-sonnet-4-20250514" : "gpt-5.4";
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
          {localAiConfig.enabled && localAiConfig.api_key && (
            <span className="text-xs text-muted-foreground ml-2 px-2 py-0.5 rounded-md bg-muted/50 border border-border">
              {localAiConfig.api_format === "anthropic" ? "Anthropic" : "OpenAI"} · {localAiConfig.model}
            </span>
          )}
        </div>
        
        {ready ? (
          <Switch
            checked={localAiConfig.enabled}
            onCheckedChange={onEnabledChange}
            disabled={aiSaving}
          />
        ) : (
          <div className="h-5 w-9 rounded-full border border-border bg-muted/60" />
        )}
      </div>

      <div className={cn("rounded-xl border border-border overflow-hidden transition-colors", localAiConfig.enabled ? "bg-card" : "bg-card/50")}>
        <button onClick={onToggleExpanded} className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/30 transition-colors cursor-pointer">
          <span className="text-sm font-medium text-foreground">
            {t("settings.aiConfigTitle", { defaultValue: "AI Configuration" })}
          </span>
          <ChevronDown
            className={cn(
              "w-4 h-4 text-muted-foreground transition-transform duration-200",
              !aiExpanded && "-rotate-90"
            )}
          />
        </button>

        {aiExpanded && (
          <div className="px-4 pb-4 pt-1 border-t border-border space-y-3">
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-xs text-muted-foreground block mb-1">{t("settings.apiFormat")}</label>
                <select
                  value={localAiConfig.api_format}
                  onChange={(e) => {
                    const nextFormat = e.target.value as "openai" | "anthropic";
                    const currentUrl = localAiConfig.base_url.trim();
                    const urlToSet = (currentUrl === "https://api.openai.com/v1" || currentUrl === "https://api.anthropic.com")
                      ? ""
                      : currentUrl;
                    
                    onConfigChange({
                      ...localAiConfig,
                      api_format: nextFormat,
                      base_url: urlToSet,
                      model: localAiConfig.model.trim()
                        ? localAiConfig.model
                        : nextFormat === "anthropic"
                        ? "claude-sonnet-4-20250514"
                        : "gpt-5.4",
                    });
                  }}
                  className={`${formControlClass} pr-8`}
                >
                  <option value="openai">{t("settings.openaiCompatible")}</option>
                  <option value="anthropic">{t("settings.anthropicMessages")}</option>
                </select>
              </div>
              <div>
                <label className="text-xs text-muted-foreground block mb-1">
                  {t("settings.translationLanguage")}
                </label>
                <select
                  value={localAiConfig.target_language}
                  onChange={(e) => onConfigChange({ ...localAiConfig, target_language: e.target.value })}
                  className={`${formControlClass} pr-8`}
                >
                  <option value="zh-CN">{t("settings.langZhCn")}</option>
                  <option value="zh-TW">{t("settings.langZhTw")}</option>
                  <option value="en">{t("settings.langEn")}</option>
                  <option value="hi">{t("settings.langHi")}</option>
                  <option value="es">{t("settings.langEs")}</option>
                  <option value="ar">{t("settings.langAr")}</option>
                  <option value="pt-BR">{t("settings.langPtBr")}</option>
                  <option value="ru">{t("settings.langRu")}</option>
                  <option value="ja">{t("settings.langJa")}</option>
                  <option value="fr">{t("settings.langFr")}</option>
                  <option value="de">{t("settings.langDe")}</option>
                  <option value="ko">{t("settings.langKo")}</option>
                </select>
              </div>
            </div>

            <div className="rounded-xl border border-border/70 bg-muted/20 px-3 py-3 space-y-3">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <p className="text-xs font-medium text-foreground">
                    {t("settings.enableMyMemoryShortText")}
                  </p>
                  <p className="text-[10px] text-muted-foreground mt-1">
                    {t("settings.enableMyMemoryShortTextHint")}
                  </p>
                </div>
                <Switch
                  checked={localAiConfig.use_mymemory_for_short_text}
                  onCheckedChange={(checked) =>
                    onConfigChange({ ...localAiConfig, use_mymemory_for_short_text: checked })
                  }
                  disabled={aiSaving}
                />
              </div>

              <div>
                <label className="text-xs text-muted-foreground block mb-1">
                  {t("settings.shortTextPriority")}
                </label>
                <select
                  value={localAiConfig.short_text_priority}
                  onChange={(e) =>
                    onConfigChange({
                      ...localAiConfig,
                      short_text_priority: e.target.value as "ai_first" | "mymemory_first",
                    })
                  }
                  className={`${formControlClass} pr-8`}
                  disabled={!localAiConfig.use_mymemory_for_short_text}
                >
                  <option value="ai_first">{t("settings.shortTextPriorityAiFirst")}</option>
                  <option value="mymemory_first">{t("settings.shortTextPriorityMyMemoryFirst")}</option>
                </select>
              </div>
            </div>

            <div>
              <label className="text-xs text-muted-foreground block mb-1">{t("settings.baseUrl")}</label>
              <Input
                type="text"
                value={localAiConfig.base_url}
                onChange={(e) => onConfigChange({ ...localAiConfig, base_url: e.target.value })}
                placeholder={aiBaseUrlPlaceholder}
                className="font-mono"
              />
            </div>

            <div>
              <label className="text-xs text-muted-foreground block mb-1">{t("settings.apiKey")}</label>
              <div className="relative">
                <Input
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
                  {showApiKey ? (
                    <EyeOff className="w-3.5 h-3.5" />
                  ) : (
                    <Eye className="w-3.5 h-3.5" />
                  )}
                </button>
              </div>
            </div>

            <div>
              <label className="text-xs text-muted-foreground block mb-1">{t("settings.model")}</label>
              <Input
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
                <span className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">{t("settings.scanOptimization", { defaultValue: "Security Scan" })}</span>
              </div>
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="text-xs text-muted-foreground block mb-1">
                    {t("settings.contextWindow", { defaultValue: "Context Window" })}
                  </label>
                  <div className="flex items-center gap-2.5">
                    <Input
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
                    <span className="text-xs font-mono text-foreground tabular-nums shrink-0">
                      K
                    </span>
                    <span className="text-[10px] text-muted-foreground">tokens</span>
                  </div>
                  <p className="text-[10px] text-muted-foreground/60 mt-1">{t("settings.contextWindowHint", { defaultValue: "Your model's max context window." })}</p>
                </div>

                <div>
                  <label className="text-xs text-muted-foreground block mb-1">
                    {t("settings.aiConcurrency", { defaultValue: "AI Concurrency" })}
                  </label>
                  <div className="flex items-center gap-2.5">
                    <Input
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
                    {t("settings.aiConcurrencyOverride", { defaultValue: "Adjust down if you encounter API rate limits." })}
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
                disabled={aiSaving || aiTesting || !localAiConfig.enabled || !localAiConfig.api_key.trim()}
              >
                {aiTesting ? (
                  <>
                    <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                    {t("common.testing")}
                  </>
                ) : aiTestResult === "success" ? (
                  <>
                    <CheckCircle className="w-3.5 h-3.5 mr-1.5 text-success" />
                    {t("common.connected")}
                  </>
                ) : aiTestResult === "error" ? (
                  <>
                    <XCircle className="w-3.5 h-3.5 mr-1.5 text-destructive" />
                    {t("common.failed")}
                  </>
                ) : (
                  <>
                    <Zap className="w-3.5 h-3.5 mr-1.5" />
                    {t("settings.testConnection")}
                  </>
                )}
              </Button>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

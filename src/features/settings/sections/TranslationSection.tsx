import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, Eye, EyeOff, Loader2, TestTube2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import { Input } from "../../../components/ui/input";
import { useTranslationApiConfig } from "../../../hooks/useTranslationApiConfig";
import { useTranslationSettings } from "../../../hooks/useTranslationSettings";
import { toast } from "../../../lib/toast";
import { cn } from "../../../lib/utils";
import type { TranslationApiConfig, TranslationSettings } from "../../../types";
import type { ProviderEntry } from "../../models/hooks/useModelProviders";
import { useModelProviders } from "../../models/hooks/useModelProviders";

const AUTO_SAVE_DELAY_MS = 600;

type TestResult = { ok: boolean; latency: number | null; error: string | null };

function sameTranslationSettings(a: TranslationSettings, b: TranslationSettings) {
  return (
    a.target_language === b.target_language &&
    (a.quality_provider_ref?.app_id ?? "") === (b.quality_provider_ref?.app_id ?? "") &&
    (a.quality_provider_ref?.provider_id ?? "") === (b.quality_provider_ref?.provider_id ?? "")
  );
}

function sameApiConfig(a: TranslationApiConfig, b: TranslationApiConfig) {
  return a.deepl_key === b.deepl_key && a.deeplx_url === b.deeplx_url;
}

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

export function TranslationSection() {
  const { t } = useTranslation();
  const { config: apiConfig, loading: apiLoading, saveConfig: saveApiConfig } = useTranslationApiConfig();
  const { settings, readiness, loading: settingsLoading, saveSettings, refreshReadiness } = useTranslationSettings();
  const claudeProviders = useModelProviders("claude");
  const codexProviders = useModelProviders("codex");

  const [localApiConfig, setLocalApiConfig] = useState<TranslationApiConfig>(apiConfig);
  const [localSettings, setLocalSettings] = useState<TranslationSettings>(settings);
  const [expanded, setExpanded] = useState(false);
  const [showSecrets, setShowSecrets] = useState(false);
  const [savingApi, setSavingApi] = useState(false);
  const [savingSettingsState, setSavingSettingsState] = useState(false);
  const [savedIndicator, setSavedIndicator] = useState(false);
  const [testingTarget, setTestingTarget] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, TestResult>>({});

  const ready = !apiLoading && !settingsLoading;

  // Sync remote → local
  useEffect(() => {
    if (ready) setLocalApiConfig(apiConfig);
  }, [apiConfig, ready]);
  useEffect(() => {
    if (ready) setLocalSettings(settings);
  }, [settings, ready]);

  // Auto-save API config
  useEffect(() => {
    if (!ready || savingApi || sameApiConfig(localApiConfig, apiConfig)) return;
    const timer = setTimeout(() => {
      setSavingApi(true);
      saveApiConfig(localApiConfig)
        .then(() => {
          refreshReadiness();
          setSavedIndicator(true);
          setTimeout(() => setSavedIndicator(false), 2000);
        })
        .catch((e) => {
          console.error("Failed to save translation API config:", e);
          toast.error(t("settings.saveTranslationApiFailed", { defaultValue: "Failed to save translation config" }));
          setLocalApiConfig(apiConfig);
        })
        .finally(() => setSavingApi(false));
    }, AUTO_SAVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [localApiConfig, apiConfig, ready, savingApi, saveApiConfig, refreshReadiness, t]);

  // Auto-save translation settings
  useEffect(() => {
    if (!ready || savingSettingsState || sameTranslationSettings(localSettings, settings)) return;
    const timer = setTimeout(() => {
      setSavingSettingsState(true);
      saveSettings(localSettings)
        .then(() => {
          setSavedIndicator(true);
          setTimeout(() => setSavedIndicator(false), 2000);
        })
        .catch((e) => {
          console.error("Failed to save translation settings:", e);
          toast.error(t("settings.saveTranslationFailed", { defaultValue: "Failed to save settings" }));
          setLocalSettings(settings);
        })
        .finally(() => setSavingSettingsState(false));
    }, AUTO_SAVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [localSettings, settings, ready, savingSettingsState, saveSettings, t]);

  // Quality provider candidates
  const qualityCandidates = useMemo(() => {
    const items: { appId: string; providerId: string; label: string; hasKey: boolean }[] = [];
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

  const selectedQualityId = localSettings.quality_provider_ref
    ? `${localSettings.quality_provider_ref.app_id}:${localSettings.quality_provider_ref.provider_id}`
    : "";

  const handleQualityChange = (value: string) => {
    if (!value) {
      setLocalSettings({ ...localSettings, quality_provider_ref: null });
      return;
    }
    const [appId, providerId] = value.split(":");
    setLocalSettings({
      ...localSettings,
      quality_provider_ref: { app_id: appId, provider_id: providerId },
    });
  };

  const handleTest = async (target: string) => {
    setTestingTarget(target);
    try {
      const latency = await invoke<number>("test_translation_provider", { provider: target });
      setTestResults((prev) => ({ ...prev, [target]: { ok: true, latency, error: null } }));
    } catch (e) {
      setTestResults((prev) => ({ ...prev, [target]: { ok: false, latency: null, error: String(e) } }));
    } finally {
      setTestingTarget(null);
    }
  };

  const formControlClass =
    "flex h-9 rounded-xl border border-input-border bg-input backdrop-blur-sm px-3 text-sm text-foreground shadow-sm transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:border-primary/60";
  const labelClass = "text-xs font-medium text-muted-foreground mb-1.5 block";

  if (!ready) return null;

  return (
    <section id="settings-translation">
      {/* Header */}
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center justify-between gap-3 rounded-2xl border border-border bg-card px-5 py-4 cursor-pointer hover:bg-muted/30 transition-colors"
      >
        <div className="flex items-center gap-3 min-w-0">
          <div className="w-9 h-9 rounded-xl bg-primary/10 flex items-center justify-center shrink-0 border border-primary/20">
            <span className="text-primary text-base">🌐</span>
          </div>
          <div className="text-left min-w-0">
            <div className="text-sm font-semibold text-foreground">
              {t("settings.translationApis", { defaultValue: "Translation Center" })}
            </div>
            <div className="text-[11px] text-muted-foreground mt-0.5">
              {readiness.ready
                ? readiness.quality_ready
                  ? t("settings.translationReady", { defaultValue: "DeepL/DeepLX + Quality LLM ready" })
                  : t("settings.translationBasicReady", { defaultValue: "DeepL/DeepLX ready (no quality LLM)" })
                : t("settings.translationNotReady", { defaultValue: "Not configured" })}
              {savedIndicator && (
                <span className="ml-2 text-green-500">{t("settings.saved", { defaultValue: "✓ Saved" })}</span>
              )}
            </div>
          </div>
        </div>
        <ChevronDown
          className={cn("w-4 h-4 text-muted-foreground transition-transform duration-200", expanded && "rotate-180")}
        />
      </button>

      {expanded && (
        <div className="mt-3 space-y-4 px-1">


          {/* ── Target Language ────────────────────────────────────── */}
          <div className="rounded-xl border border-border bg-card/50 p-4 space-y-3">
            <div className="text-xs font-semibold text-foreground">
              {t("settings.targetLanguage", { defaultValue: "Target Language" })}
              <span className="ml-2 text-muted-foreground font-normal">
                {t("settings.translationTargetLanguageHint", {
                  defaultValue: "Target language for all AI translations.",
                })}
              </span>
            </div>

            <div>
              <select
                value={localSettings.target_language}
                onChange={(e) => setLocalSettings({ ...localSettings, target_language: e.target.value })}
                className={cn(formControlClass, "w-full")}
              >
                <option value="zh-CN">🇨🇳 {t("settings.langZhCn", { defaultValue: "Chinese (Simplified)" })}</option>
                <option value="zh-TW">🇹🇼 {t("settings.langZhTw", { defaultValue: "Chinese (Traditional)" })}</option>
                <option value="en">🇺🇸 {t("settings.langEn", { defaultValue: "English" })}</option>
                <option value="ja">🇯🇵 {t("settings.langJa", { defaultValue: "Japanese" })}</option>
                <option value="ko">🇰🇷 {t("settings.langKo", { defaultValue: "Korean" })}</option>
                <option value="fr">🇫🇷 {t("settings.langFr", { defaultValue: "French" })}</option>
                <option value="de">🇩🇪 {t("settings.langDe", { defaultValue: "German" })}</option>
                <option value="es">🇪🇸 {t("settings.langEs", { defaultValue: "Spanish" })}</option>
                <option value="ru">🇷🇺 {t("settings.langRu", { defaultValue: "Russian" })}</option>
              </select>
            </div>
          </div>

          {/* ── DeepL Key ────────────────────────────────────────── */}
          <div className="rounded-xl border border-border bg-card/50 p-4 space-y-3">
            <div className="text-xs font-semibold text-foreground">
              DeepL
              <span className="ml-2 text-muted-foreground font-normal">
                {t("settings.deeplHint", { defaultValue: "Official API — best quality for traditional translation" })}
              </span>
            </div>

            <div>
              <label className={labelClass}>{t("settings.deeplKey", { defaultValue: "API Key" })}</label>
              <div className="flex items-center gap-2">
                <Input
                  type={showSecrets ? "text" : "password"}
                  value={localApiConfig.deepl_key}
                  onChange={(e) => setLocalApiConfig({ ...localApiConfig, deepl_key: e.target.value })}
                  placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx:fx"
                  className={formControlClass}
                />
                <button
                  type="button"
                  onClick={() => setShowSecrets(!showSecrets)}
                  className="w-9 h-9 flex items-center justify-center rounded-xl border border-border hover:bg-muted/50 text-muted-foreground cursor-pointer shrink-0"
                >
                  {showSecrets ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
                </button>
                <button
                  type="button"
                  onClick={() => handleTest("deepl")}
                  disabled={!!testingTarget || !localApiConfig.deepl_key.trim()}
                  className="w-9 h-9 flex items-center justify-center rounded-xl border border-border hover:bg-muted/50 text-muted-foreground cursor-pointer shrink-0 disabled:opacity-40"
                >
                  {testingTarget === "deepl" ? (
                    <Loader2 className="w-4 h-4 animate-spin" />
                  ) : (
                    <TestTube2 className="w-4 h-4" />
                  )}
                </button>
              </div>
              {testResults.deepl && (
                <div className={cn("text-[10px] mt-1", testResults.deepl.ok ? "text-green-500" : "text-red-400")}>
                  {testResults.deepl.ok ? `✓ ${testResults.deepl.latency}ms` : `✗ ${testResults.deepl.error}`}
                </div>
              )}
            </div>
          </div>

          {/* ── DeepLX URL ───────────────────────────────────────── */}
          <div className="rounded-xl border border-border bg-card/50 p-4 space-y-3">
            <div className="text-xs font-semibold text-foreground">
              DeepLX
              <span className="ml-2 text-muted-foreground font-normal">
                <ExternalAnchor
                  href="https://connect.linux.do"
                  className="underline decoration-muted-foreground/40 underline-offset-4 transition-colors hover:text-foreground"
                >
                  {t("settings.deeplxCommunityEndpoint", { defaultValue: "Free community endpoint" })}
                </ExternalAnchor>
                {" — "}
                {t("settings.deeplxHintSuffix", { defaultValue: "always available as fallback" })}
              </span>
            </div>

            <div>
              <label className={labelClass}>
                {t("settings.deeplxUrl", { defaultValue: "Endpoint URL (optional)" })}
              </label>
              <div className="flex items-center gap-2">
                <Input
                  type="text"
                  value={localApiConfig.deeplx_url}
                  onChange={(e) => setLocalApiConfig({ ...localApiConfig, deeplx_url: e.target.value })}
                  placeholder="https://api.deeplx.org/translate"
                  className={formControlClass}
                />
                <button
                  type="button"
                  onClick={() => handleTest("deeplx")}
                  disabled={!!testingTarget}
                  className="w-9 h-9 flex items-center justify-center rounded-xl border border-border hover:bg-muted/50 text-muted-foreground cursor-pointer shrink-0 disabled:opacity-40"
                >
                  {testingTarget === "deeplx" ? (
                    <Loader2 className="w-4 h-4 animate-spin" />
                  ) : (
                    <TestTube2 className="w-4 h-4" />
                  )}
                </button>
              </div>
              {testResults.deeplx && (
                <div className={cn("text-[10px] mt-1", testResults.deeplx.ok ? "text-green-500" : "text-red-400")}>
                  {testResults.deeplx.ok ? `✓ ${testResults.deeplx.latency}ms` : `✗ ${testResults.deeplx.error}`}
                </div>
              )}
            </div>
          </div>

          {/* ── Quality LLM Provider ─────────────────────────────── */}
          <div className="rounded-xl border border-border bg-card/50 p-4 space-y-3">
            <div className="text-xs font-semibold text-foreground">
              {t("settings.qualityEngine", { defaultValue: "Quality Engine (SKILL.md)" })}
              <span className="ml-2 text-muted-foreground font-normal">
                {t("settings.qualityEngineHint", {
                  defaultValue: "Uses a Models provider for AI-powered Markdown translation",
                })}
              </span>
            </div>

            <div>
              <label className={labelClass}>{t("settings.qualityProvider", { defaultValue: "Provider" })}</label>
              <div className="flex items-center gap-2">
                <select
                  value={selectedQualityId}
                  onChange={(e) => handleQualityChange(e.target.value)}
                  className={`${formControlClass} flex-1`}
                >
                  <option value="">
                    {t("settings.qualityProviderNone", { defaultValue: "None — use DeepL/DeepLX for SKILL.md" })}
                  </option>
                  {qualityCandidates.map((candidate) => (
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
                {selectedQualityId && (
                  <button
                    type="button"
                    onClick={() => handleTest(`quality:${selectedQualityId}`)}
                    disabled={!!testingTarget}
                    className="w-9 h-9 flex items-center justify-center rounded-xl border border-border hover:bg-muted/50 text-muted-foreground cursor-pointer shrink-0 disabled:opacity-40"
                  >
                    {testingTarget?.startsWith("quality:") ? (
                      <Loader2 className="w-4 h-4 animate-spin" />
                    ) : (
                      <TestTube2 className="w-4 h-4" />
                    )}
                  </button>
                )}
              </div>
              {testResults[`quality:${selectedQualityId}`] && (
                <div
                  className={cn(
                    "text-[10px] mt-1",
                    testResults[`quality:${selectedQualityId}`].ok ? "text-green-500" : "text-red-400",
                  )}
                >
                  {testResults[`quality:${selectedQualityId}`].ok
                    ? `✓ ${testResults[`quality:${selectedQualityId}`].latency}ms`
                    : `✗ ${testResults[`quality:${selectedQualityId}`].error}`}
                </div>
              )}
            </div>
          </div>

          {/* Route explanation */}
          <div className="rounded-xl border border-dashed border-border/60 bg-muted/20 px-4 py-3 text-[11px] text-muted-foreground space-y-1">
            <div className="font-medium text-foreground/70 mb-1">
              {t("settings.routeExplanationTitle", { defaultValue: "How translation routes work" })}
            </div>
            <div>
              •{" "}
              {t("settings.routeShortText", {
                defaultValue: "Short text (descriptions): DeepL → DeepLX",
              })}
            </div>
            <div>
              •{" "}
              {t("settings.routeMarkdown", {
                defaultValue: "SKILL.md: Quality LLM (if set) → DeepL → DeepLX",
              })}
            </div>
            <div>
              •{" "}
              {t("settings.routeFallback", {
                defaultValue: "DeepLX is always available as a free fallback.",
              })}
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, Eye, EyeOff, Globe, Languages, Loader2, Sparkles, TestTube2 } from "lucide-react";
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
  return a.deepl_key === b.deepl_key && a.deeplx_key === b.deeplx_key && a.deeplx_url === b.deeplx_url;
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

  useEffect(() => {
    if (ready) setLocalApiConfig(apiConfig);
  }, [apiConfig, ready]);

  useEffect(() => {
    if (ready) setLocalSettings(settings);
  }, [settings, ready]);

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
        .catch((error) => {
          console.error("Failed to save translation API config:", error);
          toast.error(t("settings.saveTranslationApiFailed", { defaultValue: "Failed to save translation config" }));
          setLocalApiConfig(apiConfig);
        })
        .finally(() => setSavingApi(false));
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [localApiConfig, apiConfig, ready, savingApi, saveApiConfig, refreshReadiness, t]);

  useEffect(() => {
    if (!ready || savingSettingsState || sameTranslationSettings(localSettings, settings)) return;

    const timer = setTimeout(() => {
      setSavingSettingsState(true);
      saveSettings(localSettings)
        .then(() => {
          setSavedIndicator(true);
          setTimeout(() => setSavedIndicator(false), 2000);
        })
        .catch((error) => {
          console.error("Failed to save translation settings:", error);
          toast.error(t("settings.saveTranslationFailed", { defaultValue: "Failed to save settings" }));
          setLocalSettings(settings);
        })
        .finally(() => setSavingSettingsState(false));
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [localSettings, settings, ready, savingSettingsState, saveSettings, t]);

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

  const languageOptions = useMemo(
    () => [
      { value: "zh-CN", emoji: "🇨🇳", label: t("settings.langZhCn", { defaultValue: "Chinese (Simplified)" }) },
      { value: "zh-TW", emoji: "繁", label: t("settings.langZhTw", { defaultValue: "Chinese (Traditional)" }) },
      { value: "en", emoji: "🇺🇸", label: t("settings.langEn", { defaultValue: "English" }) },
      { value: "ja", emoji: "🇯🇵", label: t("settings.langJa", { defaultValue: "Japanese" }) },
      { value: "ko", emoji: "🇰🇷", label: t("settings.langKo", { defaultValue: "Korean" }) },
      { value: "fr", emoji: "🇫🇷", label: t("settings.langFr", { defaultValue: "French" }) },
      { value: "de", emoji: "🇩🇪", label: t("settings.langDe", { defaultValue: "German" }) },
      { value: "es", emoji: "🇪🇸", label: t("settings.langEs", { defaultValue: "Spanish" }) },
      { value: "ru", emoji: "🇷🇺", label: t("settings.langRu", { defaultValue: "Russian" }) },
    ],
    [t],
  );

  const selectedLanguage =
    languageOptions.find((option) => option.value === localSettings.target_language) ?? languageOptions[0];
  const selectedQualityId = localSettings.quality_provider_ref
    ? `${localSettings.quality_provider_ref.app_id}:${localSettings.quality_provider_ref.provider_id}`
    : "";
  const qualityTestKey = selectedQualityId ? `quality:${selectedQualityId}` : null;
  const selectedQualityCandidate = qualityCandidates.find(
    (candidate) => `${candidate.appId}:${candidate.providerId}` === selectedQualityId,
  );
  const hasQualityProvider = qualityCandidates.some((candidate) => candidate.hasKey);
  const translationStatus = readiness.ready
    ? readiness.quality_ready
      ? t("settings.translationReady", { defaultValue: "DeepL/DeepLX + Quality LLM ready" })
      : t("settings.translationBasicReady", { defaultValue: "DeepL/DeepLX ready (no quality LLM)" })
    : t("settings.translationNotReady", { defaultValue: "Not configured" });
  const readinessBadgeLabel = readiness.quality_ready
    ? t("settings.qualityReady", { defaultValue: "Quality Ready" })
    : readiness.ready
      ? t("settings.instantReady", { defaultValue: "Instant Ready" })
      : t("settings.translationNotReady", { defaultValue: "Not configured" });
  const isSaving = savingApi || savingSettingsState;

  const handleQualityChange = (value: string) => {
    if (!value) {
      setLocalSettings({ ...localSettings, quality_provider_ref: null });
      return;
    }

    const [appId, providerId] = value.split(":");
    if (!appId || !providerId) return;

    setLocalSettings({
      ...localSettings,
      quality_provider_ref: { app_id: appId, provider_id: providerId },
    });
  };

  const handleTest = async (target: string) => {
    setTestingTarget(target);
    try {
      if (!sameApiConfig(localApiConfig, apiConfig)) {
        setSavingApi(true);
        try {
          await saveApiConfig(localApiConfig);
          await refreshReadiness();
          setSavedIndicator(true);
          setTimeout(() => setSavedIndicator(false), 2000);
        } finally {
          setSavingApi(false);
        }
      }

      const latency = await invoke<number>("test_translation_provider", { provider: target });
      setTestResults((prev) => ({ ...prev, [target]: { ok: true, latency, error: null } }));
    } catch (error) {
      setTestResults((prev) => ({ ...prev, [target]: { ok: false, latency: null, error: String(error) } }));
    } finally {
      setTestingTarget(null);
    }
  };

  const formControlClass =
    "flex h-9 w-full rounded-xl border border-input-border bg-input backdrop-blur-sm px-3 text-sm text-foreground shadow-sm transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:border-primary/60";
  const sectionCardClass = "rounded-xl border border-border bg-card/40 px-4 py-4 space-y-3";
  const providerPanelClass = "rounded-xl border border-border/70 bg-background/30 px-3.5 py-3 space-y-3";
  const actionIconButtonClass =
    "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-border bg-card/70 text-muted-foreground transition-colors hover:bg-muted/50 disabled:cursor-not-allowed disabled:opacity-40";
  const labelClass = "block text-xs text-muted-foreground";

  if (!ready) return null;

  return (
    <section>
      <div className="mb-3 flex items-center justify-between gap-3 px-1">
        <div className="flex items-center gap-2 min-w-0">
          <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-sky-500/20 bg-sky-500/10">
            <Languages className="h-4 w-4 text-sky-400" />
          </div>
          <h2 className="truncate text-sm font-semibold tracking-tight text-foreground">
            {t("settings.translationCenterTitle", { defaultValue: "Translation Center" })}
          </h2>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {isSaving ? <span className="text-xs text-muted-foreground">{t("common.saving")}</span> : null}
          {!isSaving && savedIndicator ? <span className="text-xs text-success">{t("common.saved")}</span> : null}
          <span
            className={cn(
              "rounded-md border px-2 py-0.5 text-[10px] font-medium",
              readiness.quality_ready
                ? "border-emerald-500/25 bg-emerald-500/10 text-emerald-400"
                : readiness.ready
                  ? "border-sky-500/25 bg-sky-500/10 text-sky-400"
                  : "border-amber-500/25 bg-amber-500/10 text-amber-400",
            )}
          >
            {readinessBadgeLabel}
          </span>
        </div>
      </div>

      <div className="overflow-hidden rounded-xl border border-border bg-card transition-colors">
        <button
          type="button"
          onClick={() => setExpanded((prev) => !prev)}
          className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left transition-colors hover:bg-muted/30 cursor-pointer"
        >
          <div className="min-w-0 flex-1">
            <div className="min-w-0">
              <div className="text-sm font-medium text-foreground">
                {t("settings.translationApiTitle", { defaultValue: "Translation API Configuration" })}
              </div>
              <div className="mt-0.5 truncate text-[11px] text-muted-foreground">
                {selectedLanguage.label} · {translationStatus}
              </div>
            </div>
          </div>

          <ChevronDown
            className={cn(
              "h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200",
              !expanded && "-rotate-90",
            )}
          />
        </button>

        {expanded ? (
          <div className="space-y-3 border-t border-border px-4 pb-4 pt-1">
            <div className={sectionCardClass}>
              <div className="flex items-start gap-3">
                <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-teal-500/20 bg-teal-500/10">
                  <Languages className="h-4 w-4 text-teal-400" />
                </div>
                <div className="min-w-0">
                  <div className="text-sm font-medium text-foreground">
                    {t("settings.targetLanguage", { defaultValue: "Target Language" })}
                  </div>
                  <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
                    {t("settings.translationTargetLanguageHint", {
                      defaultValue: "Target language for all AI translations.",
                    })}
                  </p>
                </div>
              </div>

              <select
                id="translation-target-language"
                value={localSettings.target_language}
                onChange={(event) => setLocalSettings({ ...localSettings, target_language: event.target.value })}
                className={cn(formControlClass, "pr-8")}
              >
                {languageOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.emoji} {option.label}
                  </option>
                ))}
              </select>
            </div>

            <div className={sectionCardClass}>
              <div className="flex items-start gap-3">
                <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-sky-500/20 bg-sky-500/10">
                  <Globe className="h-4 w-4 text-sky-400" />
                </div>
                <div className="min-w-0">
                  <div className="text-sm font-medium text-foreground">
                    {t("settings.fastProviderCredentials", { defaultValue: "Fast Engine Credentials" })}
                  </div>
                  <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
                    {t("settings.fastProviderCredentialsHint", {
                      defaultValue:
                        "Manage only classic translation API credentials here. Quality accounts stay in Models.",
                    })}
                  </p>
                </div>
              </div>

              <div className={providerPanelClass}>
                <div className="space-y-1">
                  <div className="text-xs font-medium text-foreground">DeepL</div>
                  <p className="text-[11px] leading-relaxed text-muted-foreground">
                    {t("settings.deeplHint", {
                      defaultValue: "Official API — best quality for traditional translation",
                    })}
                  </p>
                </div>

                <div className="space-y-1.5">
                  <label htmlFor="translation-deepl-key" className={labelClass}>
                    {t("settings.deeplKey", { defaultValue: "API Key" })}
                  </label>
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <Input
                      id="translation-deepl-key"
                      type={showSecrets ? "text" : "password"}
                      value={localApiConfig.deepl_key}
                      onChange={(event) => setLocalApiConfig({ ...localApiConfig, deepl_key: event.target.value })}
                      placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx:fx"
                      className="font-mono"
                    />
                    <button
                      type="button"
                      onClick={() => setShowSecrets((prev) => !prev)}
                      className={actionIconButtonClass}
                      aria-label={showSecrets ? t("common.hide") : t("settings.showSecrets", { defaultValue: "Show" })}
                    >
                      {showSecrets ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                    </button>
                    <button
                      type="button"
                      onClick={() => handleTest("deepl")}
                      disabled={!!testingTarget || !localApiConfig.deepl_key.trim()}
                      className={actionIconButtonClass}
                      aria-label={t("settings.testConnection", { defaultValue: "Test Connection" })}
                    >
                      {testingTarget === "deepl" ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <TestTube2 className="h-4 w-4" />
                      )}
                    </button>
                  </div>
                  {testResults.deepl ? (
                    <div className={cn("text-[10px]", testResults.deepl.ok ? "text-success" : "text-destructive")}>
                      {testResults.deepl.ok ? `✓ ${testResults.deepl.latency}ms` : `✗ ${testResults.deepl.error}`}
                    </div>
                  ) : null}
                </div>
              </div>

              <div className={providerPanelClass}>
                <div className="space-y-1">
                  <div className="text-xs font-medium text-foreground">DeepLX</div>
                  <p className="text-[11px] leading-relaxed text-muted-foreground">
                    <ExternalAnchor
                      href="https://connect.linux.do/dash/deeplx"
                      className="underline decoration-muted-foreground/40 underline-offset-4 transition-colors hover:text-foreground"
                    >
                      {t("settings.deeplxCommunityEndpoint", { defaultValue: "Free community endpoint" })}
                    </ExternalAnchor>
                    {" · "}
                    {t("settings.deeplxHintSuffix", { defaultValue: "always available as fallback" })}
                  </p>
                </div>

                <div className="space-y-1.5">
                  <label htmlFor="translation-deeplx-url" className={labelClass}>
                    {t("settings.deeplxUrl", { defaultValue: "Endpoint URL" })}
                  </label>
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <Input
                      id="translation-deeplx-url"
                      type="text"
                      value={localApiConfig.deeplx_url}
                      onChange={(event) => setLocalApiConfig({ ...localApiConfig, deeplx_url: event.target.value })}
                      placeholder="https://api.deeplx.org/translate"
                      className="font-mono"
                    />
                    <button
                      type="button"
                      onClick={() => handleTest("deeplx")}
                      disabled={!!testingTarget}
                      className={actionIconButtonClass}
                      aria-label={t("settings.testConnection", { defaultValue: "Test Connection" })}
                    >
                      {testingTarget === "deeplx" ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <TestTube2 className="h-4 w-4" />
                      )}
                    </button>
                  </div>
                  {testResults.deeplx ? (
                    <div className={cn("text-[10px]", testResults.deeplx.ok ? "text-success" : "text-destructive")}>
                      {testResults.deeplx.ok ? `✓ ${testResults.deeplx.latency}ms` : `✗ ${testResults.deeplx.error}`}
                    </div>
                  ) : null}
                </div>

                <div className="space-y-1.5">
                  <label htmlFor="translation-deeplx-key" className={labelClass}>
                    {t("settings.apiKey", { defaultValue: "API Key" })} ({t("common.optional")})
                  </label>
                  <div className="flex flex-col gap-2 sm:flex-row">
                    <Input
                      id="translation-deeplx-key"
                      type={showSecrets ? "text" : "password"}
                      value={localApiConfig.deeplx_key}
                      onChange={(event) => setLocalApiConfig({ ...localApiConfig, deeplx_key: event.target.value })}
                      placeholder={t("settings.deeplxKeyPlaceholder", {
                        defaultValue: "Optional auth key or token for self-hosted DeepLX nodes",
                      })}
                      className="font-mono"
                    />
                    <button
                      type="button"
                      onClick={() => setShowSecrets((prev) => !prev)}
                      className={actionIconButtonClass}
                      aria-label={showSecrets ? t("common.hide") : t("settings.showSecrets", { defaultValue: "Show" })}
                    >
                      {showSecrets ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                    </button>
                  </div>
                </div>
              </div>
            </div>

            <div className={sectionCardClass}>
              <div className="flex items-start gap-3">
                <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-emerald-500/20 bg-emerald-500/10">
                  <Sparkles className="h-4 w-4 text-emerald-400" />
                </div>
                <div className="min-w-0">
                  <div className="text-sm font-medium text-foreground">
                    {t("settings.qualityEngine", { defaultValue: "Quality Engine (SKILL.md)" })}
                  </div>
                  <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
                    {t("settings.qualityEngineHint", {
                      defaultValue: "Uses a Models provider for AI-powered Markdown translation",
                    })}
                  </p>
                </div>
              </div>

              <div className="space-y-1.5">
                <label htmlFor="translation-quality-provider" className={labelClass}>
                  {t("settings.qualityProvider", { defaultValue: "Provider" })}
                </label>
                <div className="flex flex-col gap-2 sm:flex-row">
                  <select
                    id="translation-quality-provider"
                    value={selectedQualityId}
                    onChange={(event) => handleQualityChange(event.target.value)}
                    className={cn(formControlClass, "pr-8")}
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

                  {qualityTestKey ? (
                    <button
                      type="button"
                      onClick={() => handleTest(qualityTestKey)}
                      disabled={!!testingTarget}
                      className={actionIconButtonClass}
                      aria-label={t("settings.testConnection", { defaultValue: "Test Connection" })}
                    >
                      {testingTarget === qualityTestKey ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <TestTube2 className="h-4 w-4" />
                      )}
                    </button>
                  ) : null}
                </div>

                {qualityTestKey && testResults[qualityTestKey] ? (
                  <div
                    className={cn("text-[10px]", testResults[qualityTestKey].ok ? "text-success" : "text-destructive")}
                  >
                    {testResults[qualityTestKey].ok
                      ? `✓ ${testResults[qualityTestKey].latency}ms`
                      : `✗ ${testResults[qualityTestKey].error}`}
                  </div>
                ) : null}
              </div>

              <div className="rounded-xl border border-border bg-card/40 px-3.5 py-3 space-y-1.5">
                <div className="text-xs font-medium text-foreground">
                  {selectedQualityCandidate?.label ??
                    t("settings.qualityProviderNone", { defaultValue: "None — use DeepL/DeepLX for SKILL.md" })}
                </div>
                <div className="text-[11px] leading-relaxed text-muted-foreground">
                  {selectedQualityCandidate
                    ? t("settings.qualityEngineCopy", {
                        defaultValue:
                          "Used for Markdown fidelity, terminology consistency, and high-quality retranslation. Reuses a provider from Models.",
                      })
                    : hasQualityProvider
                      ? t("settings.qualityEngineFastModeHint", {
                          defaultValue:
                            "Leaving this empty does not affect normal translation. Only high-quality retranslate depends on it.",
                        })
                      : t("settings.noQualityProviders", { defaultValue: "No Models providers connected yet" })}
                </div>
              </div>
            </div>

            <div className="rounded-xl border border-dashed border-border/60 bg-muted/20 px-4 py-3">
              <div className="text-xs font-medium text-foreground">
                {t("settings.routeExplanationTitle", { defaultValue: "How translation routes work" })}
              </div>
              <div className="mt-2 space-y-1 text-[11px] leading-relaxed text-muted-foreground">
                <div>
                  • {t("settings.routeShortText", { defaultValue: "Short text (descriptions): DeepL → DeepLX → MyMemory" })}
                </div>
                <div>
                  •{" "}
                  {t("settings.routeMarkdown", {
                    defaultValue: "SKILL.md: Quality LLM (if set) → DeepL → DeepLX",
                  })}
                </div>
                <div>
                  • {t("settings.routeFallback", { defaultValue: "DeepLX is always available. MyMemory is the final short text fallback." })}
                </div>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </section>
  );
}

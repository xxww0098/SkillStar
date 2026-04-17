import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, Eye, EyeOff, Gauge, Languages, Loader2, ShieldAlert, TestTube2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "../../../components/ui/input";
import { Switch } from "../../../components/ui/switch";
import { useTranslationApiConfig } from "../../../hooks/useTranslationApiConfig";
import { useTranslationSettings } from "../../../hooks/useTranslationSettings";
import { toast } from "../../../lib/toast";
import { cn } from "../../../lib/utils";
import type {
  MymemoryUsageStats,
  TranslationApiConfig,
  TranslationFastProvider,
  TranslationMode,
  TranslationSettings,
} from "../../../types";
import type { ProviderEntry } from "../../models/hooks/useModelProviders";
import { useModelProviders } from "../../models/hooks/useModelProviders";

const AUTO_SAVE_DELAY_MS = 600;

type TestResult = { ok: boolean; latency: number | null; error: string | null };

const DEFAULT_TEST_RESULTS: Record<string, TestResult> = {};

function sameStringArray(a: string[], b: string[]) {
  return [...a].sort().join("|") === [...b].sort().join("|");
}

function sameTranslationSettings(a: TranslationSettings, b: TranslationSettings) {
  return (
    a.target_language === b.target_language &&
    a.mode === b.mode &&
    a.fast_provider === b.fast_provider &&
    a.allow_emergency_fallback === b.allow_emergency_fallback &&
    a.experimental_providers_enabled === b.experimental_providers_enabled &&
    (a.quality_provider_ref?.app_id ?? "") === (b.quality_provider_ref?.app_id ?? "") &&
    (a.quality_provider_ref?.provider_id ?? "") === (b.quality_provider_ref?.provider_id ?? "")
  );
}

function normalizeApiConfigForSettings(
  config: TranslationApiConfig,
  settings: TranslationSettings,
): TranslationApiConfig {
  const enabledProviders: string[] = [];
  if (config.deepl_key.trim()) enabledProviders.push("deepl");
  if (config.google_key.trim()) enabledProviders.push("google");
  if (config.azure_key.trim() && config.azure_region.trim()) enabledProviders.push("azure");
  if (settings.experimental_providers_enabled) {
    enabledProviders.push("deeplx");
    enabledProviders.push("gtx");
  }

  const defaultProvider = settings.fast_provider === "experimental" ? "deeplx" : settings.fast_provider;

  return {
    ...config,
    default_provider: defaultProvider,
    enabled_providers: enabledProviders,
  };
}

function sameManagedApiConfig(a: TranslationApiConfig, b: TranslationApiConfig) {
  return (
    a.deepl_key === b.deepl_key &&
    a.deeplx_url === b.deeplx_url &&
    a.google_key === b.google_key &&
    a.azure_key === b.azure_key &&
    a.azure_region === b.azure_region &&
    a.gtx_api_key === b.gtx_api_key &&
    a.default_provider === b.default_provider &&
    sameStringArray(a.enabled_providers, b.enabled_providers)
  );
}

function modeLabel(t: (key: string, options?: Record<string, unknown>) => string, mode: TranslationMode) {
  switch (mode) {
    case "fast":
      return t("settings.translationModeFast", { defaultValue: "Fast" });
    case "quality":
      return t("settings.translationModeQuality", { defaultValue: "Quality" });
    default:
      return t("settings.translationModeBalanced", { defaultValue: "Balanced" });
  }
}

function fastProviderLabel(provider: TranslationFastProvider) {
  switch (provider) {
    case "google":
      return "Google";
    case "azure":
      return "Azure";
    case "experimental":
      return "Experimental";
    default:
      return "DeepL";
  }
}

function isFastModeWithoutQualityPrompt(mode: TranslationMode) {
  return mode === "fast";
}

function hasCodexApiKey(provider: ProviderEntry) {
  const auth = (provider.settingsConfig?.auth as Record<string, unknown> | undefined) ?? undefined;
  return typeof auth?.OPENAI_API_KEY === "string" && auth.OPENAI_API_KEY.trim().length > 0;
}

export function TranslationSection() {
  const { t } = useTranslation();
  const { config: apiConfig, loading: apiLoading, saveConfig: saveApiConfig, testProvider } = useTranslationApiConfig();
  const { settings, readiness, loading: settingsLoading, saveSettings, refreshReadiness } = useTranslationSettings();
  const claudeProviders = useModelProviders("claude");
  const codexProviders = useModelProviders("codex");

  const [localApiConfig, setLocalApiConfig] = useState<TranslationApiConfig>(apiConfig);
  const [localSettings, setLocalSettings] = useState<TranslationSettings>(settings);
  const [expanded, setExpanded] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [showSecrets, setShowSecrets] = useState(false);
  const [savingApi, setSavingApi] = useState(false);
  const [savingSettingsState, setSavingSettingsState] = useState(false);
  const [savedIndicator, setSavedIndicator] = useState(false);
  const [testingTarget, setTestingTarget] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, TestResult>>(DEFAULT_TEST_RESULTS);
  const [mymemoryUsage, setMymemoryUsage] = useState<MymemoryUsageStats | null>(null);

  const ready = !apiLoading && !settingsLoading;

  useEffect(() => {
    if (!apiLoading) {
      setLocalApiConfig(normalizeApiConfigForSettings(apiConfig, settings));
    }
  }, [apiConfig, settings, apiLoading]);

  useEffect(() => {
    if (!settingsLoading) {
      setLocalSettings(settings);
    }
  }, [settings, settingsLoading]);

  useEffect(() => {
    invoke<MymemoryUsageStats>("get_mymemory_usage_stats")
      .then(setMymemoryUsage)
      .catch(() => setMymemoryUsage(null));
  }, []);

  const qualityOptions = useMemo(() => {
    const claude = claudeProviders.sortedProviders.map((provider) => ({
      value: `claude:${provider.id}`,
      label: `${provider.name} · Claude`,
    }));
    const codex = codexProviders.sortedProviders.filter(hasCodexApiKey).map((provider) => ({
      value: `codex:${provider.id}`,
      label: `${provider.name} · Codex`,
    }));
    return [...claude, ...codex];
  }, [claudeProviders.sortedProviders, codexProviders.sortedProviders]);

  const selectedQualityToken = (() => {
    if (!localSettings.quality_provider_ref) return "";
    const token = `${localSettings.quality_provider_ref.app_id}:${localSettings.quality_provider_ref.provider_id}`;
    return qualityOptions.some((option) => option.value === token) ? token : "";
  })();

  const normalizedSavedApiConfig = useMemo(
    () => normalizeApiConfigForSettings(apiConfig, settings),
    [apiConfig, settings],
  );
  const normalizedLocalApiConfig = useMemo(
    () => normalizeApiConfigForSettings(localApiConfig, localSettings),
    [localApiConfig, localSettings],
  );

  useEffect(() => {
    if (!ready || savingApi || sameManagedApiConfig(normalizedLocalApiConfig, normalizedSavedApiConfig)) {
      return;
    }

    const timer = setTimeout(() => {
      setSavingApi(true);
      saveApiConfig(normalizedLocalApiConfig)
        .then(() => refreshReadiness())
        .then(() => {
          setSavedIndicator(true);
          setTimeout(() => setSavedIndicator(false), 2000);
        })
        .catch((error) => {
          console.error(error);
          toast.error(t("settings.saveTranslationConfigFailed", { defaultValue: "Failed to save translation config" }));
          setLocalApiConfig(normalizedSavedApiConfig);
        })
        .finally(() => setSavingApi(false));
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [normalizedLocalApiConfig, normalizedSavedApiConfig, ready, refreshReadiness, saveApiConfig, savingApi, t]);

  useEffect(() => {
    if (!ready || savingSettingsState || sameTranslationSettings(localSettings, settings)) {
      return;
    }

    const timer = setTimeout(() => {
      setSavingSettingsState(true);
      saveSettings(localSettings)
        .then(() => {
          setSavedIndicator(true);
          setTimeout(() => setSavedIndicator(false), 2000);
        })
        .catch((error) => {
          console.error(error);
          toast.error(t("settings.saveTranslationConfigFailed", { defaultValue: "Failed to save translation config" }));
          setLocalSettings(settings);
        })
        .finally(() => setSavingSettingsState(false));
    }, AUTO_SAVE_DELAY_MS);

    return () => clearTimeout(timer);
  }, [localSettings, ready, saveSettings, savingSettingsState, settings, t]);

  const updateSettings = (next: TranslationSettings) => {
    setLocalSettings(next);
    setLocalApiConfig((current) => normalizeApiConfigForSettings(current, next));
  };

  const updateApiConfig = (updater: (current: TranslationApiConfig) => TranslationApiConfig) => {
    setLocalApiConfig((current) => normalizeApiConfigForSettings(updater(current), localSettings));
  };

  const handleModeChange = (mode: TranslationMode) => {
    updateSettings({ ...localSettings, mode });
  };

  const handleFastProviderChange = (fastProvider: TranslationFastProvider) => {
    updateSettings({ ...localSettings, fast_provider: fastProvider });
  };

  const handleQualityProviderChange = (token: string) => {
    if (!token) {
      updateSettings({ ...localSettings, quality_provider_ref: null });
      return;
    }
    const [app_id, provider_id] = token.split(":");
    updateSettings({
      ...localSettings,
      quality_provider_ref: { app_id, provider_id },
    });
  };

  const handleTest = async (token: string, successLabel: string) => {
    setTestingTarget(token);
    const result = await testProvider(token);
    setTestResults((current) => ({ ...current, [token]: result }));
    if (result.ok) {
      toast.success(result.latency ? `${successLabel} (${result.latency} ms)` : successLabel);
    } else {
      toast.error(
        result.error || t("settings.translationProviderTestFailed", { defaultValue: "Provider test failed" }),
      );
    }
    setTestingTarget(null);
    void refreshReadiness();
  };

  const formattedMyMemory = useMemo(() => {
    if (!mymemoryUsage) return null;
    const formatter = new Intl.NumberFormat();
    return {
      daily: formatter.format(mymemoryUsage.daily_chars_sent),
      total: formatter.format(mymemoryUsage.total_chars_sent),
    };
  }, [mymemoryUsage]);

  const currentFastTestKey = localSettings.fast_provider === "experimental" ? "deeplx" : localSettings.fast_provider;
  const currentQualityTestKey = selectedQualityToken ? `quality:${selectedQualityToken}` : null;
  const qualityOptional = localSettings.mode !== "quality";
  const hideQualityPicker = isFastModeWithoutQualityPrompt(localSettings.mode);
  const visibleReadinessIssues = hideQualityPicker
    ? readiness.issues.filter((issue) => !issue.toLowerCase().includes("quality engine"))
    : readiness.issues;
  const formControlClass =
    "flex h-10 w-full rounded-xl border border-input-border bg-input backdrop-blur-sm px-3 text-sm text-foreground shadow-sm transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:border-primary/60";

  return (
    <section>
      <div className="flex items-center justify-between mb-3 px-1">
        <div className="flex items-center gap-2">
          <div className="w-8 h-8 rounded-xl bg-gradient-to-br from-cyan-500/15 via-sky-500/10 to-emerald-500/15 flex items-center justify-center shrink-0 border border-cyan-500/20">
            <Languages className="w-4 h-4 text-cyan-500" />
          </div>
          <div>
            <h2 className="text-sm font-semibold text-foreground tracking-tight">
              {t("settings.translationCenterTitle", { defaultValue: "Translation Center" })}
            </h2>
          </div>
          {savedIndicator && <span className="text-xs text-emerald-600 ml-1">✓</span>}
        </div>
        {(savingApi || savingSettingsState) && <Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />}
      </div>

      <div className="rounded-2xl border border-border bg-card overflow-hidden">
        <button
          type="button"
          onClick={() => setExpanded((open) => !open)}
          className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/20 transition-colors cursor-pointer"
        >
          <span className="text-sm font-medium text-foreground">
            {t("settings.translationConfigTitle", { defaultValue: "Translation Configuration" })}
          </span>
          <ChevronDown
            className={cn("w-4 h-4 text-muted-foreground transition-transform duration-200", !expanded && "-rotate-90")}
          />
        </button>

        {expanded && (
          <>
            <div className="px-4 py-4 border-t border-border/70 space-y-4">
              <div className="grid gap-3 md:grid-cols-3">
                {(["fast", "balanced", "quality"] as TranslationMode[]).map((mode) => (
                  <button
                    key={mode}
                    type="button"
                    onClick={() => handleModeChange(mode)}
                    className={cn(
                      "rounded-2xl border px-4 py-3 text-left transition-colors cursor-pointer",
                      localSettings.mode === mode
                        ? "border-primary/40 bg-primary/10"
                        : "border-border bg-card hover:bg-muted/30",
                    )}
                  >
                    <div className="text-sm font-medium text-foreground">
                      {mode === "balanced"
                        ? t("settings.translationModeBalancedRecommended", { defaultValue: "Balanced (Recommended)" })
                        : modeLabel(t, mode)}
                    </div>
                    <div className="text-[11px] text-muted-foreground mt-1">
                      {mode === "balanced"
                        ? t("settings.translationModeBalancedHint", {
                            defaultValue:
                              "Short text prefers Fast Engine, Markdown prefers Quality Engine, then auto fallback.",
                          })
                        : mode === "fast"
                          ? t("settings.translationModeFastHint", {
                              defaultValue:
                                "Prefer translation APIs everywhere, only use Quality when a document route needs it.",
                            })
                          : t("settings.translationModeQualityHint", {
                              defaultValue:
                                "Prefer the Models-linked engine first and keep emergency fallback only at the very end.",
                            })}
                    </div>
                  </button>
                ))}
              </div>

              <div className="grid gap-3 lg:grid-cols-2">
                <div className="rounded-2xl border border-border/70 bg-card/80 p-4 space-y-3">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        {t("settings.fastEngine", { defaultValue: "Fast Engine" })}
                      </div>
                      <div className="text-sm text-foreground mt-1">
                        {t("settings.fastEngineCopy", {
                          defaultValue:
                            "Short-text speed path for DeepL, Google, Azure, or the built-in free fallback lane.",
                        })}
                      </div>
                    </div>
                    <button
                      type="button"
                      disabled={testingTarget === currentFastTestKey || !ready}
                      onClick={() =>
                        void handleTest(
                          currentFastTestKey,
                          t("settings.fastEngineProbeSuccess", { defaultValue: "Fast engine probe succeeded" }),
                        )
                      }
                      className="inline-flex items-center gap-1.5 rounded-lg border border-border px-2.5 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-muted/40 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {testingTarget === currentFastTestKey ? (
                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      ) : (
                        <TestTube2 className="w-3.5 h-3.5" />
                      )}
                      {t("settings.probe", { defaultValue: "Probe" })}
                    </button>
                  </div>
                  <select
                    value={localSettings.fast_provider}
                    onChange={(event) => handleFastProviderChange(event.target.value as TranslationFastProvider)}
                    className={`${formControlClass} pr-8`}
                  >
                    <option value="deepl">DeepL</option>
                    <option value="google">Google Cloud Translation</option>
                    <option value="azure">Azure Translator</option>
                    <option value="experimental">
                      {t("settings.fastEngineExperimental", {
                        defaultValue: "Free / Experimental (DeepLX + GTX)",
                      })}
                    </option>
                  </select>
                  <div className="text-xs text-muted-foreground">
                    {t("settings.fastEngineCurrent", {
                      defaultValue: "Current lane: {{provider}}",
                      provider: fastProviderLabel(localSettings.fast_provider),
                    })}
                    {testResults[currentFastTestKey]?.ok && (
                      <span className="ml-2 text-emerald-600">{testResults[currentFastTestKey]?.latency} ms</span>
                    )}
                  </div>
                </div>

                <div className="rounded-2xl border border-border/70 bg-card/80 p-4 space-y-3">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                        {qualityOptional
                          ? t("settings.qualityEngineOptional", { defaultValue: "Quality Lane (Optional)" })
                          : t("settings.qualityEngine", { defaultValue: "Quality Lane" })}
                      </div>
                      <div className="text-sm text-foreground mt-1">
                        {hideQualityPicker
                          ? t("settings.qualityEngineFastModeCopy", {
                              defaultValue:
                                "Fast mode does not require a quality provider. Connect one only if you want high-quality retranslate or a dedicated quality lane later.",
                            })
                          : t("settings.qualityEngineCopy", {
                              defaultValue:
                                "Reuse a provider from Models for Markdown fidelity, terminology, and high-quality retranslation.",
                            })}
                      </div>
                    </div>
                    <button
                      type="button"
                      disabled={hideQualityPicker || !currentQualityTestKey || testingTarget === currentQualityTestKey}
                      onClick={() =>
                        currentQualityTestKey
                          ? void handleTest(
                              currentQualityTestKey,
                              t("settings.qualityEngineProbeSuccess", {
                                defaultValue: "Quality engine probe succeeded",
                              }),
                            )
                          : undefined
                      }
                      className="inline-flex items-center gap-1.5 rounded-lg border border-border px-2.5 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-muted/40 disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {currentQualityTestKey && testingTarget === currentQualityTestKey ? (
                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      ) : (
                        <TestTube2 className="w-3.5 h-3.5" />
                      )}
                      {t("settings.probe", { defaultValue: "Probe" })}
                    </button>
                  </div>
                  {hideQualityPicker ? (
                    <div className="rounded-xl border border-border/60 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
                      {t("settings.qualityEngineNotRequired", {
                        defaultValue:
                          "No quality provider is required for translation in Fast mode. Free and traditional translation APIs can run on their own.",
                      })}
                    </div>
                  ) : (
                    <select
                      value={selectedQualityToken}
                      onChange={(event) => handleQualityProviderChange(event.target.value)}
                      className={`${formControlClass} pr-8`}
                    >
                      <option value="">
                        {qualityOptions.length
                          ? t("settings.selectQualityEngineOptional", {
                              defaultValue: "Optional: select a Models provider",
                            })
                          : t("settings.noQualityProviders", {
                              defaultValue: "No Models providers connected yet",
                            })}
                      </option>
                      {qualityOptions.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                  )}
                  <div className="text-xs text-muted-foreground">
                    {hideQualityPicker
                      ? t("settings.qualityEngineFastModeHint", {
                          defaultValue:
                            "If you leave this empty, normal translation still works. Only high-quality retranslate depends on a quality provider.",
                        })
                      : qualityOptions.length
                        ? t("settings.qualityEngineHint", {
                            defaultValue:
                              "Managed in Models. Translation only stores a provider reference, not another API key.",
                          })
                        : t("settings.qualityEngineMissingHint", {
                            defaultValue:
                              "Go to Models to connect Anthropic-, OpenAI-, or MiniMax-compatible providers first.",
                          })}
                    {currentQualityTestKey && testResults[currentQualityTestKey]?.ok && (
                      <span className="ml-2 text-emerald-600">{testResults[currentQualityTestKey]?.latency} ms</span>
                    )}
                  </div>
                </div>
              </div>
            </div>

            <button
              type="button"
              onClick={() => setAdvancedOpen((open) => !open)}
              className="w-full px-4 py-3 flex items-center justify-between hover:bg-muted/20 transition-colors cursor-pointer"
            >
              <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                <ShieldAlert className="w-4 h-4 text-muted-foreground" />
                {t("settings.translationAdvanced", { defaultValue: "Advanced Controls" })}
              </div>
              <ChevronDown
                className={cn(
                  "w-4 h-4 text-muted-foreground transition-transform duration-200",
                  advancedOpen && "rotate-180",
                )}
              />
            </button>

            {advancedOpen && (
              <div className="px-4 pb-4 pt-1 border-t border-border space-y-4">
                <div className="grid gap-3 lg:grid-cols-3">
                  <div className="rounded-xl border border-border/60 bg-card/70 p-3 space-y-2">
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <div className="text-sm font-medium text-foreground">
                          {t("settings.targetLanguage", { defaultValue: "Target Language" })}
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                          {t("settings.translationTargetLanguageHint", {
                            defaultValue: "Shared target for short text and SKILL.md translation.",
                          })}
                        </div>
                      </div>
                      <Gauge className="w-4 h-4 text-muted-foreground" />
                    </div>
                    <Input
                      value={localSettings.target_language}
                      onChange={(event) => updateSettings({ ...localSettings, target_language: event.target.value })}
                      placeholder="zh-CN"
                      className="font-mono text-xs h-9"
                    />
                  </div>

                  <div className="rounded-xl border border-border/60 bg-card/70 p-3 space-y-2">
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <div className="text-sm font-medium text-foreground">
                          {t("settings.allowEmergencyFallback", { defaultValue: "Allow Emergency Fallback" })}
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                          {t("settings.allowEmergencyFallbackHint", {
                            defaultValue: "Keep MyMemory only as the last-resort route.",
                          })}
                        </div>
                      </div>
                      <Switch
                        checked={localSettings.allow_emergency_fallback}
                        onCheckedChange={(checked) =>
                          updateSettings({ ...localSettings, allow_emergency_fallback: checked })
                        }
                      />
                    </div>
                  </div>

                  <div className="rounded-xl border border-border/60 bg-card/70 p-3 space-y-2">
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <div className="text-sm font-medium text-foreground">
                          {t("settings.experimentalProviders", { defaultValue: "Enable Experimental Sources" })}
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                          {t("settings.experimentalProvidersHint", {
                            defaultValue:
                              "Expose DeepLX and GTX in the fast fallback lane. Leave DeepLX URL blank to use the bundled free endpoint.",
                          })}
                        </div>
                      </div>
                      <Switch
                        checked={localSettings.experimental_providers_enabled}
                        onCheckedChange={(checked) =>
                          updateSettings({ ...localSettings, experimental_providers_enabled: checked })
                        }
                      />
                    </div>
                  </div>
                </div>

                <div className="rounded-2xl border border-border/60 bg-card/70 p-4 space-y-4">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <div className="text-sm font-medium text-foreground">
                        {t("settings.fastProviderCredentials", { defaultValue: "Fast Engine Credentials" })}
                      </div>
                      <div className="text-[11px] text-muted-foreground">
                        {t("settings.fastProviderCredentialsHint", {
                          defaultValue:
                            "Manage only classic translation API credentials here. Quality accounts stay in Models.",
                        })}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() => setShowSecrets((value) => !value)}
                      className="inline-flex items-center gap-1.5 rounded-lg border border-border px-2.5 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-muted/40"
                    >
                      {showSecrets ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                      {showSecrets
                        ? t("settings.hideSecrets", { defaultValue: "Hide" })
                        : t("settings.showSecrets", { defaultValue: "Show" })}
                    </button>
                  </div>

                  <div className="grid gap-3 lg:grid-cols-2">
                    <div className="rounded-xl border border-border/50 p-3 space-y-2">
                      <div className="text-xs font-medium text-foreground">DeepL</div>
                      <Input
                        type={showSecrets ? "text" : "password"}
                        value={localApiConfig.deepl_key}
                        onChange={(event) =>
                          updateApiConfig((current) => ({ ...current, deepl_key: event.target.value }))
                        }
                        placeholder="DeepL-Auth-Key ..."
                        className="font-mono text-xs h-9"
                      />
                    </div>

                    <div className="rounded-xl border border-border/50 p-3 space-y-2">
                      <div className="text-xs font-medium text-foreground">Google Cloud Translation</div>
                      <Input
                        type={showSecrets ? "text" : "password"}
                        value={localApiConfig.google_key}
                        onChange={(event) =>
                          updateApiConfig((current) => ({ ...current, google_key: event.target.value }))
                        }
                        placeholder="AIza..."
                        className="font-mono text-xs h-9"
                      />
                    </div>

                    <div className="rounded-xl border border-border/50 p-3 space-y-2">
                      <div className="text-xs font-medium text-foreground">Azure Translator</div>
                      <Input
                        type={showSecrets ? "text" : "password"}
                        value={localApiConfig.azure_key}
                        onChange={(event) =>
                          updateApiConfig((current) => ({ ...current, azure_key: event.target.value }))
                        }
                        placeholder="Azure key..."
                        className="font-mono text-xs h-9"
                      />
                      <Input
                        value={localApiConfig.azure_region}
                        onChange={(event) =>
                          updateApiConfig((current) => ({ ...current, azure_region: event.target.value }))
                        }
                        placeholder="eastasia"
                        className="font-mono text-xs h-9"
                      />
                    </div>

                    <div className="rounded-xl border border-border/50 p-3 space-y-2">
                      <div className="text-xs font-medium text-foreground">
                        {t("settings.experimentalSources", { defaultValue: "Free / Experimental Sources" })}
                      </div>
                      <Input
                        value={localApiConfig.deeplx_url}
                        onChange={(event) =>
                          updateApiConfig((current) => ({ ...current, deeplx_url: event.target.value }))
                        }
                        placeholder={t("settings.deeplxPlaceholder", {
                          defaultValue: "Optional custom DeepLX endpoint (blank = bundled free endpoint)",
                        })}
                        className="font-mono text-xs h-9"
                      />
                      <Input
                        type={showSecrets ? "text" : "password"}
                        value={localApiConfig.gtx_api_key}
                        onChange={(event) =>
                          updateApiConfig((current) => ({ ...current, gtx_api_key: event.target.value }))
                        }
                        placeholder={t("settings.gtxPlaceholder", { defaultValue: "Optional GTX key" })}
                        className="font-mono text-xs h-9"
                      />
                    </div>
                  </div>
                </div>

                <div className="grid gap-3 lg:grid-cols-2">
                  <div className="rounded-xl border border-border/60 bg-card/70 p-4">
                    <div className="text-sm font-medium text-foreground">
                      {t("settings.routeDiagnostics", { defaultValue: "Route Diagnostics" })}
                    </div>
                    <div className="text-[11px] text-muted-foreground mt-1">
                      {t("settings.routeDiagnosticsHint", {
                        defaultValue:
                          "Recommended mode, fallback posture, and readiness issues from the backend router.",
                      })}
                    </div>
                    <div className="mt-3 text-xs text-muted-foreground">
                      {t("settings.recommendedModeLabel", {
                        defaultValue: "Recommended mode: {{mode}}",
                        mode: modeLabel(t, readiness.recommended_mode),
                      })}
                    </div>
                    <div className="mt-2 space-y-2">
                      {visibleReadinessIssues.length ? (
                        visibleReadinessIssues.map((issue) => (
                          <div
                            key={issue}
                            className="rounded-lg border border-border/50 bg-muted/40 px-3 py-2 text-xs text-muted-foreground"
                          >
                            {issue}
                          </div>
                        ))
                      ) : (
                        <div className="rounded-lg border border-emerald-500/20 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-600">
                          {t("settings.translationNoIssues", { defaultValue: "No routing issues detected." })}
                        </div>
                      )}
                    </div>
                  </div>

                  <div className="rounded-xl border border-border/60 bg-card/70 p-4">
                    <div className="text-sm font-medium text-foreground">
                      {t("settings.emergencyFallbackStats", { defaultValue: "Emergency Fallback Stats" })}
                    </div>
                    <div className="text-[11px] text-muted-foreground mt-1">
                      {t("settings.emergencyFallbackStatsHint", {
                        defaultValue:
                          "MyMemory is intentionally hidden from the main flow and shown here only for diagnostics.",
                      })}
                    </div>
                    <div className="mt-3 rounded-lg border border-border/50 bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
                      {formattedMyMemory
                        ? t("settings.myMemoryUsageDailyWithTotal", {
                            daily: formattedMyMemory.daily,
                            total: formattedMyMemory.total,
                          })
                        : t("settings.myMemoryCharsSentUnknown", { defaultValue: "MyMemory usage unavailable." })}
                    </div>
                    <div className="mt-3 text-xs text-muted-foreground">
                      <span className="font-medium text-foreground">
                        {t("settings.retranslateHighQuality", { defaultValue: "High-Quality Retranslate" })}
                      </span>{" "}
                      {t("settings.retranslateHighQualityHint", {
                        defaultValue: "Always forces the Quality Engine path and skips MyMemory fallback.",
                      })}
                    </div>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </section>
  );
}

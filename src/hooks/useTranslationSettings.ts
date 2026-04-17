import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { TranslationReadiness, TranslationSettings } from "../types";

const DEFAULT_SETTINGS: TranslationSettings = {
  target_language: "zh-CN",
  mode: "balanced",
  fast_provider: "deepl",
  quality_provider_ref: null,
  allow_emergency_fallback: true,
  experimental_providers_enabled: false,
};

const DEFAULT_READINESS: TranslationReadiness = {
  fast_ready: false,
  quality_ready: false,
  emergency_ready: true,
  issues: [],
  recommended_mode: "balanced",
};

const CONFIG_CACHE_TTL_MS = 3_000;

let cachedSettings: TranslationSettings | null = null;
let cachedReadiness: TranslationReadiness | null = null;
let cachedAt = 0;
let inflightSettings: Promise<TranslationSettings> | null = null;
let inflightReadiness: Promise<TranslationReadiness> | null = null;

export function invalidateTranslationSettingsCache() {
  cachedSettings = null;
  cachedReadiness = null;
  cachedAt = 0;
}

export async function getTranslationSettingsCached(): Promise<TranslationSettings> {
  const now = Date.now();
  if (cachedSettings && now - cachedAt < CONFIG_CACHE_TTL_MS) {
    return cachedSettings;
  }
  if (inflightSettings) return inflightSettings;

  inflightSettings = invoke<TranslationSettings>("get_translation_settings")
    .then((settings) => {
      cachedSettings = { ...DEFAULT_SETTINGS, ...settings };
      cachedAt = Date.now();
      return cachedSettings;
    })
    .catch(() => DEFAULT_SETTINGS)
    .finally(() => {
      inflightSettings = null;
    });

  return inflightSettings;
}

export async function getTranslationReadinessCached(forceRefresh = false): Promise<TranslationReadiness> {
  const now = Date.now();
  if (!forceRefresh && cachedReadiness && now - cachedAt < CONFIG_CACHE_TTL_MS) {
    return cachedReadiness;
  }
  if (!forceRefresh && inflightReadiness) return inflightReadiness;

  inflightReadiness = invoke<TranslationReadiness>("get_translation_readiness")
    .then((readiness) => {
      cachedReadiness = { ...DEFAULT_READINESS, ...readiness };
      cachedAt = Date.now();
      return cachedReadiness;
    })
    .catch(() => DEFAULT_READINESS)
    .finally(() => {
      inflightReadiness = null;
    });

  return inflightReadiness;
}

export function isMarkdownTranslationReady(settings: TranslationSettings, readiness: TranslationReadiness): boolean {
  return settings.mode === "quality" ? readiness.quality_ready : readiness.fast_ready || readiness.quality_ready;
}

export function useTranslationSettings() {
  const [settings, setSettings] = useState<TranslationSettings>(DEFAULT_SETTINGS);
  const [readiness, setReadiness] = useState<TranslationReadiness>(DEFAULT_READINESS);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([getTranslationSettingsCached(), getTranslationReadinessCached()])
      .then(([nextSettings, nextReadiness]) => {
        setSettings(nextSettings);
        setReadiness(nextReadiness);
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  const refreshReadiness = useCallback(async () => {
    const next = await getTranslationReadinessCached(true);
    setReadiness(next);
    return next;
  }, []);

  const saveSettings = useCallback(async (nextSettings: TranslationSettings) => {
    await invoke("save_translation_settings", { settings: nextSettings });
    invalidateTranslationSettingsCache();
    setSettings(nextSettings);
    const nextReadiness = await getTranslationReadinessCached(true);
    setReadiness(nextReadiness);
    return nextReadiness;
  }, []);

  return {
    settings,
    readiness,
    loading,
    saveSettings,
    refreshReadiness,
  };
}

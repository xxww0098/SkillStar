import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { modelsKeys } from "./keys";
import { tauriInvoke } from "../../../lib/ipc";
import type { ProviderPresetFlat } from "../../../types";

const PRESETS_STALE_TIME_MS = 60_000 * 60;
const QUERY_KEY = modelsKeys.presets();

/**
 * OpenAI-compatible virtual preset (not in the Rust registry). `name` is
 * resolved via `t()` at construction time inside `useProviderPresets` (below)
 * rather than hardcoded — it's UI display text (shown as a preset card title
 * and copied verbatim into the created provider's `name` field), unlike the
 * backend presets' brand names (proper nouns), which stay untranslated on
 * purpose. Reuses `models.preset.categoryCompatible`, which is already the
 * same label.
 */
function buildOpenaiCompatiblePreset(name: string): ProviderPresetFlat {
  return {
    id: "openai-compatible",
    name,
    category: "openai_compatible",
    base_url_openai: "https://api.openai.com/v1",
    base_url_anthropic: "",
    models_url: "https://api.openai.com/v1/models",
    models: [],
    icon_color: "#10A37F",
  };
}

/**
 * Built-in flat provider presets from the backend (single source of truth).
 */
export function useProviderPresets() {
  const { t } = useTranslation();
  const { data, isLoading, error } = useQuery<ProviderPresetFlat[]>({
    queryKey: QUERY_KEY,
    queryFn: () => tauriInvoke("get_provider_presets_flat"),
    staleTime: PRESETS_STALE_TIME_MS,
  });

  const presets = data ?? [];

  const grouped = {
    // v3's single `official` bucket held both, and the UI had no way to tell
    // "log in with your browser" from "paste an API key" apart from an id list.
    official: presets.filter((p) => p.category === "native_login" || p.category === "vendor_official"),
    domestic: presets.filter((p) => p.category === "domestic"),
    relay: presets.filter((p) => p.category === "relay"),
    openai_compatible: [buildOpenaiCompatiblePreset(t("models.preset.categoryCompatible"))],
  };

  return { presets, grouped, isLoading, error };
}

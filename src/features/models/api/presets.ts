import { useQuery } from "@tanstack/react-query";
import { modelsKeys } from "./keys";
import { tauriInvoke } from "../../../lib/ipc";
import type { ProviderPresetFlat } from "../../../types";

const PRESETS_STALE_TIME_MS = 60_000 * 60;
const QUERY_KEY = modelsKeys.presets();

/** OpenAI-compatible virtual preset (not in the Rust registry). */
export const OPENAI_COMPATIBLE_PRESET: ProviderPresetFlat = {
  id: "openai-compatible",
  name: "OpenAI 兼容",
  category: "openai_compatible",
  base_url_openai: "https://api.openai.com/v1",
  base_url_anthropic: "",
  models_url: "https://api.openai.com/v1/models",
  models: [],
  icon_color: "#10A37F",
};

/**
 * Built-in flat provider presets from the backend (single source of truth).
 */
export function useProviderPresets() {
  const { data, isLoading, error } = useQuery<ProviderPresetFlat[]>({
    queryKey: QUERY_KEY,
    queryFn: () => tauriInvoke("get_provider_presets_flat"),
    staleTime: PRESETS_STALE_TIME_MS,
  });

  const presets = data ?? [];

  const grouped = {
    official: presets.filter((p) => p.category === "official"),
    domestic: presets.filter((p) => p.category === "domestic"),
    relay: presets.filter((p) => p.category === "relay"),
    openai_compatible: [OPENAI_COMPATIBLE_PRESET],
  };

  return { presets, grouped, isLoading, error };
}

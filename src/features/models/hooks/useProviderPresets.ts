import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import type { ModelAppId } from "../components/AppCapsuleSwitcher";
import type { ProviderEntry } from "./useModelProviders";

interface ProviderPresetsResponse {
  presets: ProviderEntry[];
}

export function useProviderPresets(appId: ModelAppId) {
  const [loading, setLoading] = useState(true);
  const [presets, setPresets] = useState<ProviderEntry[]>([]);

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      setLoading(true);
      try {
        const result = await invoke<ProviderPresetsResponse>("get_model_provider_presets", { appId });
        if (!cancelled) {
          setPresets(result.presets || []);
        }
      } catch {
        if (!cancelled) {
          setPresets([]);
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };

    load();

    return () => {
      cancelled = true;
    };
  }, [appId]);

  const grouped = useMemo(() => {
    return presets.map((preset) => ({
      ...preset,
      settingsConfig: preset.settingsConfig || {},
    }));
  }, [presets]);

  return { loading, presets: grouped };
}

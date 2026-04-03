import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  readBackgroundRun,
  writeBackgroundRun,
} from "../features/settings/sections/BackgroundRunSection";
import { getLanguage } from "../i18n";

type PatrolStatus = {
  enabled: boolean;
  interval_secs: number;
};

/**
 * Consolidates all Tauri lifecycle setup that was previously spread across
 * 4 separate useEffect hooks in AppContent:
 *
 * 1. Sync patrol enabled state on mount
 * 2. Listen for window-hidden → auto-start patrol
 * 3. Listen for patrol enabled-changed → sync to localStorage
 * 4. Sync language to tray on mount
 */
export function useTauriSetup() {
  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];

    (async () => {
      try {
        // 1. Sync patrol enabled state on mount
        await invoke("set_patrol_enabled", { enabled: readBackgroundRun() });
      } catch {
        // Not in Tauri environment — all subsequent calls will also fail,
        // but we continue to set up listeners in case we're in a dev wrapper.
      }

      if (cancelled) return;

      try {
        // 2. Listen for window-hidden → auto-start patrol
        const unlistenHidden = await listen("skillstar://window-hidden", async () => {
          if (!readBackgroundRun()) return;

          const status = await invoke<PatrolStatus>("get_patrol_status").catch(() => null);
          if (status && !status.enabled) {
            await invoke("set_patrol_enabled", { enabled: true }).catch(() => {});
          }
          await invoke("start_patrol", {
            intervalSecs: status?.interval_secs ?? 30,
          }).catch(() => {});
        });
        if (cancelled) { unlistenHidden(); } else { cleanups.push(unlistenHidden); }

        // 3. Listen for patrol enabled-changed → sync to localStorage
        const unlistenEnabled = await listen<boolean>("patrol://enabled-changed", (event) => {
          writeBackgroundRun(Boolean(event.payload));
        });
        if (cancelled) { unlistenEnabled(); } else { cleanups.push(unlistenEnabled); }
      } catch {
        // Not in Tauri environment
      }

      if (cancelled) return;

      try {
        // 4. Sync language to tray on mount
        await invoke("update_tray_language", { lang: getLanguage() });
      } catch {
        // Not in Tauri environment
      }
    })();

    return () => {
      cancelled = true;
      cleanups.forEach((fn) => fn());
    };
  }, []);
}

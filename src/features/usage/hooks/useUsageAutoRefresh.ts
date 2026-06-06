import { useCallback, useEffect, useRef, useState } from "react";

const STORAGE_KEY = "skillstar:usage-auto-refresh";

export const USAGE_REFRESH_INTERVALS = [
  { ms: 60_000, key: "interval1m" },
  { ms: 300_000, key: "interval5m" },
  { ms: 900_000, key: "interval15m" },
  { ms: 1_800_000, key: "interval30m" },
  { ms: 3_600_000, key: "interval1h" },
] as const;

export type UsageRefreshIntervalKey = (typeof USAGE_REFRESH_INTERVALS)[number]["key"];

const DEFAULT_INTERVAL_MS = USAGE_REFRESH_INTERVALS[1].ms;

export interface UsageAutoRefreshSettings {
  enabled: boolean;
  intervalMs: number;
}

function readSettings(): UsageAutoRefreshSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { enabled: false, intervalMs: DEFAULT_INTERVAL_MS };
    const parsed = JSON.parse(raw) as Partial<UsageAutoRefreshSettings>;
    const intervalMs = USAGE_REFRESH_INTERVALS.some((item) => item.ms === parsed.intervalMs)
      ? parsed.intervalMs!
      : DEFAULT_INTERVAL_MS;
    return { enabled: parsed.enabled === true, intervalMs };
  } catch {
    return { enabled: false, intervalMs: DEFAULT_INTERVAL_MS };
  }
}

function writeSettings(settings: UsageAutoRefreshSettings) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // ignore
  }
}

export function useUsageAutoRefreshSettings() {
  const [settings, setSettings] = useState<UsageAutoRefreshSettings>(readSettings);

  const setAutoRefreshEnabled = useCallback((enabled: boolean) => {
    setSettings((prev) => {
      const next = { ...prev, enabled };
      writeSettings(next);
      return next;
    });
  }, []);

  const setIntervalMs = useCallback((intervalMs: number) => {
    setSettings((prev) => {
      const next = { ...prev, intervalMs };
      writeSettings(next);
      return next;
    });
  }, []);

  return {
    autoRefreshEnabled: settings.enabled,
    intervalMs: settings.intervalMs,
    setAutoRefreshEnabled,
    setIntervalMs,
  };
}

/** Runs the refresh timer while Usage mode is active. */
export function useUsageAutoRefreshRunner(
  settings: UsageAutoRefreshSettings,
  onRefresh: () => Promise<void>,
  refreshing: boolean,
) {
  const onRefreshRef = useRef(onRefresh);
  const refreshingRef = useRef(refreshing);

  useEffect(() => {
    onRefreshRef.current = onRefresh;
  }, [onRefresh]);

  useEffect(() => {
    refreshingRef.current = refreshing;
  }, [refreshing]);

  useEffect(() => {
    if (!settings.enabled) return;

    let cancelled = false;

    const tick = () => {
      // Skip while a refresh is in flight or the window is in the background —
      // no point polling provider quotas the user can't see (saves network/battery).
      if (cancelled || refreshingRef.current || document.hidden) return;
      void onRefreshRef.current();
    };

    tick();
    const timer = window.setInterval(tick, settings.intervalMs);

    // Catch up immediately when the user returns to the window, so the data
    // they see on focus is fresh rather than up to one interval stale.
    const onVisibility = () => {
      if (!document.hidden) tick();
    };
    document.addEventListener("visibilitychange", onVisibility);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [settings.enabled, settings.intervalMs]);
}

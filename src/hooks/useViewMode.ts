import { useEffect, useState } from "react";
import type { ViewMode } from "../types";

const VIEW_MODE_KEY = "skillstar:view-mode";

export function useViewMode(defaultMode: ViewMode = "grid") {
  const [viewMode, setViewModeState] = useState<ViewMode>(() => {
    try {
      const stored = localStorage.getItem(VIEW_MODE_KEY);
      if (stored === "grid" || stored === "list") {
        return stored;
      }
    } catch {
      // Ignore
    }
    return defaultMode;
  });

  const setViewMode = (mode: ViewMode) => {
    try {
      localStorage.setItem(VIEW_MODE_KEY, mode);
    } catch {
      // Ignore
    }
    setViewModeState(mode);
    // Dispatch custom event to sync across components
    window.dispatchEvent(new CustomEvent("skillstar:view-mode-changed", { detail: mode }));
  };

  useEffect(() => {
    const handleSync = (e: Event) => {
      const customEvent = e as CustomEvent<ViewMode>;
      if (customEvent.detail) {
        setViewModeState(customEvent.detail);
      }
    };
    window.addEventListener("skillstar:view-mode-changed", handleSync);
    return () => window.removeEventListener("skillstar:view-mode-changed", handleSync);
  }, []);

  return [viewMode, setViewMode] as const;
}

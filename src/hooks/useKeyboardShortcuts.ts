import { useCallback, useEffect } from "react";
import type { NavPage } from "../types";

interface KeyboardShortcutsOptions {
  onNavigate: (page: NavPage) => void;
  onToggleCommandPalette: () => void;
  onImport?: () => void;
  onRefresh?: () => void;
}

/**
 * Global keyboard shortcuts for the desktop app.
 *
 * ⌘K / Ctrl+K → Command palette
 * ⌘1–6       → Navigate pages
 * ⌘,         → Settings
 * ⌘I         → Import (on MySkills)
 */
export function useKeyboardShortcuts({
  onNavigate,
  onToggleCommandPalette,
  onImport,
  onRefresh,
}: KeyboardShortcutsOptions) {
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      const target = e.target as HTMLElement;
      const isInput = target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;

      // ── ⌘K / Ctrl+K → Command Palette ──
      if (meta && e.key === "k") {
        e.preventDefault();
        onToggleCommandPalette();
        return;
      }

      // Don't process other shortcuts when focused on input
      if (isInput) return;

      // ── ⌘, → Settings ──
      if (meta && e.key === ",") {
        e.preventDefault();
        onNavigate("settings");
        return;
      }

      // ── ⌘1–6 → Page navigation ──
      if (meta && !e.shiftKey && !e.altKey) {
        const pageMap: Record<string, NavPage> = {
          "1": "my-skills",
          "2": "marketplace",
          "3": "skill-cards",
          "4": "projects",
          "5": "security-scan",
          "6": "settings",
        };
        const page = pageMap[e.key];
        if (page) {
          e.preventDefault();
          onNavigate(page);
          return;
        }
      }

      // ── ⌘I → Import (contextual) ──
      if (meta && e.key === "i" && onImport) {
        e.preventDefault();
        onImport();
        return;
      }

      // ── ⌘R → Refresh (with cooldown handled by caller) ──
      if (meta && e.key === "r" && onRefresh) {
        e.preventDefault();
        onRefresh();
        return;
      }
    },
    [onNavigate, onToggleCommandPalette, onImport, onRefresh],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);
}

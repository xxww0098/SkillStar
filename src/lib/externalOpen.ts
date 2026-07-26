import { isTauri } from "@tauri-apps/api/core";
import { tauriInvoke } from "./ipc";

const DUPLICATE_SUPPRESS_MS = 900;

let lastOpenedUrl: string | null = null;
let lastOpenedAt = 0;

function isHttpUrl(value: string): boolean {
  return /^https?:\/\//i.test(value);
}

/**
 * Open an external URL through backend native handlers.
 * Returns `true` when a launch request was successfully issued.
 */
export async function openExternalUrl(rawUrl: string): Promise<boolean> {
  const url = rawUrl.trim();
  if (!isHttpUrl(url)) {
    if (import.meta.env.DEV) console.warn("[externalOpen] blocked non-http(s) URL:", rawUrl);
    return false;
  }

  const now = Date.now();
  // Only suppress after a *successful* open — failed attempts must remain retriable.
  if (lastOpenedUrl === url && now - lastOpenedAt < DUPLICATE_SUPPRESS_MS) {
    return true;
  }

  // Plain browser (Vite-only) — no Tauri backend.
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    lastOpenedUrl = url;
    lastOpenedAt = Date.now();
    return true;
  }

  try {
    await tauriInvoke("open_external_url", { url });
    lastOpenedUrl = url;
    lastOpenedAt = Date.now();
    return true;
  } catch (error) {
    if (import.meta.env.DEV) console.error("[externalOpen] tauriInvoke(open_external_url) failed:", error);
    return false;
  }
}

/**
 * Handle <a> click / auxclick and route http(s) URLs through native external open.
 * Returns `true` when the event was intercepted.
 */
export function handleExternalAnchorClick(
  event: {
    defaultPrevented: boolean;
    button?: number;
    preventDefault: () => void;
    stopPropagation?: () => void;
  },
  rawUrl: string,
): boolean {
  if (event.defaultPrevented) return false;

  // Ignore non-primary mouse buttons except middle-click (button === 1),
  // which should still open the system browser rather than a dead webview tab.
  if (typeof event.button === "number" && event.button !== 0 && event.button !== 1) {
    return false;
  }

  const url = rawUrl.trim();
  if (!isHttpUrl(url)) return false;

  event.preventDefault();
  event.stopPropagation?.();
  void openExternalUrl(url);
  return true;
}

/** @internal test helpers */
export const __test__ = {
  isHttpUrl,
  resetDuplicateGuard() {
    lastOpenedUrl = null;
    lastOpenedAt = 0;
  },
};

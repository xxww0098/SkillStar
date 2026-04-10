import { invoke } from "@tauri-apps/api/core";

const DUPLICATE_SUPPRESS_MS = 900;

let lastOpenedUrl: string | null = null;
let lastOpenedAt = 0;

function isHttpUrl(value: string): boolean {
  return /^https?:\/\//i.test(value);
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Open an external URL through backend native handlers.
 * Returns `true` when a launch request was successfully issued.
 */
export async function openExternalUrl(rawUrl: string): Promise<boolean> {
  const url = rawUrl.trim();
  if (!isHttpUrl(url)) {
    console.warn("[externalOpen] blocked non-http(s) URL:", rawUrl);
    return false;
  }

  const now = Date.now();
  if (lastOpenedUrl === url && now - lastOpenedAt < DUPLICATE_SUPPRESS_MS) {
    return true;
  }
  lastOpenedUrl = url;
  lastOpenedAt = now;

  try {
    await invoke("open_external_url", { url });
    return true;
  } catch (error) {
    // In browser-only dev mode, fallback to window.open.
    if (!isTauriRuntime()) {
      window.open(url, "_blank", "noopener,noreferrer");
      return true;
    }
    console.error("[externalOpen] invoke(open_external_url) failed:", error);
    return false;
  }
}

/**
 * Handle <a> click and route http(s) URLs through native external open.
 * Returns `true` when the event was intercepted.
 */
export function handleExternalAnchorClick(
  event: {
    defaultPrevented: boolean;
    preventDefault: () => void;
  },
  rawUrl: string,
): boolean {
  if (event.defaultPrevented) return false;

  const url = rawUrl.trim();
  if (!isHttpUrl(url)) return false;

  event.preventDefault();
  void openExternalUrl(url);
  return true;
}

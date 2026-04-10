import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

// ── Platform detection (cached) ─────────────────────────────────────
export type Platform = "macos" | "windows" | "linux" | "unknown";

let _cachedPlatform: Platform | null = null;

/** Detect the current OS platform from navigator. Result is cached. */
export function detectPlatform(): Platform {
  if (_cachedPlatform) return _cachedPlatform;
  if (typeof navigator === "undefined") return "unknown";
  const source = `${navigator.userAgent} ${navigator.platform}`.toLowerCase();
  if (source.includes("mac")) _cachedPlatform = "macos";
  else if (source.includes("win")) _cachedPlatform = "windows";
  else if (source.includes("linux")) _cachedPlatform = "linux";
  else _cachedPlatform = "unknown";
  return _cachedPlatform;
}

/** Return `true` when running on Windows. */
export function isWindows(): boolean {
  return detectPlatform() === "windows";
}

/**
 * Format a relative path for display using the current platform's separator.
 * On Windows, forward slashes are replaced with backslashes.
 */
export function formatPlatformPath(path: string): string {
  if (isWindows()) return path.replace(/\//g, "\\");
  return path;
}

function normalizeSlashes(path: string): string {
  return path.replace(/\\/g, "/");
}

/**
 * Infer home root from an absolute path.
 * - Windows: `C:/Users/<name>`
 * - macOS/Linux: `/Users/<name>` or `/home/<name>`
 */
export function inferUserHomeRoot(path: string): string | null {
  const normalized = normalizeSlashes(path);

  const windows = normalized.match(/^([A-Za-z]:\/Users\/[^/]+)(?:\/|$)/);
  if (windows?.[1]) return windows[1];

  const unix = normalized.match(/^(\/(?:Users|home)\/[^/]+)(?:\/|$)/);
  if (unix?.[1]) return unix[1];

  return null;
}

/**
 * Format an absolute path for current platform display.
 * - Windows: keeps absolute path and uses backslashes.
 * - macOS/Linux: collapses `/Users/<u>/...` and `/home/<u>/...` to `~/...`.
 */
export function formatGlobalPathForDisplay(path: string, platform: Platform = detectPlatform()): string {
  const normalized = normalizeSlashes(path);
  if (platform === "windows") return normalized.replace(/\//g, "\\");

  const homeRoot = inferUserHomeRoot(normalized);
  if (!homeRoot) return normalized;
  if (normalized === homeRoot) return "~";

  const prefix = `${homeRoot}/`;
  if (normalized.startsWith(prefix)) return `~/${normalized.slice(prefix.length)}`;

  return normalized;
}

/**
 * Resolve SkillStar data root display path from resolved home dir.
 * Example (Windows): `C:\\Users\\name\\.skillstar\\`
 */
export function resolveSkillstarDataPath(home: string, platform: Platform = detectPlatform()): string | null {
  const trimmed = home.replace(/[\\/]+$/, "");
  if (!trimmed) return null;

  if (platform === "windows") {
    const winHome = trimmed.replace(/\//g, "\\");
    return `${winHome}\\.skillstar\\`;
  }
  if (platform === "linux" || platform === "macos") {
    const unixHome = trimmed.replace(/\\/g, "/");
    return `${unixHome}/.skillstar/`;
  }
  return null;
}

// Re-export frontmatter utilities so existing importers don't break.
export {
  normalizeSkillMarkdownForPreview,
  unwrapOuterMarkdownFence,
} from "./frontmatter";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * Return Tailwind size classes for an agent icon.
 * PNG icons have built-in rounded-square chrome and need a
 * larger render size to match the visual weight of bare SVG icons.
 *
 * @param icon  - profile.icon path
 * @param base  - base size for SVG icons, default "w-3.5 h-3.5"
 */
export function agentIconCls(icon: string, base = "w-3.5 h-3.5"): string {
  if (icon.endsWith(".png")) {
    // Bump one step: 3.5→5, 4→5, 5→6
    return base
      .replace(/w-3\.5/, "w-5")
      .replace(/h-3\.5/, "h-5")
      .replace(/w-4\b/, "w-5")
      .replace(/h-4\b/, "h-5");
  }
  return base;
}

/** Format install count: 759100 → "759.1K", 1200000 → "1.2M" */
export function formatInstalls(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}K`;
  return count.toLocaleString();
}

export type SettingsFocusTarget = "ai-provider" | "storage";

/** Navigate to Settings and request focus on a specific section. */
export function navigateToSettingsSection(target: SettingsFocusTarget) {
  try {
    localStorage.setItem("skillstar:settings-focus", target);
  } catch {
    // ignore localStorage access errors
  }
  window.dispatchEvent(new CustomEvent("skillstar:navigate", { detail: { page: "settings" } }));
  window.dispatchEvent(new CustomEvent("skillstar:settings-focus", { detail: { target } }));
}

/** Navigate to AI settings page via custom event */
export function navigateToAiSettings() {
  navigateToSettingsSection("ai-provider");
}

type Translator = (key: string, options?: Record<string, unknown>) => string;

export function formatAiErrorMessage(error: string | null | undefined, t: Translator): string | null {
  if (!error) return null;
  const msg = String(error).trim();
  const lower = msg.toLowerCase();

  if (
    lower.includes("ai provider is disabled") ||
    lower.includes("ai provider is not configured") ||
    lower.includes("api key is empty")
  ) {
    return t("skillEditor.aiNotConfigured", {
      defaultValue: "AI is not configured. Please configure it in Settings.",
    });
  }

  if (lower.includes("mymemory")) {
    return t("detailPanel.mymemoryUnavailable", {
      defaultValue: "MyMemory translation is unavailable right now. Please try again later.",
    });
  }

  if (
    lower.includes("failed to send request") ||
    lower.includes("timed out") ||
    lower.includes("connection") ||
    lower.includes("dns")
  ) {
    return t("detailPanel.networkError", {
      defaultValue: "Network request failed. Please check your network or proxy settings.",
    });
  }

  if (lower.includes("untranslated result")) {
    return t("detailPanel.untranslatedResult", {
      defaultValue: "Translation service returned un-translated text. Please try again or switch provider.",
    });
  }

  return msg;
}

export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch (err) {
    console.warn(
      "navigator.clipboard.writeText failed (likely due to async context loss), falling back to execCommand:",
      err,
    );
  }

  try {
    const textArea = document.createElement("textarea");
    textArea.value = text;
    textArea.style.position = "fixed";
    textArea.style.left = "-999999px";
    textArea.style.top = "-999999px";
    document.body.appendChild(textArea);
    textArea.focus();
    textArea.select();
    const success = document.execCommand("copy");
    textArea.remove();
    return success;
  } catch (e) {
    console.error("Fallback execCommand('copy') failed:", e);
    return false;
  }
}

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

// Re-export frontmatter utilities so existing importers don't break.
export {
  unwrapOuterMarkdownFence,
  normalizeSkillMarkdownForPreview,
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
      .replace(/w-3\.5/, "w-5").replace(/h-3\.5/, "h-5")
      .replace(/w-4\b/, "w-5").replace(/h-4\b/, "h-5");
  }
  return base;
}

/** Format install count: 759100 → "759.1K", 1200000 → "1.2M" */
export function formatInstalls(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}K`;
  return count.toLocaleString();
}

/** Navigate to AI settings page via custom event */
export function navigateToAiSettings() {
  try {
    localStorage.setItem("skillstar:settings-focus", "ai-provider");
  } catch {
    // ignore localStorage access errors
  }
  window.dispatchEvent(
    new CustomEvent("skillstar:navigate", { detail: { page: "settings" } })
  );
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

  return msg;
}

export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch (err) {
    console.warn("navigator.clipboard.writeText failed (likely due to async context loss), falling back to execCommand:", err);
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


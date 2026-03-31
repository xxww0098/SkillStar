import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

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

/** Unwrap an AI response that wraps content in a markdown code fence */
export function unwrapOuterMarkdownFence(text: string): string {
  const trimmed = text.trim();
  const fenced = trimmed.match(/^```(?:markdown|md)?\s*\n([\s\S]*?)\n```$/i);
  return (fenced ? fenced[1] : text).replace(/^\uFEFF/, "");
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

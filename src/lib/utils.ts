import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
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

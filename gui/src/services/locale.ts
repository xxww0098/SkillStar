import { signal } from "nsh";

export type Locale = "zh-CN" | "en";

// i18n.rs normalize parity: "en" or anything en-* is English; every other
// tag follows the product fallback, zh-CN.
export function normalizeLocale(code: string): Locale {
  const trimmed = code.trim();
  return trimmed.toLowerCase() === "en" || trimmed.toLowerCase().startsWith("en-")
    ? "en"
    : "zh-CN";
}

// Initial value: the product default zh-CN (GuiPrefs' serde default — the
// first run has no prefs file, so the system locale does not decide the
// UI language). The prefs-bridge wave re-seeds this from gui_prefs.json's
// `language` field; normalizeLocale above is what that bridge will run raw
// values through.
export const locale = signal<Locale>("zh-CN");

export function setLocale(next: Locale) {
  locale.set(next);
}

export function toggleLocale() {
  locale.set(locale() === "zh-CN" ? "en" : "zh-CN");
}

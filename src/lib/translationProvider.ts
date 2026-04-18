import type { TFunction } from "i18next";

const PROVIDER_LABELS: Record<string, string> = {
  deepl: "DeepL",
  deeplx: "DeepLX",
  mymemory: "MyMemory",
};

export function formatTranslationProviderLabel(provider: string | null | undefined, t?: TFunction): string | null {
  if (!provider) return null;
  const normalized = provider.trim().toLowerCase();
  if (!normalized) return null;

  // Quality LLM providers use "app:provider" format (e.g. "claude:my-key")
  if (normalized.includes(":")) {
    const [app] = normalized.split(":");
    const appLabel = app === "claude" ? "Claude" : app === "codex" ? "Codex" : app;
    return t ? t("detailPanel.translationSourceAi", { defaultValue: appLabel }) : appLabel;
  }

  return PROVIDER_LABELS[normalized] ?? provider.trim();
}

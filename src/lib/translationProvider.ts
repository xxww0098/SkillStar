import type { TFunction } from "i18next";

const PROVIDER_LABELS: Record<string, string> = {
  ai: "AI",
  mymemory: "MyMemory",
  deepl: "DeepL",
  deeplx: "DeepLX",
  google: "Google Translate",
  azure: "Azure Translator",
  gtx: "GTX",
  deepseek: "DeepSeek",
  claude: "Claude",
  openai: "OpenAI",
  gemini: "Gemini",
  perplexity: "Perplexity",
  azureopenai: "Azure OpenAI",
  azure_openai: "Azure OpenAI",
  siliconflow: "SiliconFlow",
  groq: "Groq",
  openrouter: "OpenRouter",
  nvidia: "NVIDIA",
  customllm: "Custom LLM",
  custom_llm: "Custom LLM",
};

export function formatTranslationProviderLabel(provider: string | null | undefined, t?: TFunction): string | null {
  if (!provider) return null;
  const normalized = provider.trim().toLowerCase();
  if (!normalized) return null;

  if (normalized === "ai") {
    return t ? t("detailPanel.translationSourceAi") : PROVIDER_LABELS.ai;
  }
  if (normalized === "mymemory") {
    return t ? t("detailPanel.translationSourceMyMemory") : PROVIDER_LABELS.mymemory;
  }

  return PROVIDER_LABELS[normalized] ?? provider.trim();
}

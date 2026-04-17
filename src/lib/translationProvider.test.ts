import { describe, expect, it } from "vitest";
import { formatTranslationProviderLabel } from "./translationProvider";

describe("formatTranslationProviderLabel", () => {
  it("formats known translation-api providers into user-facing labels", () => {
    expect(formatTranslationProviderLabel("deeplx")).toBe("DeepLX");
    expect(formatTranslationProviderLabel("azureopenai")).toBe("Azure OpenAI");
    expect(formatTranslationProviderLabel("custom_llm")).toBe("Custom LLM");
  });

  it("uses i18n labels for generic ai and mymemory sources when available", () => {
    const t = (key: string) =>
      ({
        "detailPanel.translationSourceAi": "AI",
        "detailPanel.translationSourceMyMemory": "MyMemory",
      })[key] ?? key;

    expect(formatTranslationProviderLabel("ai", t as never)).toBe("AI");
    expect(formatTranslationProviderLabel("mymemory", t as never)).toBe("MyMemory");
  });

  it("falls back to the raw provider token when it is unknown", () => {
    expect(formatTranslationProviderLabel("some-new-provider")).toBe("some-new-provider");
  });

  it("returns null for empty values", () => {
    expect(formatTranslationProviderLabel(null)).toBeNull();
    expect(formatTranslationProviderLabel("   ")).toBeNull();
  });
});

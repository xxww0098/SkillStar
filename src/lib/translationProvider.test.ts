import { describe, expect, it } from "vitest";
import { formatTranslationProviderLabel } from "./translationProvider";

describe("formatTranslationProviderLabel", () => {
  it("formats known translation-api providers into user-facing labels", () => {
    expect(formatTranslationProviderLabel("deepl")).toBe("DeepL");
    expect(formatTranslationProviderLabel("deeplx")).toBe("DeepLX");
    expect(formatTranslationProviderLabel("mymemory")).toBe("MyMemory");
  });

  it("formats quality LLM provider references (app:provider)", () => {
    const t = (key: string, opts?: { defaultValue?: string }) => opts?.defaultValue ?? key;
    expect(formatTranslationProviderLabel("claude:my-key", t as never)).toBe("Claude");
    expect(formatTranslationProviderLabel("codex:provider-1", t as never)).toBe("Codex");
  });

  it("falls back to the raw provider token when it is unknown", () => {
    expect(formatTranslationProviderLabel("some-new-provider")).toBe("some-new-provider");
  });

  it("returns null for empty values", () => {
    expect(formatTranslationProviderLabel(null)).toBeNull();
    expect(formatTranslationProviderLabel("   ")).toBeNull();
  });
});

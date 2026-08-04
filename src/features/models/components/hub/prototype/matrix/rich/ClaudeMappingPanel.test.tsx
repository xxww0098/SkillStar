import { describe, expect, it } from "vitest";
import type { ProviderEntryFlat } from "../../../../../../../types";
import {
  emptyClaudeMap,
  oneClickClaudeMap,
  seedClaudeMap,
  type ClaudeMapState,
} from "./ClaudeMappingPanel";

function provider(partial: Partial<ProviderEntryFlat> = {}): ProviderEntryFlat {
  return {
    id: "p1",
    name: "DeepSeek",
    base_url_openai: "",
    base_url_anthropic: "https://api.deepseek.com/anthropic",
    models_url: "https://api.deepseek.com/models",
    api_key: "sk",
    models: ["deepseek-v4-pro", "deepseek-v4-flash"],
    default_model: "deepseek-v4-pro",
    sort_index: 0,
    ...partial,
  };
}

describe("oneClickClaudeMap", () => {
  it("broadcasts the first filled role model to every role", () => {
    const value: ClaudeMapState = {
      ...emptyClaudeMap(),
      opus: { displayName: "", requestModel: "custom-opus", support1M: true },
    };
    const next = oneClickClaudeMap(value, ["catalog-a"], "default-m");
    expect(next).not.toBeNull();
    expect(next!.sonnet.requestModel).toBe("custom-opus");
    expect(next!.haiku.requestModel).toBe("custom-opus");
    expect(next!.subagent.requestModel).toBe("custom-opus");
    expect(next!.sonnet.displayName).toBe("custom-opus");
    expect(next!.subagent.displayName).toBe("");
    expect(next!.sonnet.support1M).toBe(true);
  });

  it("falls back to default_model then catalog", () => {
    expect(oneClickClaudeMap(emptyClaudeMap(), ["a", "b"], "def")!.sonnet.requestModel).toBe("def");
    expect(oneClickClaudeMap(emptyClaudeMap(), ["a", "b"], "")!.sonnet.requestModel).toBe("a");
    expect(oneClickClaudeMap(emptyClaudeMap(), [], "")).toBeNull();
  });
});

describe("seedClaudeMap", () => {
  it("spreads catalog models across roles", () => {
    const next = seedClaudeMap(provider());
    expect(next.sonnet.requestModel).toBe("deepseek-v4-pro");
    expect(next.opus.requestModel).toBe("deepseek-v4-flash");
  });
});

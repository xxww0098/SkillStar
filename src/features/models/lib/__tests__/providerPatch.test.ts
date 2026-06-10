import { describe, expect, it } from "vitest";
import type { ProviderEntryFlat } from "../../../../types";
import {
  buildModelCatalog,
  buildProviderPatch,
  CLAUDE_MODEL_META_KEYS,
  computeDirty,
  getMetaString,
  getModelCatalogFromMeta,
  providerToFormValues,
  validatePatch,
} from "../providerPatch";

function makeProvider(overrides: Partial<ProviderEntryFlat> = {}): ProviderEntryFlat {
  return {
    id: "p1",
    preset_id: "deepseek",
    name: "DeepSeek",
    api_key: "sk-test",
    base_url_openai: "https://api.deepseek.com/v1",
    base_url_anthropic: "https://api.deepseek.com/anthropic",
    models_url: "https://api.deepseek.com/v1/models",
    models: ["deepseek-chat"],
    default_model: "deepseek-chat",
    sort_index: 0,
    meta: {
      timeout: 30,
      [CLAUDE_MODEL_META_KEYS.main]: "deepseek-chat",
      custom_key: "keep-me",
    },
    ...overrides,
  } as ProviderEntryFlat;
}

describe("buildModelCatalog", () => {
  it("trims, drops empties and dedupes preserving order", () => {
    expect(buildModelCatalog([" a ", "b", "", "a", "  ", "c", "b"])).toEqual(["a", "b", "c"]);
  });
});

describe("getMetaString / getModelCatalogFromMeta", () => {
  it("returns empty string for missing or non-string values", () => {
    expect(getMetaString(undefined, "x")).toBe("");
    expect(getMetaString({ x: 3 }, "x")).toBe("");
    expect(getMetaString({ x: "v" }, "x")).toBe("v");
  });

  it("filters malformed catalog entries", () => {
    const meta = { model_catalog: [{ id: "m1" }, { nope: true }, null, "str"] };
    expect(getModelCatalogFromMeta(meta)).toEqual([{ id: "m1" }]);
  });
});

describe("validatePatch", () => {
  it("requires a name", () => {
    expect(validatePatch({ name: "  " })).toMatch(/名称/);
    expect(validatePatch({ name: "x" })).toBeNull();
  });

  it("rejects non-http(s) urls but allows empty", () => {
    expect(validatePatch({ name: "x", base_url_openai: "ftp://x" })).toMatch(/OpenAI/);
    expect(validatePatch({ name: "x", base_url_anthropic: "not a url" })).toMatch(/Anthropic/);
    expect(validatePatch({ name: "x", models_url: "javascript:alert(1)" })).toMatch(/模型/);
    expect(validatePatch({ name: "x", base_url_openai: "", models_url: "" })).toBeNull();
  });
});

describe("providerToFormValues", () => {
  it("applies the documented defaults for missing meta", () => {
    const values = providerToFormValues(makeProvider({ meta: undefined }));
    expect(values.contextLength).toBe(128000);
    expect(values.maxTokens).toBe(4096);
    expect(values.timeout).toBe(30);
    expect(values.retryCount).toBe(3);
    expect(values.streaming).toBe(true);
    expect(values.codexWireApi).toBe("responses");
    expect(values.codexAuthMode).toBe("api_key");
  });

  it("prefers top-level codex fields over meta fallbacks", () => {
    const provider = makeProvider({
      codex_wire_api: "chat",
      meta: { codex_wire_api: "responses" },
    });
    expect(providerToFormValues(provider).codexWireApi).toBe("chat");
  });
});

describe("buildProviderPatch", () => {
  it("collects referenced model ids and preserves unrelated meta keys", () => {
    const provider = makeProvider();
    const values = {
      ...providerToFormValues(provider),
      models: ["deepseek-chat"],
      defaultModel: "deepseek-coder",
      claudeMainModel: "claude-x",
    };
    const patch = buildProviderPatch(values, provider.meta);
    expect(patch.models).toEqual(["deepseek-chat", "deepseek-coder", "claude-x"]);
    expect(patch.default_model).toBe("deepseek-coder");
    expect((patch.meta as Record<string, unknown>).custom_key).toBe("keep-me");
    expect((patch.meta as Record<string, unknown>)[CLAUDE_MODEL_META_KEYS.main]).toBe("claude-x");
  });

  it("trims fields and turns empty notes into undefined", () => {
    const provider = makeProvider();
    const values = { ...providerToFormValues(provider), name: "  X  ", notes: "   " };
    const patch = buildProviderPatch(values, provider.meta);
    expect(patch.name).toBe("X");
    expect(patch.notes).toBeUndefined();
  });
});

describe("computeDirty", () => {
  it("is false for untouched values", () => {
    const provider = makeProvider();
    expect(computeDirty(provider, providerToFormValues(provider))).toBe(false);
  });

  it("detects each kind of edit", () => {
    const provider = makeProvider();
    const base = providerToFormValues(provider);
    expect(computeDirty(provider, { ...base, name: "Other" })).toBe(true);
    expect(computeDirty(provider, { ...base, apiKey: "sk-new" })).toBe(true);
    expect(computeDirty(provider, { ...base, defaultModel: "deepseek-coder" })).toBe(true);
    expect(computeDirty(provider, { ...base, streaming: false })).toBe(true);
    expect(computeDirty(provider, { ...base, codexWireApi: "chat" })).toBe(true);
    expect(computeDirty(provider, { ...base, modelCatalog: [{ id: "m" }] })).toBe(true);
  });

  it("treats referenced tier models as part of the models list", () => {
    const provider = makeProvider({ models: ["deepseek-chat"] });
    const base = providerToFormValues(provider);
    // claudeHaikuModel adds a new id to the persisted models list → dirty
    expect(computeDirty(provider, { ...base, claudeHaikuModel: "haiku-x" })).toBe(true);
  });
});

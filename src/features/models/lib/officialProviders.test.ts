import { describe, expect, it } from "vitest";
import type { ProviderEntryFlat } from "../../../types";
import {
  CLAUDE_OFFICIAL_ID,
  CODEX_OFFICIAL_ID,
  findOfficialProvider,
  isNativeOfficialProvider,
  isToolOnOfficial,
  matrixProviders,
  officialBindToolId,
  officialProviderIdForTool,
  toolSupportsOfficial,
  withEnsuredOfficialProviders,
} from "./officialProviders";

function thirdParty(id: string, sort = 0): ProviderEntryFlat {
  return {
    id,
    name: id,
    base_url_openai: "https://example.com/v1",
    base_url_anthropic: "",
    models_url: "",
    api_key: "sk",
    models: ["m"],
    default_model: "m",
    sort_index: sort,
    preset_id: id,
  };
}

describe("officialProviders", () => {
  it("detects Official by id or preset_id", () => {
    expect(isNativeOfficialProvider({ id: CLAUDE_OFFICIAL_ID })).toBe(true);
    expect(isNativeOfficialProvider({ id: "p-1", preset_id: CODEX_OFFICIAL_ID })).toBe(true);
    expect(isNativeOfficialProvider({ id: "p-deepseek", preset_id: "deepseek" })).toBe(false);
  });

  it("maps tools ↔ Official seed ids", () => {
    expect(toolSupportsOfficial("claude-code")).toBe(true);
    expect(toolSupportsOfficial("claude-desktop")).toBe(true);
    expect(toolSupportsOfficial("codex")).toBe(true);
    expect(toolSupportsOfficial("opencode")).toBe(false);
    expect(officialProviderIdForTool("claude-code")).toBe(CLAUDE_OFFICIAL_ID);
    expect(officialProviderIdForTool("claude-desktop")).toBe(CLAUDE_OFFICIAL_ID);
    expect(officialProviderIdForTool("codex")).toBe(CODEX_OFFICIAL_ID);
    expect(officialBindToolId({ id: CLAUDE_OFFICIAL_ID })).toBe("claude-code");
    expect(officialBindToolId({ id: CODEX_OFFICIAL_ID })).toBe("codex");
  });

  it("hides Official from matrix provider rows", () => {
    const next = withEnsuredOfficialProviders([thirdParty("deepseek", 1)]);
    const rows = matrixProviders(next);
    expect(rows.map((p) => p.id)).toEqual(["deepseek"]);
    expect(rows.every((p) => !isNativeOfficialProvider(p))).toBe(true);
  });

  it("injects missing Official seeds without duplicating", () => {
    const once = withEnsuredOfficialProviders([thirdParty("deepseek")]);
    expect(once.filter((p) => isNativeOfficialProvider(p)).map((p) => p.id).sort()).toEqual([
      CLAUDE_OFFICIAL_ID,
      CODEX_OFFICIAL_ID,
    ]);
    const again = withEnsuredOfficialProviders(once);
    expect(again.filter((p) => p.id === CLAUDE_OFFICIAL_ID)).toHaveLength(1);
  });

  it("reports when a tool is bound to Official", () => {
    const providers = withEnsuredOfficialProviders([]);
    expect(isToolOnOfficial(providers, {}, "claude-code")).toBe(false);
    expect(
      isToolOnOfficial(
        providers,
        { "claude-code": { entries: [{ provider_id: CLAUDE_OFFICIAL_ID, model: "" }], active_index: 0 } },
        "claude-code",
      ),
    ).toBe(true);
    expect(findOfficialProvider(providers, "codex")?.id).toBe(CODEX_OFFICIAL_ID);
  });
});

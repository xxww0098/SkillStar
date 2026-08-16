import { describe, expect, it } from "vitest";
import type { ProviderEntryFlat, RoleTarget } from "../../../../../../types";
import type { RoleDefDto } from "../../../../../../types/generated/RoleDefDto";
import { claudeFillCount, oneClickClaudeRoles, seedClaudeRoles } from "./ClaudeMappingPanel";

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

function role(id: string, agentKey: string, inherits: string | null = null): RoleDefDto {
  return { id, agent_key: agentKey, primary: true, inherits, requires: "any" };
}

const DEFS: RoleDefDto[] = [
  role("default", "ANTHROPIC_MODEL"),
  role("fast", "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
  role("sonnet", "ANTHROPIC_DEFAULT_SONNET_MODEL"),
  role("opus", "ANTHROPIC_DEFAULT_OPUS_MODEL"),
  role("subagent", "CLAUDE_CODE_SUBAGENT_MODEL", "default"),
];

describe("oneClickClaudeRoles", () => {
  it("broadcasts the first filled role model to every declared role", () => {
    const roles: Record<string, RoleTarget> = {
      opus: { provider_id: "p1", model: "custom-opus" },
    };
    const next = oneClickClaudeRoles(roles, DEFS, "p1", ["catalog-a"], "default-m");
    expect(next).not.toBeNull();
    expect(Object.keys(next!).sort()).toEqual(["default", "fast", "opus", "sonnet", "subagent"]);
    for (const def of DEFS) {
      expect(next![def.id]).toEqual({ provider_id: "p1", model: "custom-opus" });
    }
  });

  it("falls back to default_model then catalog", () => {
    expect(oneClickClaudeRoles({}, DEFS, "p1", ["a", "b"], "def")!.fast.model).toBe("def");
    expect(oneClickClaudeRoles({}, DEFS, "p1", ["a", "b"], "")!.fast.model).toBe("a");
    expect(oneClickClaudeRoles({}, DEFS, "p1", [], "")).toBeNull();
  });

  /** Every assignment must name the bound provider — a role pointing elsewhere
   *  is one the writer skips, and the panel must not create one. */
  it("targets the bound provider on every row", () => {
    const next = oneClickClaudeRoles({}, DEFS, "p-bound", ["m"], "");
    expect(new Set(Object.values(next!).map((target) => target.provider_id))).toEqual(new Set(["p-bound"]));
  });
});

describe("seedClaudeRoles", () => {
  it("spreads catalog models across the declared roles", () => {
    const next = seedClaudeRoles(provider(), DEFS);
    expect(next.default.model).toBe("deepseek-v4-pro");
    expect(next.fast.model).toBe("deepseek-v4-flash");
  });

  it("produces nothing when the provider has no models at all", () => {
    expect(seedClaudeRoles(provider({ models: [], default_model: "" }), DEFS)).toEqual({});
  });
});

describe("claudeFillCount", () => {
  it("counts only roles that would actually be written", () => {
    const roles: Record<string, RoleTarget> = {
      default: { provider_id: "p1", model: "m" },
      // Whitespace is not a model: the writer drops the key rather than
      // writing an empty value Claude cannot resolve.
      fast: { provider_id: "p1", model: "  " },
      // A role the agent does not declare must not inflate the count.
      fable: { provider_id: "p1", model: "fable-5" },
    };
    expect(claudeFillCount(roles, DEFS)).toEqual({ filled: 1, total: 5 });
  });
});

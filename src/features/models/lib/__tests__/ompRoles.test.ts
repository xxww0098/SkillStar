import { describe, expect, it } from "vitest";
import {
  buildOmpLaunchCommand,
  isDefaultThinking,
  OMP_DEFAULT_CYCLE_ORDER,
  OMP_PRIMARY_ROLES,
  OMP_ROLE_DEFS,
  OMP_ROLES_INHERITING_DEFAULT,
  OMP_SECONDARY_ROLES,
  OMP_THINKING_LEVELS,
  OMP_TOOL_ID,
  modelReasoning,
  ompThinkingLevelsFor,
  previewRoleValue,
} from "../ompRoles";

/**
 * Cross-language registry reconciliation (same contract as
 * `agentRegistry.test.ts`).
 *
 * `OMP_MODEL_ROLES` and `OMP_THINKING_LEVELS` in
 * `crates/skillstar-models/src/tool_sync/types.rs` decide what actually reaches
 * `~/.omp/agent/config.yml`. The literals below are copies of those lists: if
 * either side gains, loses or reorders an entry without the other, this test
 * goes red and the panel stops offering a role the writer would silently drop.
 */
const RUST_OMP_MODEL_ROLES = [
  "default",
  "smol",
  "slow",
  "plan",
  "vision",
  "designer",
  "commit",
  "tiny",
  "task",
  "advisor",
] as const;

const RUST_OMP_THINKING_LEVELS = [
  "inherit",
  "off",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "auto",
] as const;

describe("OMP_ROLE_DEFS", () => {
  it("covers exactly the Rust role list, in the same order", () => {
    // Compared on `agentKey`, not `id`: the store speaks canonical ids
    // (`fast`, `subagent`) and `config.yml` speaks OMP's (`smol`, `task`).
    // `RoleDef.agent_key` in the Rust registry is the same translation, and
    // `registry_agent_keys_match_the_migration_table` pins that side.
    expect(OMP_ROLE_DEFS.map((r) => r.agentKey)).toEqual([...RUST_OMP_MODEL_ROLES]);
  });

  it("stores under canonical ids so a migrated role map is still readable", () => {
    // v4 renamed `smol` to `fast` and `task` to `subagent` in the store. A
    // panel keyed by the on-disk names would show every migrated user an empty
    // row and then write a duplicate role beside their real one.
    const byKey = Object.fromEntries(OMP_ROLE_DEFS.map((r) => [r.agentKey, r.id]));
    expect(byKey.smol).toBe("fast");
    expect(byKey.task).toBe("subagent");
    expect(byKey.default).toBe("default");
  });

  it("mirrors the Rust thinking levels", () => {
    expect([...OMP_THINKING_LEVELS]).toEqual([...RUST_OMP_THINKING_LEVELS]);
  });

  it("splits into the four flag-backed roles and six secondary ones", () => {
    expect(OMP_PRIMARY_ROLES.map((r) => r.id)).toEqual(["default", "fast", "slow", "plan"]);
    expect(OMP_SECONDARY_ROLES.map((r) => r.id)).toEqual([
      "vision",
      "designer",
      "commit",
      "tiny",
      "subagent",
      "advisor",
    ]);
    expect(OMP_PRIMARY_ROLES.length + OMP_SECONDARY_ROLES.length).toBe(OMP_ROLE_DEFS.length);
  });

  it("gives a CLI flag to primary roles only", () => {
    for (const role of OMP_ROLE_DEFS) {
      expect(Boolean(role.flag)).toBe(role.primary);
    }
    expect(OMP_PRIMARY_ROLES.map((r) => r.flag)).toEqual(["--model", "--smol", "--slow", "--plan"]);
  });

  it("keys are unique and usable as i18n suffixes", () => {
    expect(new Set(OMP_ROLE_DEFS.map((r) => r.key)).size).toBe(OMP_ROLE_DEFS.length);
    // The i18n suffix follows OMP's own name, which is what the copy describes.
    expect(OMP_ROLE_DEFS.every((r) => r.key === r.agentKey)).toBe(true);
  });

  it("cycle order and default-inheriting roles reference real roles", () => {
    const ids = new Set(OMP_ROLE_DEFS.map((r) => r.id));
    const agentKeys = new Set(OMP_ROLE_DEFS.map((r) => r.agentKey));
    // The cycle order is OMP's own setting, so it is spelled OMP's way; the
    // inheritance list is a store-side fact, so it is spelled canonically.
    for (const key of OMP_DEFAULT_CYCLE_ORDER) expect(agentKeys.has(key)).toBe(true);
    for (const id of OMP_ROLES_INHERITING_DEFAULT) expect(ids.has(id)).toBe(true);
    // Ctrl+P never reaches `plan`, which is why the panel says so.
    expect(OMP_DEFAULT_CYCLE_ORDER).not.toContain("plan");
  });

  it("binds to the omp tool id used by the matrix column", () => {
    expect(OMP_TOOL_ID).toBe("omp");
  });
});

/**
 * The managed key must match `skillstar_managed_key`
 * (`crates/skillstar-models/src/tool_sync/multi_provider.rs`): first 8 chars of
 * the provider id, ASCII-lowercased, every non-alphanumeric replaced by `_`.
 */
describe("previewRoleValue", () => {
  it("derives the managed key from the first 8 chars of the provider id", () => {
    expect(previewRoleValue("a1b2c3d4e5f6", "deepseek-chat")).toBe("skillstar_a1b2c3d4/deepseek-chat");
  });

  it("lowercases uppercase ids", () => {
    expect(previewRoleValue("ABCDEF12", "gpt-5")).toBe("skillstar_abcdef12/gpt-5");
    expect(previewRoleValue("MiXeD-Id-9", "gpt-5")).toBe("skillstar_mixed_id/gpt-5");
  });

  it("replaces every non-alphanumeric char with an underscore", () => {
    expect(previewRoleValue("a-b.c d!", "m")).toBe("skillstar_a_b_c_d_/m");
    // The prefix underscore plus eight replaced dashes — nine in a row.
    expect(previewRoleValue("--------x", "m")).toBe(`skillstar_${"_".repeat(8)}/m`);
  });

  it("pads nothing for ids shorter than 8 chars", () => {
    expect(previewRoleValue("ab", "m")).toBe("skillstar_ab/m");
  });

  it("appends the thinking level only when it is not the inherit default", () => {
    expect(previewRoleValue("abcdefgh", "m", "xhigh")).toBe("skillstar_abcdefgh/m:xhigh");
    expect(previewRoleValue("abcdefgh", "m", "inherit")).toBe("skillstar_abcdefgh/m");
    expect(previewRoleValue("abcdefgh", "m", null)).toBe("skillstar_abcdefgh/m");
    expect(previewRoleValue("abcdefgh", "m", undefined)).toBe("skillstar_abcdefgh/m");
  });

  it("trims the model and returns null for an incomplete assignment", () => {
    expect(previewRoleValue("abcdefgh", "  spaced  ")).toBe("skillstar_abcdefgh/spaced");
    expect(previewRoleValue("abcdefgh", "   ")).toBeNull();
    expect(previewRoleValue("", "m")).toBeNull();
  });
});

describe("isDefaultThinking", () => {
  it("treats unset and `inherit` as the same thing", () => {
    expect(isDefaultThinking(undefined)).toBe(true);
    expect(isDefaultThinking(null)).toBe(true);
    expect(isDefaultThinking("inherit")).toBe(true);
  });

  it("treats every other level as an explicit override", () => {
    for (const level of OMP_THINKING_LEVELS.filter((l) => l !== "inherit")) {
      expect(isDefaultThinking(level)).toBe(false);
    }
  });
});

describe("buildOmpLaunchCommand", () => {
  it("emits flags in primary-role order and skips unset roles", () => {
    expect(
      buildOmpLaunchCommand({
        slow: "skillstar_aaaaaaaa/deep",
        default: "skillstar_aaaaaaaa/base",
      }),
    ).toBe('omp --model "skillstar_aaaaaaaa/base" --slow "skillstar_aaaaaaaa/deep"');
  });

  it("returns the bare command when nothing is assigned", () => {
    expect(buildOmpLaunchCommand({})).toBe("omp");
    expect(buildOmpLaunchCommand({ default: "   " })).toBe("omp");
  });

  it("ignores secondary roles, which have no CLI flag", () => {
    expect(buildOmpLaunchCommand({ vision: "skillstar_aaaaaaaa/vlm" })).toBe("omp");
  });

  it("covers all four flags", () => {
    // Keyed by canonical id, matching the role map the panel holds.
    expect(buildOmpLaunchCommand({ default: "a", fast: "b", slow: "c", plan: "d" })).toBe(
      'omp --model "a" --smol "b" --slow "c" --plan "d"',
    );
  });
});

/**
 * Narrowing the thinking picker by what the model can do (02 §9.3 gap 2).
 *
 * The expectations below are the same table as
 * `tool_sync::tests::roles::*_thinking_*` in Rust. Both sides have to agree:
 * this one decides what the user can pick, that one decides what gets written,
 * and a picker offering a level the writer discards is the defect being fixed.
 */
describe("ompThinkingLevelsFor", () => {
  it("keeps the whole grammar when the catalogue says nothing", () => {
    expect(ompThinkingLevelsFor(null)).toEqual([...OMP_THINKING_LEVELS]);
    expect(ompThinkingLevelsFor(undefined)).toEqual([...OMP_THINKING_LEVELS]);
  });

  it("offers no tiers for a model with no reasoning mode", () => {
    expect(ompThinkingLevelsFor({ kind: "none" })).toEqual(["inherit"]);
  });

  it("offers exactly an effort model's tiers, in grammar order", () => {
    expect(
      ompThinkingLevelsFor({
        kind: "effort",
        // Out of order on purpose: the picker must still read low → high.
        values: ["high", "low", "medium"],
        default: "medium",
        can_disable: true,
      }),
    ).toEqual(["inherit", "off", "low", "medium", "high", "auto"]);
  });

  it("omits `off` when the model cannot disable reasoning", () => {
    expect(ompThinkingLevelsFor({ kind: "effort", values: ["low"], default: null, can_disable: false })).toEqual([
      "inherit",
      "low",
      "auto",
    ]);
  });

  it("maps a budget model onto the tiers OMP's suffix grammar can express", () => {
    const levels = ompThinkingLevelsFor({ kind: "budget_tokens", min: 1024, max: 32000, default: 4096 });
    expect(levels).toContain("high");
    // A token count has no spelling in `provider/model:level`.
    expect(levels).not.toContain("minimal");
  });
});

describe("modelReasoning", () => {
  const catalog = [{ id: "thinky", reasoning: { kind: "toggle" as const, can_disable: true } }, { id: "plain" }];

  it("finds the capability recorded for a model", () => {
    expect(modelReasoning(catalog, "thinky")).toEqual({ kind: "toggle", can_disable: true });
  });

  it("returns null for a model the catalogue does not cover", () => {
    // Not `{kind:"none"}`: "we have no entry" and "it has no reasoning" lead to
    // opposite pickers.
    expect(modelReasoning(catalog, "plain")).toBeNull();
    expect(modelReasoning(catalog, "unknown-model")).toBeNull();
    expect(modelReasoning(catalog, "  ")).toBeNull();
  });
});

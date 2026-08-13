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
    expect(OMP_ROLE_DEFS.map((r) => r.id)).toEqual([...RUST_OMP_MODEL_ROLES]);
  });

  it("mirrors the Rust thinking levels", () => {
    expect([...OMP_THINKING_LEVELS]).toEqual([...RUST_OMP_THINKING_LEVELS]);
  });

  it("splits into the four flag-backed roles and six secondary ones", () => {
    expect(OMP_PRIMARY_ROLES.map((r) => r.id)).toEqual(["default", "smol", "slow", "plan"]);
    expect(OMP_SECONDARY_ROLES.map((r) => r.id)).toEqual(["vision", "designer", "commit", "tiny", "task", "advisor"]);
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
    expect(OMP_ROLE_DEFS.every((r) => r.key === r.id)).toBe(true);
  });

  it("cycle order and default-inheriting roles reference real roles", () => {
    const ids = new Set(OMP_ROLE_DEFS.map((r) => r.id));
    for (const id of [...OMP_DEFAULT_CYCLE_ORDER, ...OMP_ROLES_INHERITING_DEFAULT]) {
      expect(ids.has(id)).toBe(true);
    }
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
    expect(buildOmpLaunchCommand({ default: "a", smol: "b", slow: "c", plan: "d" })).toBe(
      'omp --model "a" --smol "b" --slow "c" --plan "d"',
    );
  });
});

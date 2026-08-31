import { describe, expect, it } from "vitest";
import { firstSkipPath, formatBatchToggleSkip, SKIP_UNMANAGED_REAL_DIRECTORY } from "./batchToggleSkip";

const t = (key: string, options?: Record<string, unknown>) => {
  if (key === "skillToggle.skipUnmanagedDirItem") {
    return `${options?.name} occupies ${options?.path}`;
  }
  return key;
};

describe("formatBatchToggleSkip", () => {
  it("localizes an unmanaged-directory skip without leaking the English reason", () => {
    const text = formatBatchToggleSkip(
      {
        skill_name: "research",
        code: SKIP_UNMANAGED_REAL_DIRECTORY,
        path: "/Users/xxww/.hermes/skills/research",
        reason: "name collision: target '/tmp/skills/research' is an unmanaged real directory",
      },
      t,
    );
    expect(text).toBe("research occupies /Users/xxww/.hermes/skills/research");
    expect(text).not.toContain("unmanaged real directory");
  });

  it("falls back to skill + reason for unknown codes", () => {
    expect(formatBatchToggleSkip({ skill_name: "demo", code: "other", path: "", reason: "not found in hub" }, t)).toBe(
      "demo: not found in hub",
    );
  });
});

describe("firstSkipPath", () => {
  it("returns the first non-empty path", () => {
    expect(
      firstSkipPath([
        { skill_name: "a", code: "x", path: "", reason: "" },
        { skill_name: "b", code: "x", path: "/tmp/b", reason: "" },
      ]),
    ).toBe("/tmp/b");
  });
});

import { describe, expect, it } from "vitest";

describe("smoke test", () => {
  it("should pass basic assertion", () => {
    expect(1 + 1).toBe(2);
  });

  it("should handle string operations", () => {
    expect("SkillStar".toLowerCase()).toBe("skillstar");
  });
});

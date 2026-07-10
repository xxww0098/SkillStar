import { describe, expect, it } from "vitest";
import { DefaultUsageBody } from "./DefaultUsageBody";
import { resolveUsageBodyRegistration, USAGE_BODY_REGISTRY } from "./bodyRegistry";

/** Specialized catalog ids — locked in test only (AGENTS enumerable-fact rule). */
const EXPECTED_SPECIALIZED = ["cursor", "deepseek", "glm", "xai"] as const;

describe("USAGE_BODY_REGISTRY", () => {
  it("registry keys match specialized catalog set", () => {
    expect(Object.keys(USAGE_BODY_REGISTRY).sort()).toEqual([...EXPECTED_SPECIALIZED].sort());
  });

  it("every registration declares owns* fields", () => {
    for (const [id, reg] of Object.entries(USAGE_BODY_REGISTRY)) {
      expect(typeof reg.ownsCredits, id).toBe("boolean");
      expect(typeof reg.ownsBalance, id).toBe("boolean");
      expect(
        reg.ownsPrimaryReset === true || reg.ownsPrimaryReset === false || reg.ownsPrimaryReset === "infer",
        id,
      ).toBe(true);
      expect(resolveUsageBodyRegistration(id).component).not.toBe(DefaultUsageBody);
    }
  });

  it("unknown catalog falls back to DefaultUsageBody", () => {
    const reg = resolveUsageBodyRegistration("unknown-vendor-xyz");
    expect(reg.component).toBe(DefaultUsageBody);
    expect(reg.ownsCredits).toBe(false);
    expect(reg.ownsBalance).toBe(false);
    expect(reg.ownsPrimaryReset).toBe("infer");
  });
});

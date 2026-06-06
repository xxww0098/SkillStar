import { describe, expect, it } from "vitest";
import { isBreakdownQuotaWindow, isMonetaryQuota } from "./usageLabels";

describe("isBreakdownQuotaWindow", () => {
  it("detects Antigravity-style percent breakdown windows", () => {
    const window = {
      label: "模型额度",
      total: 100,
      breakdown: [{ label: "Claude/GPT", used: 75, total: 100, percent: 75 }],
    };

    expect(isBreakdownQuotaWindow(window)).toBe(true);
    expect(isMonetaryQuota(window)).toBe(false);
  });

  it("does not treat Cursor monetary windows as percent breakdown", () => {
    const window = {
      label: "Total",
      total: 20_000,
      breakdown: [{ label: "Auto + Composer", used: 5_000, total: 20_000, percent: 25 }],
    };

    expect(isBreakdownQuotaWindow(window)).toBe(false);
    expect(isMonetaryQuota(window)).toBe(true);
  });
});

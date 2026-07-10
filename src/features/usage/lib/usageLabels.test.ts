import { describe, expect, it } from "vitest";
import { isBreakdownQuotaWindow, isMonetaryQuota, remainingBarPercent } from "./usageLabels";

describe("remainingBarPercent", () => {
  it("is full when nothing is used and empty when fully consumed", () => {
    expect(remainingBarPercent(0)).toBe(100);
    expect(remainingBarPercent(100)).toBe(0);
    expect(remainingBarPercent(30)).toBe(70);
  });

  it("clamps out-of-range values", () => {
    expect(remainingBarPercent(-5)).toBe(100);
    expect(remainingBarPercent(140)).toBe(0);
  });
});

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

  it("treats Grok weekly credits as non-monetary even if a stale total is present", () => {
    // Strict weekly bars are percent-only; never invent $ from monthlyLimit.
    const window = {
      label: "Weekly credits",
      total: 20_000,
      used: 2_600,
      breakdown: [],
    };

    expect(isMonetaryQuota(window)).toBe(false);
    expect(isBreakdownQuotaWindow(window)).toBe(false);
  });

  it("keeps percent-only weekly bars non-monetary", () => {
    expect(
      isMonetaryQuota({
        label: "Weekly credits",
        total: null,
        breakdown: [],
      }),
    ).toBe(false);
  });
});

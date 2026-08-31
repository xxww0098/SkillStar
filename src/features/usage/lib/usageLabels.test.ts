import { describe, expect, it } from "vitest";
import type { TFunction } from "i18next";
import {
  formatAntigravityQuotaLabel,
  isAbsoluteQuotaWindow,
  isBreakdownQuotaWindow,
  isMonetaryQuota,
  remainingBarPercent,
  subscriptionCardTitle,
} from "./usageLabels";

const testT = ((key: string) =>
  ({
    "usage.antigravityGeminiModels": "Gemini 模型",
    "usage.antigravityClaudeGptModels": "Claude / GPT 模型",
    "usage.antigravityWeeklyLimit": "周额度",
    "usage.antigravityFiveHourLimit": "5 小时额度",
  })[key] ?? key) as TFunction;

describe("subscriptionCardTitle", () => {
  it("strips catalog · prefix and legacy Grok · names", () => {
    expect(subscriptionCardTitle("Grok · user@x.com", "Grok")).toBe("user@x.com");
    expect(subscriptionCardTitle("Grok · user@x.com")).toBe("user@x.com");
    expect(subscriptionCardTitle("Codex · personal", "Codex")).toBe("personal");
    expect(subscriptionCardTitle("user@x.com", "Grok")).toBe("user@x.com");
    expect(subscriptionCardTitle("plain name")).toBe("plain name");
  });
});

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

describe("formatAntigravityQuotaLabel", () => {
  it("shortens known quota labels and preserves the full tooltip", () => {
    expect(formatAntigravityQuotaLabel("Gemini Models · Weekly Limit", testT)).toEqual({
      display: "Gemini 模型 · 周额度",
      title: "Gemini Models · Weekly Limit",
    });
  });

  it("also cleans labels from snapshots written before suffix normalization", () => {
    expect(formatAntigravityQuotaLabel("Claude and GPT models · Five Hour Limit Remaining", testT)).toEqual({
      display: "Claude / GPT 模型 · 5 小时额度",
      title: "Claude and GPT models · Five Hour Limit",
    });
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

describe("isAbsoluteQuotaWindow", () => {
  it("treats total === 100 as percent-based quota, never absolute", () => {
    expect(isAbsoluteQuotaWindow({ label: "Auto + Composer", used: 47, total: 100 })).toBe(false);
    expect(isAbsoluteQuotaWindow({ label: "Gemini", used: 0, total: 100 })).toBe(false);
    expect(isAbsoluteQuotaWindow({ label: "OverQuota", used: 120, total: 100 })).toBe(false);
  });

  it("detects real absolute token/request quotas", () => {
    expect(isAbsoluteQuotaWindow({ label: "Tokens", used: 500, total: 2000 })).toBe(true);
  });

  it("rejects 5h / 7d rate limit windows and monetary windows", () => {
    expect(isAbsoluteQuotaWindow({ label: "5h", used: 20, total: 1000 })).toBe(false);
    expect(isAbsoluteQuotaWindow({ label: "7d", used: 20, total: 1000 })).toBe(false);
    expect(
      isAbsoluteQuotaWindow({
        label: "Total",
        used: 5000,
        total: 20_000,
        breakdown: [{ label: "API", used: 100, total: 100 }],
      }),
    ).toBe(false);
  });
});

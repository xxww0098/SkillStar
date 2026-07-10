import { describe, expect, it } from "vitest";
import type { UsageWindow } from "../types";
import { computeBodyOwnsPrimaryReset, windowRendersOwnReset } from "./resetOwnership";

function win(over: Partial<UsageWindow> = {}): UsageWindow {
  return {
    label: "Weekly credits",
    used: 0,
    total: null,
    percent: 40,
    reset_at: 1_700_000_000,
    ...over,
  };
}

describe("windowRendersOwnReset", () => {
  it("false without reset_at", () => {
    expect(windowRendersOwnReset(win({ reset_at: null }))).toBe(false);
  });

  it("false for Codex 5h/7d labels (defer to meta)", () => {
    expect(windowRendersOwnReset(win({ label: "5h" }))).toBe(false);
    expect(windowRendersOwnReset(win({ label: "7d" }))).toBe(false);
  });

  it("false for monetary / breakdown / absolute", () => {
    expect(windowRendersOwnReset(win({ label: "Total", total: 10000, used: 1000 }))).toBe(false);
  });

  it("true for simple weekly-style bar with reset", () => {
    expect(windowRendersOwnReset(win({ label: "Weekly credits", total: null, percent: 50 }))).toBe(true);
  });
});

describe("computeBodyOwnsPrimaryReset", () => {
  it("honors explicit true/false", () => {
    expect(computeBodyOwnsPrimaryReset(null, null, true)).toBe(true);
    expect(computeBodyOwnsPrimaryReset(null, null, false)).toBe(false);
  });

  it("infers from weekly source + simple bar", () => {
    const usage = { weekly: win({ label: "Weekly credits" }), hourly: null, monthly: null };
    const resetInfo = {
      resetAt: 1_700_000_000,
      usedPercent: 40,
      mode: "billing" as const,
      source: "weekly" as const,
    };
    expect(computeBodyOwnsPrimaryReset(usage, resetInfo, "infer")).toBe(true);
  });
});

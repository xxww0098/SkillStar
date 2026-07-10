import { describe, expect, it } from "vitest";
import { BALANCE_PARSERS, parseBalanceResponse } from "./balance";

// Fixtures mirror the real response shapes documented on BALANCE_PARSERS.
// Every parser key must have a fixture here — the loop test below enforces it,
// and the Rust side pins the key set in
// `flat_preset_balance_parsers_are_pinned_to_frontend` (providers/tests/part2.rs).
const FIXTURES: Record<string, { raw: unknown; available: number; currency: string }> = {
  deepseek: {
    raw: { balance_infos: [{ total_balance: "12.34", currency: "CNY" }] },
    available: 12.34,
    currency: "CNY",
  },
  kimi: {
    raw: { data: { available_balance: 1.23, total_balance: 5.0, currency: "CNY" } },
    available: 1.23,
    currency: "CNY",
  },
  openrouter: {
    raw: { data: { total_credits: 5.0, usage: 3.5 } },
    available: 1.5,
    currency: "USD",
  },
  siliconflow: {
    raw: { data: { balance: "7.89" } },
    available: 7.89,
    currency: "CNY",
  },
};

describe("parseBalanceResponse", () => {
  it("has a fixture for every registered parser", () => {
    expect(Object.keys(FIXTURES).sort()).toEqual(Object.keys(BALANCE_PARSERS).sort());
  });

  for (const [presetId, fixture] of Object.entries(FIXTURES)) {
    it(`parses the ${presetId} response shape`, () => {
      const parsed = parseBalanceResponse(presetId, fixture.raw);
      expect(parsed).not.toBeNull();
      expect(parsed?.available).toBeCloseTo(fixture.available);
      expect(parsed?.currency).toBe(fixture.currency);
      expect(parsed?.updated_at).toBeGreaterThan(0);
    });
  }

  it("returns null for a preset without a parser", () => {
    expect(parseBalanceResponse("grok", { anything: 1 })).toBeNull();
  });

  it("returns null for non-object payloads", () => {
    expect(parseBalanceResponse("deepseek", null)).toBeNull();
    expect(parseBalanceResponse("deepseek", "oops")).toBeNull();
  });

  it("returns null when deepseek balance_infos is missing", () => {
    expect(parseBalanceResponse("deepseek", { balance_infos: [] })).toBeNull();
  });
});

import { describe, expect, it } from "vitest";
import { getPlanColor, getPlanLabel } from "./AccountRow";

describe("getPlanColor", () => {
  it("returns gray for empty or undefined plan", () => {
    expect(getPlanColor()).toBe("#6B7280");
    expect(getPlanColor("")).toBe("#6B7280");
  });

  it("returns purple for team/ultra/advanced/premium plans", () => {
    expect(getPlanColor("team")).toBe("#7C3AED");
    expect(getPlanColor("ultra_pro")).toBe("#7C3AED");
    expect(getPlanColor("advanced")).toBe("#7C3AED");
    expect(getPlanColor("premium")).toBe("#7C3AED");
  });

  it("returns amber for pro/plus plans", () => {
    expect(getPlanColor("pro")).toBe("#F59E0B");
    expect(getPlanColor("plus_monthly")).toBe("#F59E0B");
  });

  it("returns blue for enterprise plans", () => {
    expect(getPlanColor("enterprise")).toBe("#3B82F6");
  });

  it("is case-insensitive", () => {
    expect(getPlanColor("PRO")).toBe("#F59E0B");
    expect(getPlanColor("Ultra")).toBe("#7C3AED");
  });
});

describe("getPlanLabel", () => {
  it("returns FREE for empty or undefined plan", () => {
    expect(getPlanLabel()).toBe("FREE");
    expect(getPlanLabel("")).toBe("FREE");
  });

  it("returns abbreviation for known plan types", () => {
    expect(getPlanLabel("ultra")).toBe("ULTRA");
    expect(getPlanLabel("advanced")).toBe("ADVANCED");
    expect(getPlanLabel("premium")).toBe("PREMIUM");
    expect(getPlanLabel("team_pro")).toBe("TEAM");
    expect(getPlanLabel("pro")).toBe("PRO");
    expect(getPlanLabel("plus")).toBe("PLUS");
    expect(getPlanLabel("enterprise")).toBe("ENT");
  });

  it("returns KEY for api_key plans", () => {
    expect(getPlanLabel("api_key")).toBe("KEY");
  });

  it("truncates unknown plans to 8 characters uppercase", () => {
    expect(getPlanLabel("some_very_long_plan_name")).toBe("SOME_VER");
  });

  it("handles mixed case input", () => {
    expect(getPlanLabel("Ultra")).toBe("ULTRA");
    expect(getPlanLabel("Pro_Monthly")).toBe("PRO");
  });
});

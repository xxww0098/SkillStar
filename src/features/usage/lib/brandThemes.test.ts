import { describe, expect, it } from "vitest";
import { brandChipStyle, brandHeroSurface } from "./brandThemes";

describe("brandChipStyle", () => {
  it("uses light neutral chip for near-black brands (xAI)", () => {
    const chip = brandChipStyle("111111");
    expect(chip.color).toBe("rgb(228, 228, 231)");
    expect(chip.backgroundColor).toContain("255, 255, 255");
    expect(chip.borderColor).toContain("255, 255, 255");
  });

  it("uses dark text for near-white brands", () => {
    const chip = brandChipStyle("FFFFFF");
    expect(chip.color).toBe("rgb(39, 39, 42)");
  });

  it("keeps brand-tinted chip for mid-luminance brands", () => {
    const chip = brandChipStyle("3B82F6");
    expect(chip.color).toBe("rgb(59, 130, 246)");
    expect(chip.backgroundColor).toContain("59, 130, 246");
  });
});

describe("brandHeroSurface", () => {
  it("uses light border for near-black brands so the card edge is visible", () => {
    const surface = brandHeroSurface("111111");
    expect(surface.borderColor).toContain("255, 255, 255");
  });

  it("uses brand tint for vivid brands", () => {
    const surface = brandHeroSurface("10A37F");
    expect(surface.borderColor).toContain("16, 163, 127");
  });
});

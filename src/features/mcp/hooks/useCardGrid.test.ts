import { describe, expect, it } from "vitest";
import { mcpGridColumnCount } from "./useCardGrid";

describe("mcpGridColumnCount", () => {
  it("never returns fewer than one column", () => {
    expect(mcpGridColumnCount(0, 0)).toBe(1);
    expect(mcpGridColumnCount(10, 0)).toBe(1);
    expect(mcpGridColumnCount(0, 4)).toBe(4);
  });

  it("fits as many 320px columns as the width allows", () => {
    expect(mcpGridColumnCount(336, 0)).toBe(1);
    expect(mcpGridColumnCount(671, 0)).toBe(2);
    expect(mcpGridColumnCount(1000, 0)).toBe(3);
  });

  it("holds the previous count until the container clearly shrinks past it", () => {
    // 3 columns need 3 * 336 - 16 = 992px, held down to 992 - 8 = 984.
    expect(mcpGridColumnCount(990, 3)).toBe(3);
    expect(mcpGridColumnCount(984, 3)).toBe(3);
    expect(mcpGridColumnCount(980, 3)).toBe(2);
  });

  it("grows immediately without hysteresis", () => {
    expect(mcpGridColumnCount(1010, 2)).toBe(3);
  });
});

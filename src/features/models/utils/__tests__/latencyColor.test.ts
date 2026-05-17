import { describe, expect, it } from "vitest";
import * as fc from "fast-check";
import { type LatencyColor, getLatencyColor } from "../latencyColor";

/**
 * **Validates: Requirements 7.1**
 *
 * Property 10: Latency-to-Color Mapping
 * For any latency value (including null/undefined), the color mapping function should return:
 * - gray for null/undefined (untested)
 * - green for values < 500ms
 * - yellow for 500–2000ms
 * - red for > 2000ms or error status
 * The function is total (defined for all inputs) and deterministic.
 */
describe("getLatencyColor - Property 10: Latency-to-Color Mapping", () => {
  it("null/undefined → gray", () => {
    fc.assert(
      fc.property(fc.constantFrom(null, undefined), (input) => {
        expect(getLatencyColor(input)).toBe("gray");
      }),
      { numRuns: 100 },
    );
  });

  it("any value < 500 → green", () => {
    fc.assert(
      fc.property(fc.double({ min: 0, max: 499.999, noNaN: true }), (latency) => {
        expect(getLatencyColor(latency)).toBe("green");
      }),
      { numRuns: 100 },
    );
  });

  it("any value in [500, 2000] → yellow", () => {
    fc.assert(
      fc.property(fc.double({ min: 500, max: 2000, noNaN: true }), (latency) => {
        expect(getLatencyColor(latency)).toBe("yellow");
      }),
      { numRuns: 100 },
    );
  });

  it("any value > 2000 → red", () => {
    fc.assert(
      fc.property(fc.double({ min: 2000.001, max: 1_000_000, noNaN: true }), (latency) => {
        expect(getLatencyColor(latency)).toBe("red");
      }),
      { numRuns: 100 },
    );
  });

  it("function is total and deterministic for all numeric inputs", () => {
    fc.assert(
      fc.property(fc.double({ noNaN: true, min: -1_000_000, max: 1_000_000 }), (latency) => {
        const result1 = getLatencyColor(latency);
        const result2 = getLatencyColor(latency);

        // Total: always returns a valid LatencyColor
        const validColors: LatencyColor[] = ["green", "yellow", "red", "gray"];
        expect(validColors).toContain(result1);

        // Deterministic: same input always produces same output
        expect(result1).toBe(result2);
      }),
      { numRuns: 100 },
    );
  });
});

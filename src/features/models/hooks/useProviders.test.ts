import { describe, it, expect } from "vitest";
import * as fc from "fast-check";
import { maskApiKey } from "./useProviders";

describe("maskApiKey", () => {
  /**
   * **Validates: Requirements 8.2**
   *
   * Property 17: API Key Masking
   * For any string of length ≥ 8, masked output shows at most first 3 + last 4 chars
   * with ellipsis in between. Total length is exactly 10 (3 prefix + 3 dots + 4 suffix).
   */
  it("should mask keys of length ≥ 8: first 3 + '...' + last 4", () => {
    fc.assert(
      fc.property(fc.string({ minLength: 8, maxLength: 200 }), (key) => {
        const masked = maskApiKey(key);

        // Starts with the first 3 characters of the original
        expect(masked.startsWith(key.slice(0, 3))).toBe(true);

        // Ends with the last 4 characters of the original
        expect(masked.endsWith(key.slice(-4))).toBe(true);

        // Contains "..." in the middle
        expect(masked).toContain("...");

        // Total length is exactly 10 (3 + 3 + 4)
        expect(masked.length).toBe(10);
      }),
    );
  });

  /**
   * **Validates: Requirements 8.2**
   *
   * For any string of length < 8, the masked output is "***".
   */
  it("should return '***' for keys shorter than 8 characters", () => {
    fc.assert(
      fc.property(fc.string({ minLength: 0, maxLength: 7 }), (key) => {
        const masked = maskApiKey(key);
        expect(masked).toBe("***");
      }),
    );
  });
});

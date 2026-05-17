import { describe, expect, it } from "vitest";
import * as fc from "fast-check";
import type { AppId, LatencyResult, ProviderEntry } from "../../../types";
import { type ProviderWithLatency, sortByLatency } from "./HealthDashboard";

// --- Generators ---

/** Minimal ProviderEntry sufficient for sorting tests */
function arbProviderEntry(): fc.Arbitrary<ProviderEntry> {
  return fc.record({
    id: fc.uuid(),
    name: fc.string({ minLength: 1, maxLength: 32 }),
    category: fc.constantFrom("cloud", "local", "proxy"),
    settings_config: fc.record({
      base_url: fc.constant("https://api.example.com"),
      api_key: fc.constant("sk-test"),
      models: fc.constant([{ source_model: "m1", target_model: "m2", enabled: true }]),
    }),
  });
}

function arbAppId(): fc.Arbitrary<AppId> {
  return fc.constantFrom("claude", "codex");
}

/** Generate a LatencyResult with "ok" status and a positive latency_ms */
function arbOkResult(): fc.Arbitrary<LatencyResult> {
  return fc.record({
    provider_id: fc.uuid(),
    app_id: arbAppId(),
    latency_ms: fc.integer({ min: 1, max: 30000 }),
    status: fc.constant("ok" as const),
    tested_at: fc.constant(new Date().toISOString()),
  });
}

/** Generate a LatencyResult with "timeout" or "error" status */
function arbNonOkResult(): fc.Arbitrary<LatencyResult> {
  return fc.record({
    provider_id: fc.uuid(),
    app_id: arbAppId(),
    latency_ms: fc.constant(null),
    status: fc.constantFrom("timeout" as const, "error" as const),
    error_message: fc.option(fc.string({ minLength: 1, maxLength: 50 }), { nil: undefined }),
    tested_at: fc.constant(new Date().toISOString()),
  });
}

/** Generate a LatencyResult that can be ok, timeout, or error */
function arbLatencyResult(): fc.Arbitrary<LatencyResult> {
  return fc.oneof(arbOkResult(), arbNonOkResult());
}

/** Generate a ProviderWithLatency item with optional result (undefined = no data) */
function arbProviderWithLatency(): fc.Arbitrary<ProviderWithLatency> {
  return fc.record({
    provider: arbProviderEntry(),
    appId: arbAppId(),
    result: fc.option(arbLatencyResult(), { nil: undefined }),
  });
}

// --- Property Tests ---

describe("sortByLatency - Property 13: Latency Results Sorting", () => {
  /**
   * **Validates: Requirement 5.6**
   *
   * Property 13: Latency Results Sorting
   * For any set of LatencyResult entries, after sorting:
   * - All "ok" results come before timeout/error results
   * - Within "ok" results, they are sorted ascending by latency_ms
   * - Items with no result come last
   */
  it("ok results come before timeout/error results, which come before no-data items", () => {
    fc.assert(
      fc.property(fc.array(arbProviderWithLatency(), { minLength: 0, maxLength: 30 }), (items) => {
        const sorted = sortByLatency(items);

        // Find the boundary indices
        let lastOkIndex = -1;
        let firstNonOkWithResultIndex = sorted.length;
        let lastNonOkWithResultIndex = -1;
        let firstNoDataIndex = sorted.length;

        for (let i = 0; i < sorted.length; i++) {
          const item = sorted[i];
          if (item.result?.status === "ok") {
            lastOkIndex = i;
          } else if (item.result != null) {
            // timeout or error
            if (firstNonOkWithResultIndex === sorted.length) {
              firstNonOkWithResultIndex = i;
            }
            lastNonOkWithResultIndex = i;
          } else {
            // no result
            if (firstNoDataIndex === sorted.length) {
              firstNoDataIndex = i;
            }
          }
        }

        // All ok items must come before any non-ok items with results
        if (lastOkIndex >= 0 && firstNonOkWithResultIndex < sorted.length) {
          expect(lastOkIndex).toBeLessThan(firstNonOkWithResultIndex);
        }

        // All non-ok items with results must come before no-data items
        if (lastNonOkWithResultIndex >= 0 && firstNoDataIndex < sorted.length) {
          expect(lastNonOkWithResultIndex).toBeLessThan(firstNoDataIndex);
        }

        // All ok items must come before no-data items
        if (lastOkIndex >= 0 && firstNoDataIndex < sorted.length) {
          expect(lastOkIndex).toBeLessThan(firstNoDataIndex);
        }
      }),
    );
  });

  it("ok results are sorted ascending by latency_ms", () => {
    fc.assert(
      fc.property(fc.array(arbProviderWithLatency(), { minLength: 0, maxLength: 30 }), (items) => {
        const sorted = sortByLatency(items);

        // Extract ok results in order
        const okItems = sorted.filter((item) => item.result?.status === "ok");

        // Verify ascending order by latency_ms
        for (let i = 1; i < okItems.length; i++) {
          const prevMs = okItems[i - 1].result?.latency_ms ?? 0;
          const currMs = okItems[i].result?.latency_ms ?? 0;
          expect(currMs).toBeGreaterThanOrEqual(prevMs);
        }
      }),
    );
  });

  it("sort preserves all original items (no items lost or duplicated)", () => {
    fc.assert(
      fc.property(fc.array(arbProviderWithLatency(), { minLength: 0, maxLength: 30 }), (items) => {
        const sorted = sortByLatency(items);
        expect(sorted.length).toBe(items.length);
      }),
    );
  });

  it("sort does not mutate the original array", () => {
    fc.assert(
      fc.property(fc.array(arbProviderWithLatency(), { minLength: 1, maxLength: 10 }), (items) => {
        const originalCopy = [...items];
        sortByLatency(items);
        expect(items).toEqual(originalCopy);
      }),
    );
  });
});

import { describe, expect, it } from "vitest";
import * as fc from "fast-check";
import { filterProviders } from "../filterProviders";
import type { ProviderEntryFlat } from "@/types";

/**
 * **Validates: Requirements 2.6**
 *
 * Property 3: Search Filter Correctness
 * For any list of providers and any non-empty search string, the filtered result
 * should contain exactly those providers whose name contains the search string
 * (case-insensitive). No matching provider should be excluded, and no non-matching
 * provider should be included.
 */

/** Generator for a minimal ProviderEntryFlat with a given name */
function providerWithName(name: string): ProviderEntryFlat {
  return {
    id: crypto.randomUUID(),
    name,
    base_url_openai: "https://api.example.com/v1",
    base_url_anthropic: "",
    models_url: "https://api.example.com/v1/models",
    api_key: "sk-test",
    models: ["model-1"],
    default_model: "model-1",
    sort_index: 0,
  };
}

/** Arbitrary for generating a random provider name (non-empty, printable) */
const arbProviderName = fc.string({ minLength: 1, maxLength: 30 });

/** Arbitrary for generating a list of providers with random names */
const arbProviderList = fc
  .array(arbProviderName, { minLength: 0, maxLength: 20 })
  .map((names) => names.map(providerWithName));

/** Arbitrary for a non-empty, non-whitespace-only search query */
const arbSearchQuery = fc.string({ minLength: 1, maxLength: 15 }).filter((s) => s.trim().length > 0);

describe("filterProviders - Property 3: Search Filter Correctness", () => {
  it("filtered result contains exactly providers whose name includes the query (case-insensitive)", () => {
    fc.assert(
      fc.property(arbProviderList, arbSearchQuery, (providers, query) => {
        const result = filterProviders(providers, query);
        const lowerQuery = query.toLowerCase();

        // Every result item must match
        for (const p of result) {
          expect(p.name.toLowerCase().includes(lowerQuery)).toBe(true);
        }

        // Every matching provider from the original list must be in the result
        const expectedMatches = providers.filter((p) => p.name.toLowerCase().includes(lowerQuery));
        expect(result.length).toBe(expectedMatches.length);
      }),
      { numRuns: 100 },
    );
  });

  it("no matching provider is excluded from the result", () => {
    fc.assert(
      fc.property(arbProviderList, arbSearchQuery, (providers, query) => {
        const result = filterProviders(providers, query);
        const lowerQuery = query.toLowerCase();
        const resultIds = new Set(result.map((p) => p.id));

        for (const p of providers) {
          if (p.name.toLowerCase().includes(lowerQuery)) {
            expect(resultIds.has(p.id)).toBe(true);
          }
        }
      }),
      { numRuns: 100 },
    );
  });

  it("no non-matching provider is included in the result", () => {
    fc.assert(
      fc.property(arbProviderList, arbSearchQuery, (providers, query) => {
        const result = filterProviders(providers, query);
        const lowerQuery = query.toLowerCase();

        for (const p of result) {
          expect(p.name.toLowerCase().includes(lowerQuery)).toBe(true);
        }
      }),
      { numRuns: 100 },
    );
  });

  it("empty search returns all providers", () => {
    fc.assert(
      fc.property(arbProviderList, fc.constantFrom("", " ", "  ", "\t", " \t "), (providers, emptyQuery) => {
        const result = filterProviders(providers, emptyQuery);
        expect(result.length).toBe(providers.length);
        expect(result).toEqual(providers);
      }),
      { numRuns: 100 },
    );
  });

  it("search is case-insensitive", () => {
    fc.assert(
      fc.property(arbProviderList, arbSearchQuery, (providers, query) => {
        const resultLower = filterProviders(providers, query.toLowerCase());
        const resultUpper = filterProviders(providers, query.toUpperCase());
        const resultOriginal = filterProviders(providers, query);

        // All case variants produce the same result set
        expect(resultLower.length).toBe(resultOriginal.length);
        expect(resultUpper.length).toBe(resultOriginal.length);

        const idsOriginal = resultOriginal.map((p) => p.id);
        const idsLower = resultLower.map((p) => p.id);
        const idsUpper = resultUpper.map((p) => p.id);

        expect(idsLower).toEqual(idsOriginal);
        expect(idsUpper).toEqual(idsOriginal);
      }),
      { numRuns: 100 },
    );
  });
});

import { describe, it, expect } from "vitest";
import * as fc from "fast-check";
import { getProviderToolBadges } from "../activations";
import type { ToolActivationsMap } from "../../../../types";

/**
 * **Validates: Requirements 2.2, 2.3, 5.3**
 *
 * Property 4: Tool Activation Badge Computation
 *
 * For any tool_activations map and for any provider_id,
 * `getProviderToolBadges(providerId, toolActivations)` returns exactly
 * the set of tool_ids where that provider is the active provider_id.
 */
describe("getProviderToolBadges — Property 4: Tool Activation Badge Computation", () => {
  // ── Generators ──────────────────────────────────────────────────────

  /** Generate a random tool_id (e.g., "claude-code", "codex", or arbitrary) */
  const arbToolId = fc.stringMatching(/^[a-z][a-z0-9-]{0,15}$/);

  /** Generate a random provider_id (UUID-like) */
  const arbProviderId = fc.uuid();

  /** Generate a random ToolActivation entry (provider_id + model) or null */
  const arbToolActivation = fc.oneof(
    fc.record({
      provider_id: arbProviderId,
      model: fc.stringMatching(/^[a-z][a-z0-9-]{0,20}$/),
    }),
    fc.constant(null),
  );

  /** Generate a random ToolActivationsMap with 0–10 entries */
  const arbToolActivationsMap: fc.Arbitrary<ToolActivationsMap> = fc
    .array(fc.tuple(arbToolId, arbToolActivation), { minLength: 0, maxLength: 10 })
    .map((entries) => Object.fromEntries(entries));

  // ── Property Tests ──────────────────────────────────────────────────

  it("returns exactly the tool_ids where the provider is active (random provider from map)", () => {
    fc.assert(
      fc.property(arbToolActivationsMap, arbProviderId, (toolActivations, providerId) => {
        const badges = getProviderToolBadges(providerId, toolActivations);

        // Compute expected: all tool_ids where activation.provider_id === providerId
        const expected = Object.entries(toolActivations)
          .filter(([, activation]) => activation?.provider_id === providerId)
          .map(([toolId]) => toolId);

        // Same elements (order-independent)
        expect(new Set(badges)).toEqual(new Set(expected));
        expect(badges.length).toBe(expected.length);
      }),
      { numRuns: 200 },
    );
  });

  it("returns exactly the tool_ids where the provider is active (provider picked from map values)", () => {
    fc.assert(
      fc.property(
        arbToolActivationsMap.filter((map) => {
          // Ensure at least one non-null activation exists to pick a provider from
          return Object.values(map).some((v) => v !== null);
        }),
        (toolActivations) => {
          // Pick a provider_id that actually appears in the map
          const activeProviderIds = Object.values(toolActivations)
            .filter((v): v is { provider_id: string; model: string } => v !== null)
            .map((v) => v.provider_id);

          const pickedProviderId = activeProviderIds[0];
          const badges = getProviderToolBadges(pickedProviderId, toolActivations);

          // Compute expected
          const expected = Object.entries(toolActivations)
            .filter(([, activation]) => activation?.provider_id === pickedProviderId)
            .map(([toolId]) => toolId);

          expect(new Set(badges)).toEqual(new Set(expected));
          expect(badges.length).toBe(expected.length);
        },
      ),
      { numRuns: 200 },
    );
  });

  // ── Edge Cases ──────────────────────────────────────────────────────

  it("returns empty array for an empty tool_activations map", () => {
    const badges = getProviderToolBadges("any-provider-id", {});
    expect(badges).toEqual([]);
  });

  it("returns empty array when all activations are null", () => {
    fc.assert(
      fc.property(fc.array(arbToolId, { minLength: 1, maxLength: 10 }), arbProviderId, (toolIds, providerId) => {
        const map: ToolActivationsMap = Object.fromEntries(toolIds.map((id) => [id, null]));
        const badges = getProviderToolBadges(providerId, map);
        expect(badges).toEqual([]);
      }),
      { numRuns: 100 },
    );
  });

  it("returns empty array when provider is not in any activation", () => {
    fc.assert(
      fc.property(
        arbToolActivationsMap.filter((map) => Object.keys(map).length > 0),
        (toolActivations) => {
          // Use a provider_id that definitely doesn't appear in the map
          const usedIds = new Set(
            Object.values(toolActivations)
              .filter((v): v is { provider_id: string; model: string } => v !== null)
              .map((v) => v.provider_id),
          );
          const nonExistentId = `non-existent-${Date.now()}-${Math.random()}`;
          expect(usedIds.has(nonExistentId)).toBe(false);

          const badges = getProviderToolBadges(nonExistentId, toolActivations);
          expect(badges).toEqual([]);
        },
      ),
      { numRuns: 100 },
    );
  });

  it("returns all tool_ids when provider is active for every tool", () => {
    fc.assert(
      fc.property(
        fc.array(arbToolId, { minLength: 1, maxLength: 8 }),
        arbProviderId,
        fc.stringMatching(/^[a-z][a-z0-9-]{0,20}$/),
        (toolIds, providerId, model) => {
          // Create a map where every tool points to the same provider
          const uniqueToolIds = [...new Set(toolIds)];
          const map: ToolActivationsMap = Object.fromEntries(
            uniqueToolIds.map((id) => [id, { provider_id: providerId, model }]),
          );

          const badges = getProviderToolBadges(providerId, map);
          expect(new Set(badges)).toEqual(new Set(uniqueToolIds));
          expect(badges.length).toBe(uniqueToolIds.length);
        },
      ),
      { numRuns: 100 },
    );
  });
});

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

  /** Generate a ToolBinding: 0–3 entries with an active pointer. */
  const arbBinding = fc
    .array(
      fc.record({
        provider_id: arbProviderId,
        model: fc.stringMatching(/^[a-z][a-z0-9-]{0,20}$/),
      }),
      { minLength: 0, maxLength: 3 },
    )
    .chain((entries) =>
      fc.record({
        entries: fc.constant(entries),
        active_index: entries.length > 0 ? fc.nat({ max: entries.length - 1 }) : fc.constant(0),
      }),
    );

  /** Generate a random ToolActivationsMap with 0–10 entries */
  const arbToolActivationsMap: fc.Arbitrary<ToolActivationsMap> = fc
    .array(fc.tuple(arbToolId, arbBinding), { minLength: 0, maxLength: 10 })
    .map((entries) => Object.fromEntries(entries));

  // ── Property Tests ──────────────────────────────────────────────────

  it("returns exactly the tool_ids where the provider is bound (random provider from map)", () => {
    fc.assert(
      fc.property(arbToolActivationsMap, arbProviderId, (toolActivations, providerId) => {
        const badges = getProviderToolBadges(providerId, toolActivations);

        // Expected: all tool_ids where any binding entry references providerId
        const expected = Object.entries(toolActivations)
          .filter(([, binding]) => binding?.entries.some((e) => e.provider_id === providerId))
          .map(([toolId]) => toolId);

        // Same elements (order-independent)
        expect(new Set(badges)).toEqual(new Set(expected));
        expect(badges.length).toBe(expected.length);
      }),
      { numRuns: 200 },
    );
  });

  it("returns exactly the tool_ids where the provider is bound (provider picked from map values)", () => {
    fc.assert(
      fc.property(
        arbToolActivationsMap.filter((map) => {
          // Ensure at least one entry exists to pick a provider from
          return Object.values(map).some((b) => b.entries.length > 0);
        }),
        (toolActivations) => {
          // Pick a provider_id that actually appears in the map
          const boundProviderIds = Object.values(toolActivations).flatMap((b) => b.entries.map((e) => e.provider_id));

          const pickedProviderId = boundProviderIds[0];
          const badges = getProviderToolBadges(pickedProviderId, toolActivations);

          // Compute expected
          const expected = Object.entries(toolActivations)
            .filter(([, binding]) => binding?.entries.some((e) => e.provider_id === pickedProviderId))
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

  it("returns empty array when all bindings are empty", () => {
    fc.assert(
      fc.property(fc.array(arbToolId, { minLength: 1, maxLength: 10 }), arbProviderId, (toolIds, providerId) => {
        const map: ToolActivationsMap = Object.fromEntries(toolIds.map((id) => [id, { entries: [], active_index: 0 }]));
        const badges = getProviderToolBadges(providerId, map);
        expect(badges).toEqual([]);
      }),
      { numRuns: 100 },
    );
  });

  it("returns empty array when provider is not bound in any tool", () => {
    fc.assert(
      fc.property(
        arbToolActivationsMap.filter((map) => Object.keys(map).length > 0),
        (toolActivations) => {
          // Use a provider_id that definitely doesn't appear in the map
          const usedIds = new Set(Object.values(toolActivations).flatMap((b) => b.entries.map((e) => e.provider_id)));
          const nonExistentId = `non-existent-${Date.now()}-${Math.random()}`;
          expect(usedIds.has(nonExistentId)).toBe(false);

          const badges = getProviderToolBadges(nonExistentId, toolActivations);
          expect(badges).toEqual([]);
        },
      ),
      { numRuns: 100 },
    );
  });

  it("returns all tool_ids when provider is bound for every tool", () => {
    fc.assert(
      fc.property(
        fc.array(arbToolId, { minLength: 1, maxLength: 8 }),
        arbProviderId,
        fc.stringMatching(/^[a-z][a-z0-9-]{0,20}$/),
        (toolIds, providerId, model) => {
          // Create a map where every tool binds the same provider
          const uniqueToolIds = [...new Set(toolIds)];
          const map: ToolActivationsMap = Object.fromEntries(
            uniqueToolIds.map((id) => [id, { entries: [{ provider_id: providerId, model }], active_index: 0 }]),
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

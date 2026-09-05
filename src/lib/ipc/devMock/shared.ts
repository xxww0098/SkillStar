/**
 * Shared plumbing for the sharded browser-dev IPC mock (see ./index.ts).
 *
 * Holds the fragment contract (`DevMockHandlers`), the collision-checked merge
 * used by the entry module, and the few helpers/stores consumed by more than
 * one domain fragment. Domain-specific handlers, sample data, and in-memory
 * stores live in the sibling `./<domain>.ts` / `./<domain>Data.ts` modules.
 */

/** A single mocked IPC command implementation. */
export type DevMockHandler = (args: Record<string, unknown>) => unknown;

/**
 * One domain's slice of the handler table. Every `./<domain>.ts` module exports
 * a `*_HANDLERS` const of this type; ./index.ts merges them all.
 */
export type DevMockHandlers = Record<string, DevMockHandler>;

/**
 * Merge domain fragments into the single lookup table used by `devInvoke`.
 *
 * Duplicate command keys across fragments are a bug (two domains claiming the
 * same command — plain object spread would silently keep the last one), so the
 * merge throws at module-init time. Because this module only loads in browser
 * dev, the throw surfaces immediately on first render. The same invariant is
 * also enforced statically by the cross-fragment duplicate scan in
 * ../devMockCoverage.test.ts (and biome's `noDuplicateProperties` covers
 * duplicates within a single fragment literal).
 */
export function mergeHandlerFragments(fragments: DevMockHandlers[]): DevMockHandlers {
  const merged: DevMockHandlers = {};
  for (const fragment of fragments) {
    for (const [command, handler] of Object.entries(fragment)) {
      if (command in merged) {
        throw new Error(`[devMock] duplicate handler for command "${command}" — two fragments mock the same command`);
      }
      merged[command] = handler;
    }
  }
  return merged;
}

/** ISO timestamp `daysAgo` days in the past (sample-data helper). */
export function iso(daysAgo = 0): string {
  // App code (not a workflow script) — Date is allowed here.
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  return d.toISOString();
}

/**
 * Locale-aware number formatting for the cross-view chips (Usage sessions,
 * Models route candidates), shared so neither feature imports the other.
 *
 * `formatCompactCount` uses `Intl` compact notation, so the magnitude word
 * ("K"/"M"/"万") follows the active UI locale instead of being hardcoded —
 * and no locale literal lives in this file. `formatEstimateUsd` is the
 * quiet estimate figure the Usage today line has always drawn.
 */

/** Compact count: `1.2K` / `3.4M` (zh: `1.2万`), `—` for non-finite. */
export function formatCompactCount(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return new Intl.NumberFormat(undefined, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(value);
}

/** `$x.xxxx` under a dollar, `$x.xx` above, `$0` for nothing — one quiet
 *  estimate figure (the cost is always an estimate, never a bill). */
export function formatEstimateUsd(cost: number): string {
  if (!Number.isFinite(cost) || cost === 0) return "$0";
  return cost < 1 ? `$${cost.toFixed(4)}` : `$${cost.toFixed(2)}`;
}

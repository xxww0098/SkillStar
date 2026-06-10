import type { ProviderEntryFlat } from "@/types";

/**
 * Filter providers by search query (case-insensitive name match).
 *
 * Returns all providers when the query is empty or whitespace-only.
 * Otherwise returns only providers whose name contains the query string
 * (case-insensitive comparison).
 */
export function filterProviders(providers: ProviderEntryFlat[], query: string): ProviderEntryFlat[] {
  if (!query.trim()) return providers;
  const lower = query.toLowerCase();
  return providers.filter((p) => p.name.toLowerCase().includes(lower));
}

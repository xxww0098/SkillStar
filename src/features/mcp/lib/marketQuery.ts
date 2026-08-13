import type { McpServerKind, McpServerQuery, McpServerStatus, McpSortKey } from "../../../types";

/**
 * Browse state for the MCP catalog: what the filter/sort controls hold, and
 * how it compiles into the backend's `McpServerQuery`.
 *
 * The catalog is ~21k rows, so browsing is *always* a paginated backend query —
 * every field here maps to a column the snapshot can filter or order on, and
 * nothing is re-filtered in the renderer. `buildMcpServerQuery` emits only the
 * fields that differ from the Rust-side defaults, so a default state is the
 * cheapest possible query.
 */
export interface McpMarketFilterState {
  search: string;
  kinds: McpServerKind[];
  runtimes: string[];
  licenses: string[];
  /** Include rows the registry marks `deprecated` (off by default). */
  includeDeprecated: boolean;
  /** Hide rows the registry has superseded with a newer version. */
  latestOnly: boolean;
  recommendedOnly: boolean;
  minStars: number | null;
  maxStars: number | null;
  sort: McpSortKey;
  /** `null` keeps the sort key's natural direction. */
  descending: boolean | null;
}

export const DEFAULT_MCP_PAGE_SIZE = 60;

/**
 * Default browse state.
 *
 * `includeDeprecated: false` is the deliberate part. The registry's own rule is
 * "list but warn", and the backend honours that (an empty `statuses` returns
 * deprecated rows flagged rather than dropping them) — but a browse grid that
 * mixes 243 deprecated servers in with healthy ones, each behind an equally
 * live Install button, is how a user installs an abandoned server by accident.
 * They are one toggle away, and always badged when shown.
 */
export const DEFAULT_MCP_MARKET_FILTERS: McpMarketFilterState = {
  search: "",
  kinds: [],
  runtimes: [],
  licenses: [],
  includeDeprecated: false,
  latestOnly: false,
  recommendedOnly: false,
  minStars: null,
  maxStars: null,
  sort: "default",
  descending: null,
};

export interface McpMarketQueryInput {
  filters: McpMarketFilterState;
  limit: number;
  offset: number;
  /** Scope to one publisher bucket (`"github"` = the remote registry table). */
  publisherId?: string | null;
}

/** Statuses to request for a given "include deprecated" choice. */
export function statusesFor(includeDeprecated: boolean): McpServerStatus[] {
  return includeDeprecated ? ["active", "deprecated"] : ["active"];
}

/**
 * Compile browse state into the command payload.
 *
 * Only non-default fields are emitted: every `McpServerQuery` field has a Rust
 * default, and sending `[]`/`null` for the untouched ones just makes the wire
 * payload (and the query key) noisier than the state it represents.
 */
export function buildMcpServerQuery({
  filters,
  limit,
  offset,
  publisherId,
}: McpMarketQueryInput): Partial<McpServerQuery> {
  const query: Partial<McpServerQuery> = { limit, offset };
  const search = filters.search.trim();
  if (search) query.search = search;
  if (publisherId) query.publisherId = publisherId;
  if (filters.kinds.length > 0) query.kinds = [...filters.kinds];
  if (filters.runtimes.length > 0) query.runtimes = [...filters.runtimes];
  if (filters.licenses.length > 0) query.licenses = [...filters.licenses];
  // Always explicit: the backend default is "no status filter", which *includes*
  // deprecated rows. Leaving it out would silently invert the default above.
  query.statuses = statusesFor(filters.includeDeprecated);
  if (filters.recommendedOnly) query.recommendedOnly = true;
  if (filters.latestOnly) query.latestOnly = true;
  if (filters.minStars != null) query.minStars = filters.minStars;
  if (filters.maxStars != null) query.maxStars = filters.maxStars;
  if (filters.sort !== "default") query.sort = filters.sort;
  if (filters.descending != null) query.descending = filters.descending;
  return query;
}

/** How many filter controls are away from their default, for a "clear" badge. */
export function activeMcpFilterCount(filters: McpMarketFilterState): number {
  let count = 0;
  if (filters.kinds.length > 0) count += 1;
  if (filters.runtimes.length > 0) count += 1;
  if (filters.licenses.length > 0) count += 1;
  if (filters.includeDeprecated) count += 1;
  if (filters.latestOnly) count += 1;
  if (filters.recommendedOnly) count += 1;
  if (filters.minStars != null || filters.maxStars != null) count += 1;
  return count;
}

/** Whether anything at all narrows the catalog (search included). */
export function hasActiveMcpNarrowing(filters: McpMarketFilterState): boolean {
  return filters.search.trim().length > 0 || activeMcpFilterCount(filters) > 0;
}

/** Add/remove one value of a multi-select filter, preserving order. */
export function toggleFilterValue<T>(values: readonly T[], value: T): T[] {
  return values.includes(value) ? values.filter((item) => item !== value) : [...values, value];
}

export interface McpPageWindow {
  /** 1-based index of the first row on this page; 0 when the page is empty. */
  from: number;
  /** 1-based index of the last row on this page; 0 when the page is empty. */
  to: number;
  total: number;
  pageIndex: number;
  pageCount: number;
  hasPrev: boolean;
  hasNext: boolean;
}

/**
 * Describe the current page for the "showing 1–60 of 21363" line.
 *
 * `total` is the pre-pagination match count the backend returns alongside the
 * page, so this needs no second round trip and stays correct while filters
 * change under it.
 */
export function mcpPageWindow(total: number, offset: number, limit: number, itemCount: number): McpPageWindow {
  const safeLimit = Math.max(1, limit);
  const safeOffset = Math.max(0, offset);
  const pageCount = total > 0 ? Math.ceil(total / safeLimit) : 0;
  return {
    from: itemCount > 0 ? safeOffset + 1 : 0,
    to: itemCount > 0 ? safeOffset + itemCount : 0,
    total,
    pageIndex: Math.floor(safeOffset / safeLimit),
    pageCount,
    hasPrev: safeOffset > 0,
    hasNext: safeOffset + itemCount < total,
  };
}

/**
 * Keep the offset inside the result set after `total` shrinks (a filter got
 * narrower while the user was on page 40). Snaps to the last page rather than
 * to 0 so the view does not jump home on every keystroke.
 */
export function clampMcpOffset(offset: number, total: number, limit: number): number {
  const safeLimit = Math.max(1, limit);
  if (total <= 0) return 0;
  const lastPageOffset = Math.max(0, Math.floor((total - 1) / safeLimit) * safeLimit);
  return Math.min(Math.max(0, offset), lastPageOffset);
}

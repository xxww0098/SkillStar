import type { McpSourceDescriptor, SyncStateEntry } from "../../../types";

/**
 * Catalog freshness, per source.
 *
 * A merged catalog has a failure mode a single-source one does not: a sync
 * where three of four sources succeeded is *reported as a success* and looks
 * complete, while the fourth source's servers are simply absent. The backend
 * records that as a per-source `mcp_registry:<id>` scope carrying its own
 * `lastError` and `degradedReason`; this module turns those rows into the
 * sentence the UI owes the user — "what you are looking at is not the whole
 * catalog, and here is why."
 */
export type McpSourceHealth = "fresh" | "degraded" | "stale" | "error" | "never";

const SOURCE_SCOPE_PREFIX = "mcp_registry:";

/** `mcp_registry:github` → `github`; the aggregate scope → `null`. */
export function parseMcpSourceScope(scope: string): string | null {
  return scope.startsWith(SOURCE_SCOPE_PREFIX) ? scope.slice(SOURCE_SCOPE_PREFIX.length) : null;
}

/**
 * Classify one sync-state row.
 *
 * Order matters and encodes severity of *what the user is looking at*, not of
 * the last request: a source that has never succeeded has no rows at all, an
 * errored one has rows plus a failing refresh, and a degraded one has rows it
 * already knows are incomplete. Only a source with data, no error and a refresh
 * still in the future is `fresh`.
 */
export function classifyMcpSourceHealth(entry: SyncStateEntry, now: number = Date.now()): McpSourceHealth {
  if (!entry.last_success_at) return entry.last_error ? "error" : "never";
  if (entry.last_error) return "error";
  if (entry.degraded_reason) return "degraded";
  const next = entry.next_refresh_at ? Date.parse(entry.next_refresh_at) : Number.NaN;
  if (!Number.isNaN(next) && next <= now) return "stale";
  return "fresh";
}

export interface McpSourceStatus {
  scope: string;
  /** Source id when this row belongs to one source; `null` for the aggregate. */
  sourceId: string | null;
  /** The descriptor, when the source is still configured. */
  descriptor: McpSourceDescriptor | null;
  health: McpSourceHealth;
  lastSuccessAt: string | null;
  lastAttemptAt: string | null;
  lastError: string | null;
  degradedReason: string | null;
  sourceHost: string | null;
}

/**
 * Join per-source sync states with the configured sources.
 *
 * A source can have state without a descriptor (the user removed it after a
 * sync) and a descriptor without state (just added, never synced). Both are
 * kept: dropping the first hides rows that are still in the catalog, dropping
 * the second hides a source the user is waiting on.
 */
export function buildMcpSourceStatuses(
  states: readonly SyncStateEntry[],
  sources: readonly McpSourceDescriptor[],
  now: number = Date.now(),
): McpSourceStatus[] {
  const byId = new Map(sources.map((source) => [source.id, source]));
  const seen = new Set<string>();

  const fromStates = states
    .filter((entry) => entry.scope.startsWith(SOURCE_SCOPE_PREFIX))
    .map((entry) => {
      const sourceId = parseMcpSourceScope(entry.scope);
      if (sourceId) seen.add(sourceId);
      return {
        scope: entry.scope,
        sourceId,
        descriptor: sourceId ? (byId.get(sourceId) ?? null) : null,
        health: classifyMcpSourceHealth(entry, now),
        lastSuccessAt: entry.last_success_at,
        lastAttemptAt: entry.last_attempt_at,
        lastError: entry.last_error,
        degradedReason: entry.degraded_reason ?? null,
        sourceHost: entry.source_host ?? null,
      } satisfies McpSourceStatus;
    });

  const neverSynced = sources
    .filter((source) => !seen.has(source.id))
    .map(
      (source) =>
        ({
          scope: `${SOURCE_SCOPE_PREFIX}${source.id}`,
          sourceId: source.id,
          descriptor: source,
          health: "never",
          lastSuccessAt: null,
          lastAttemptAt: null,
          lastError: null,
          degradedReason: null,
          sourceHost: null,
        }) satisfies McpSourceStatus,
    );

  return [...fromStates, ...neverSynced];
}

export interface McpCatalogHealth {
  /** True when at least one *enabled* source cannot vouch for its rows. */
  incomplete: boolean;
  /** One line per reason, ready to render verbatim. */
  reasons: Array<{ sourceId: string; label: string; health: McpSourceHealth; detail: string }>;
  /** Most recent successful sync across all sources. */
  lastSuccessAt: string | null;
  freshCount: number;
  enabledCount: number;
}

/**
 * Roll per-source statuses up into the banner model.
 *
 * Disabled sources are excluded on purpose: a source the user switched off is
 * not a gap in the catalog, and counting it would make the warning permanent
 * and therefore ignorable. A status with no descriptor is treated as enabled —
 * its rows are in the catalog either way.
 */
export function summarizeMcpCatalogHealth(statuses: readonly McpSourceStatus[]): McpCatalogHealth {
  const enabled = statuses.filter((status) => status.descriptor?.enabled !== false);
  const reasons = enabled
    .filter((status) => status.health !== "fresh")
    .map((status) => ({
      sourceId: status.sourceId ?? status.scope,
      label: status.descriptor?.displayName ?? status.sourceId ?? status.scope,
      health: status.health,
      detail: status.degradedReason ?? status.lastError ?? "",
    }));

  const successTimes = enabled
    .map((status) => status.lastSuccessAt)
    .filter((value): value is string => value != null)
    .sort();

  return {
    incomplete: reasons.some((reason) => reason.health === "degraded" || reason.health === "error"),
    reasons,
    lastSuccessAt: successTimes.length > 0 ? successTimes[successTimes.length - 1] : null,
    freshCount: enabled.filter((status) => status.health === "fresh").length,
    enabledCount: enabled.length,
  };
}

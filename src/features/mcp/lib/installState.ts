import type { McpMarketEntry, McpServerEntry } from "../../../types";

/**
 * The three-state catalog badge: is this card already installed, is the
 * installed copy behind the catalog, and is the row itself deprecated.
 *
 * The old rule was `installedNames.has(entry.name)` — a bare string compare
 * against the *sanitized config key*, which collides across publishers
 * (`github` from two different registries) and misses any entry the user
 * renamed. Installed entries now carry a source fingerprint
 * (`registryName` / `installedVersion` / `sourceId` / `runtimeKind`), so the
 * match is by identity.
 */
export type McpCatalogState = "notInstalled" | "installed" | "updateAvailable";

export interface McpEntryStatus {
  state: McpCatalogState;
  /** The installed entry this card resolves to, if any. */
  installed: McpServerEntry | null;
  installedVersion: string | null;
  /** Catalog version, when the row publishes one. */
  latestVersion: string | null;
  /** The registry marks this server deprecated. */
  deprecated: boolean;
  /** The registry knows a newer version of this row (`isLatest === false`). */
  superseded: boolean;
  /** True when the match came from the legacy name compare, not a fingerprint. */
  matchedByName: boolean;
}

const norm = (value: string | null | undefined): string => (value ?? "").trim().toLowerCase();

/**
 * Index installed servers for repeated card lookups.
 *
 * Two maps, in priority order:
 * - `byRegistryName` — `McpServerEntry.registryName` is the registry's own
 *   reverse-DNS `server.json` name, which is exactly `McpMarketEntry.namespace`.
 *   This is the identity match.
 * - `byName` — the legacy fallback for entries created before fingerprints
 *   existed (or by hand / by import). It only ever answers when no fingerprint
 *   matched, and never carries enough provenance to claim "update available".
 */
export interface McpInstalledIndex {
  byRegistryName: Map<string, McpServerEntry>;
  byName: Map<string, McpServerEntry>;
}

export function buildInstalledIndex(servers: readonly McpServerEntry[]): McpInstalledIndex {
  const byRegistryName = new Map<string, McpServerEntry>();
  const byName = new Map<string, McpServerEntry>();
  for (const server of servers) {
    const registryName = norm(server.registryName);
    if (registryName && !byRegistryName.has(registryName)) byRegistryName.set(registryName, server);
    const name = norm(server.name);
    if (name && !byName.has(name)) byName.set(name, server);
  }
  return { byRegistryName, byName };
}

/**
 * Compare two version strings.
 *
 * Returns a negative number when `a < b`, positive when `a > b`, `0` when they
 * are equal, and `null` when the two cannot be meaningfully ordered (different
 * shapes, non-numeric segments that are not plain prerelease tags). Registry
 * versions are conventionally semver but nothing enforces it, so "I cannot
 * tell" is a real answer and callers must handle it rather than get a
 * confidently wrong ordering.
 */
export function compareMcpVersions(a: string, b: string): number | null {
  const parse = (value: string): { release: number[]; pre: string | null } | null => {
    const trimmed = value.trim().replace(/^v/i, "");
    if (!trimmed) return null;
    const [core, ...rest] = trimmed.split("-");
    const segments = core.split(".");
    if (segments.length === 0 || segments.some((segment) => !/^\d+$/.test(segment))) return null;
    return { release: segments.map(Number), pre: rest.length > 0 ? rest.join("-") : null };
  };

  const left = parse(a);
  const right = parse(b);
  if (!left || !right) return null;

  const length = Math.max(left.release.length, right.release.length);
  for (let i = 0; i < length; i += 1) {
    const diff = (left.release[i] ?? 0) - (right.release[i] ?? 0);
    if (diff !== 0) return diff < 0 ? -1 : 1;
  }
  // Equal releases: a prerelease sorts *before* its own release (semver §11).
  if (left.pre === right.pre) return 0;
  if (left.pre === null) return 1;
  if (right.pre === null) return -1;
  return left.pre < right.pre ? -1 : 1;
}

/**
 * Resolve one catalog card against the installed store.
 *
 * "Update available" is deliberately conservative: it needs a fingerprint
 * match, a version on both sides, and either a strictly greater catalog version
 * or — when the two versions are not comparable — a catalog row the registry
 * still considers current (`isLatest`). A name-only match never reports an
 * update, because the version it would compare against was never recorded from
 * this row.
 */
export function resolveMcpEntryStatus(entry: McpMarketEntry, index: McpInstalledIndex): McpEntryStatus {
  const deprecated = entry.status === "deprecated";
  const superseded = entry.isLatest === false;
  const latestVersion = entry.version?.trim() || null;

  const byFingerprint = index.byRegistryName.get(norm(entry.namespace)) ?? null;
  const installed = byFingerprint ?? index.byName.get(norm(entry.name)) ?? null;

  if (!installed) {
    return {
      state: "notInstalled",
      installed: null,
      installedVersion: null,
      latestVersion,
      deprecated,
      superseded,
      matchedByName: false,
    };
  }

  const matchedByName = byFingerprint == null;
  const installedVersion = installed.installedVersion?.trim() || null;
  const base = {
    installed,
    installedVersion,
    latestVersion,
    deprecated,
    superseded,
    matchedByName,
  };

  if (matchedByName || !installedVersion || !latestVersion || installedVersion === latestVersion) {
    return { state: "installed", ...base };
  }

  const comparison = compareMcpVersions(latestVersion, installedVersion);
  const newer = comparison == null ? entry.isLatest : comparison > 0;
  return { state: newer ? "updateAvailable" : "installed", ...base };
}

/**
 * Latest catalog version for an installed entry, from a catalog row that was
 * already fetched for it. Returns `null` when the row is not the same server.
 */
export function latestVersionForInstalled(
  server: McpServerEntry,
  candidates: readonly McpMarketEntry[],
): McpMarketEntry | null {
  const registryName = norm(server.registryName);
  if (!registryName) return null;
  return candidates.find((entry) => norm(entry.namespace) === registryName) ?? null;
}

/** Whether an installed entry is behind the catalog row it was installed from. */
export function installedHasUpdate(server: McpServerEntry, entry: McpMarketEntry | null): boolean {
  if (!entry) return false;
  const installedVersion = server.installedVersion?.trim();
  const latestVersion = entry.version?.trim();
  if (!installedVersion || !latestVersion || installedVersion === latestVersion) return false;
  const comparison = compareMcpVersions(latestVersion, installedVersion);
  return comparison == null ? entry.isLatest : comparison > 0;
}

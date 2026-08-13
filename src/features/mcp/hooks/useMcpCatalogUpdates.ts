import { useQueries } from "@tanstack/react-query";
import { useMemo } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { LocalFirstResult, McpMarketEntry, McpServerEntry, McpServerPage } from "../../../types";
import { mcpKeys } from "../api/keys";
import { installedHasUpdate, latestVersionForInstalled } from "../lib/installState";

const UPDATE_STALE_TIME_MS = 5 * 60_000;

/**
 * Cap on how many installed servers get an update check per render.
 *
 * Each check is one IPC round trip, and a store with hundreds of entries would
 * otherwise fan out to hundreds of concurrent queries on page load. The cap is
 * reported back so the UI can say the list was truncated rather than implying
 * the unchecked entries are up to date.
 */
const MAX_UPDATE_CHECKS = 60;

export interface McpUpdateInfo {
  entry: McpMarketEntry;
  latestVersion: string | null;
  hasUpdate: boolean;
}

/**
 * "Has a newer version" for installed servers.
 *
 * Only entries carrying a `registryName` fingerprint are checked: without it
 * there is no provenance to compare against, and guessing by config key is the
 * exact mistake the fingerprint was added to fix. The lookup asks the catalog
 * for the *latest* row of that reverse-DNS name (`latestOnly`, one row), so a
 * superseded row can never be mistaken for the newest one.
 */
export function useMcpCatalogUpdates(servers: readonly McpServerEntry[], enabled = true) {
  const checkable = useMemo(
    () => servers.filter((server) => (server.registryName ?? "").trim().length > 0).slice(0, MAX_UPDATE_CHECKS),
    [servers],
  );

  const results = useQueries({
    queries: checkable.map((server) => {
      const registryName = (server.registryName ?? "").trim();
      const query = { search: registryName, latestOnly: true, limit: 5, offset: 0 };
      return {
        queryKey: mcpKeys.marketLatest(registryName),
        queryFn: () =>
          tauriInvoke("query_mcp_market_servers_local", { query }) as Promise<LocalFirstResult<McpServerPage>>,
        enabled,
        staleTime: UPDATE_STALE_TIME_MS,
      };
    }),
  });

  const byServerId = useMemo(() => {
    const map = new Map<string, McpUpdateInfo>();
    checkable.forEach((server, index) => {
      const items = results[index]?.data?.data.items ?? [];
      const entry = latestVersionForInstalled(server, items);
      if (!entry) return;
      map.set(server.id, {
        entry,
        latestVersion: entry.version?.trim() || null,
        hasUpdate: installedHasUpdate(server, entry),
      });
    });
    return map;
  }, [checkable, results]);

  return {
    byServerId,
    updateCount: [...byServerId.values()].filter((info) => info.hasUpdate).length,
    /** Servers skipped by the cap — never counted as "up to date". */
    uncheckedCount: Math.max(0, servers.filter((s) => (s.registryName ?? "").trim()).length - checkable.length),
    isLoading: results.some((result) => result.isLoading),
  };
}

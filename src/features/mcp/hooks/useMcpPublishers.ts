import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpPublisherSummary } from "../../../types";
import { mcpKeys } from "../api/keys";

const STALE_TIME_MS = 5 * 60_000;

/**
 * Official MCP publishers for the marketplace grid. Curated publishers
 * (AdsPower / BigModel) are always seeded, so this returns instantly; GitHub's
 * server count reflects whatever the registry sync has cached so far.
 */
export function useMcpPublishers(enabled = true) {
  const query = useQuery<McpPublisherSummary[]>({
    queryKey: mcpKeys.publishers(),
    queryFn: () => tauriInvoke("list_mcp_publishers_local"),
    enabled,
    staleTime: STALE_TIME_MS,
  });

  return {
    publishers: query.data ?? [],
    isLoading: query.isLoading,
    isError: query.isError,
  };
}

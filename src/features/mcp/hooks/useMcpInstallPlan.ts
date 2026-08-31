import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpInstallPlan } from "../../../types";
import { mcpKeys } from "../api/keys";

const PLAN_STALE_TIME_MS = 60_000;

/**
 * The pre-install confirmation payload for one catalog entry.
 *
 * Re-fetched whenever the user picks a different runtime shape, because the
 * shape decides the transport, the command, the resolved binary and *which*
 * inputs the form must collect — a remote endpoint asks for headers, an npm
 * package asks for env vars. Reusing the previous plan's fields against a new
 * shape would ask for the wrong things and then confirm a command that is not
 * the one about to run.
 */
export function useMcpInstallPlan(serverId: string | null, runtimeId: string | null) {
  const query = useQuery<McpInstallPlan>({
    queryKey: mcpKeys.installPlan(serverId, runtimeId),
    queryFn: () =>
      tauriInvoke("mcp_market_install_plan", {
        id: serverId as string,
        ...(runtimeId ? { runtimeId } : {}),
      }),
    enabled: serverId != null,
    staleTime: PLAN_STALE_TIME_MS,
  });

  return {
    plan: query.data ?? null,
    isLoading: query.isLoading,
    isFetching: query.isFetching,
    error: query.error ?? null,
  };
}

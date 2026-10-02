/**
 * Cross-view route reads (spec slice 13): one model ref's candidates in
 * `route_smart` order with the allowance percents, and the catalog-scoped
 * "which agents route here" join the Usage quota card jumps to. All local
 * reads over the ledger, the account book, and the provider store.
 */
import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import type { RouteComparison } from "../../../types/generated/RouteComparison";
import { modelsKeys } from "./keys";

/** One model ref's candidates compared over the same ledger scope. The
 *  query is disabled for an empty ref so the hub renders nothing until an
 *  agent is in focus. */
export function useRouteComparison(modelRef: string | null) {
  const ref = modelRef?.trim() || null;
  return useQuery<RouteComparison>({
    queryKey: modelsKeys.routeComparison(ref ?? ""),
    queryFn: () => tauriInvoke("get_route_comparison", { modelRef: ref ?? "" }),
    enabled: ref !== null,
    staleTime: 30_000,
  });
}

/** One row of the serving join: an agent whose current model's candidates
 *  include the catalog, with the matched candidate for the chip. */
export interface ServingAgent {
  agentId: string;
  agentName: string;
  modelRef: string;
  candidateIndex: number;
  comparison: RouteComparison;
}

/**
 * The catalog leg of the triangle: which agents' current models resolve to
 * candidates attributed to `catalogId`. One local comparison read per agent
 * with a model (a handful of rows, each a disk read), joined client-side —
 * no new backend truth.
 */
export function useServingAgents(
  catalogId: string | null,
  agents: { id: string; name: string; model_label?: string | null }[],
) {
  // The join reads the board's agents; the ids-and-models signature rides
  // the key so a board that lands after the focus request re-runs the join
  // instead of answering with the empty list it started from.
  const signature = agents.map((agent) => `${agent.id}:${agent.model_label ?? ""}`).join("|");
  return useQuery<ServingAgent[]>({
    queryKey: [...modelsKeys.servingAgents(catalogId ?? ""), signature],
    enabled: catalogId !== null,
    staleTime: 30_000,
    queryFn: async () => {
      if (!catalogId) return [];
      const withModels = agents.filter((agent) => (agent.model_label ?? "").trim().length > 0);
      const comparisons = await Promise.all(
        withModels.map((agent) => tauriInvoke("get_route_comparison", { modelRef: agent.model_label as string })),
      );
      const rows: ServingAgent[] = [];
      comparisons.forEach((comparison, index) => {
        const candidateIndex = comparison.candidates.findIndex((candidate) => candidate.catalog === catalogId);
        if (candidateIndex === -1) return;
        rows.push({
          agentId: withModels[index].id,
          agentName: withModels[index].name,
          modelRef: withModels[index].model_label as string,
          candidateIndex,
          comparison,
        });
      });
      return rows;
    },
  });
}

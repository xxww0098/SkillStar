/**
 * Routing and affinity for the selected provider and saved groups.
 * The query fails closed: the column draws nothing from an error string.
 */
import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import type { RoutingPage } from "../../../lib/ipc/commands/models";
import { modelsKeys } from "./keys";

export function useRoutingPage(providerId: string | null) {
  const id = providerId ?? "";
  return useQuery<RoutingPage>({
    queryKey: modelsKeys.routingPage(id),
    queryFn: () => tauriInvoke("get_routing_page", { providerId: id }),
  });
}

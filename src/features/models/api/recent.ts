/**
 * Gateway column: the in-memory ring of recent calls.
 * Failures stay in the query result. The column still draws its headers.
 */
import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import type { RecentCallDto } from "../../../types/generated/RecentCallDto";
import { modelsKeys } from "./keys";

export function useRecentCalls() {
  return useQuery<RecentCallDto[]>({
    queryKey: modelsKeys.recentCalls(),
    queryFn: () => tauriInvoke("get_recent_calls"),
  });
}

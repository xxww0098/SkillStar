/**
 * Gateway column: the in-memory ring of recent calls.
 * Failures stay in the query result. An empty success is the column's empty sentence.
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

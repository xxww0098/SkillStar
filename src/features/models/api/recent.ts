/**
 * Gateway column: recent calls from the persistent usage ledger's newest
 * page, merged with the in-memory ring. The rows survive a restart and
 * carry in-tokens, session, and latency alongside the completion count;
 * an absent count stays an empty string, never a printed zero.
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

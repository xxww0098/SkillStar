import { useMutation } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpProbeReport } from "../../../types";

export interface McpProbeEntry {
  report: McpProbeReport | null;
  /** Transport-level failure — the command itself did not return a report. */
  error: string | null;
  pending: boolean;
}

const EMPTY: McpProbeEntry = { report: null, error: null, pending: false };

/**
 * On-demand health checks, one server at a time.
 *
 * Explicitly *not* a `useQuery`: a probe starts a process or opens a socket to
 * a third-party endpoint, so it runs when the user asks and never on mount,
 * refocus or cache expiry.
 *
 * `probe_mcp_server` does not reject for an unhealthy server — an unreachable
 * one, a missing launcher, and a remote server answering `401` with a
 * `WWW-Authenticate` challenge all come back as a *report* with a status. The
 * 401 in particular is the server correctly asking for authorization, not a
 * failure, and the UI must not paint it red. Only a genuine command error
 * (the server id is gone, the store cannot be read) lands in `error`.
 */
export function useMcpProbe() {
  const [entries, setEntries] = useState<Record<string, McpProbeEntry>>({});

  const mutation = useMutation({
    mutationFn: (id: string) => tauriInvoke("probe_mcp_server", { id }),
    onMutate: (id) => {
      setEntries((prev) => ({ ...prev, [id]: { report: prev[id]?.report ?? null, error: null, pending: true } }));
    },
    onSuccess: (report, id) => {
      setEntries((prev) => ({ ...prev, [id]: { report, error: null, pending: false } }));
    },
    onError: (error, id) => {
      setEntries((prev) => ({
        ...prev,
        [id]: {
          report: prev[id]?.report ?? null,
          error: error instanceof Error ? error.message : String(error),
          pending: false,
        },
      }));
    },
  });

  const probe = useCallback(
    async (id: string) => {
      try {
        return await mutation.mutateAsync(id);
      } catch {
        // Already recorded on the entry; callers read `entryFor`.
        return null;
      }
    },
    [mutation],
  );

  return {
    probe,
    entryFor: useCallback((id: string): McpProbeEntry => entries[id] ?? EMPTY, [entries]),
    clear: useCallback((id: string) => setEntries((prev) => ({ ...prev, [id]: EMPTY })), []),
  };
}

import { useMutation } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpProbeReport } from "../../../types";

export interface McpProbeEntry {
  report: McpProbeReport | null;
  /** Transport-level failure — the command itself did not return a report. */
  error: string | null;
  pending: boolean;
}

const EMPTY: McpProbeEntry = { report: null, error: null, pending: false };

/** Visible fleet only — never the whole 2k-server catalog, never on focus. */
export const MCP_FLEET_PROBE_CAP = 8;

/**
 * On-demand health checks, one server at a time.
 *
 * Explicitly *not* a `useQuery`: a probe starts a process or opens a socket to
 * a third-party endpoint, so the per-server `probe` runs when the user asks
 * and never on mount, refocus or cache expiry.
 *
 * The command-center fleet may call `probeFleet` once for the first
 * {@link MCP_FLEET_PROBE_CAP} installed ids. That is sequential, capped, and
 * still never bound to window focus.
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

  const probeRef = useRef(probe);
  probeRef.current = probe;

  const probeFleet = useCallback(async (ids: readonly string[]) => {
    const unique: string[] = [];
    for (const id of ids) {
      if (id && !unique.includes(id)) unique.push(id);
      if (unique.length >= MCP_FLEET_PROBE_CAP) break;
    }
    for (const id of unique) {
      await probeRef.current(id);
    }
  }, []);

  return {
    probe,
    probeFleet,
    entryFor: useCallback((id: string): McpProbeEntry => entries[id] ?? EMPTY, [entries]),
    clear: useCallback((id: string) => setEntries((prev) => ({ ...prev, [id]: EMPTY })), []),
  };
}

/**
 * Probe the visible fleet once per distinct capped id list. Not on focus.
 */
export function useMcpFleetProbe(serverIds: readonly string[], probeFleet: (ids: readonly string[]) => Promise<void>) {
  const key = serverIds.slice(0, MCP_FLEET_PROBE_CAP).join("\0");
  const startedFor = useRef<string | null>(null);
  useEffect(() => {
    if (!key) return;
    if (startedFor.current === key) return;
    startedFor.current = key;
    void probeFleet(key.split("\0"));
  }, [key, probeFleet]);
}

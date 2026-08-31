import { useEffect, useRef, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpInstallAnswer, McpInstallPreview } from "../../../types";

const PREVIEW_DEBOUNCE_MS = 300;

/**
 * The entry and command line one set of answers produces.
 *
 * Deliberately **not** a TanStack query. The answers carry the user's secrets,
 * and a query key is a cache key: caching them would keep every keystroke's
 * secret in memory for the rest of the session, and in a key that anything can
 * enumerate. A debounced effect plus a stale-response guard is the whole
 * mechanism instead.
 *
 * The debounce is also a cost control, not only a UX one: the command reaches
 * the catalog detail read path, which still runs a curated-seed write
 * transaction ahead of every read.
 */
export function useMcpInstallPreview(
  serverId: string | null,
  runtimeId: string | null,
  answers: readonly McpInstallAnswer[],
  /**
   * Bump to re-derive from the catalog row as it stands *now*, with the same
   * answers. The one caller that needs it is a submit refused because the row
   * changed under the user: without a re-read the command on screen would still
   * be the old one, and re-approving it would be refused again forever.
   */
  refreshToken = 0,
) {
  const [preview, setPreview] = useState<McpInstallPreview | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const [pending, setPending] = useState(false);
  // Monotonic request counter: a slow reply for older answers must never
  // overwrite a newer one, or the user approves a command they already edited
  // past.
  const latest = useRef(0);

  useEffect(() => {
    if (serverId == null) {
      setPreview(null);
      setPending(false);
      return;
    }

    // Pending from the first keystroke, not from the request: the displayed
    // command is already stale during the debounce window.
    setPending(true);
    const request = ++latest.current;
    const timer = setTimeout(() => {
      tauriInvoke("mcp_market_install_preview", {
        id: serverId,
        ...(runtimeId ? { runtimeId } : {}),
        answers: [...answers],
      })
        .then((next) => {
          if (request !== latest.current) return;
          setPreview(next);
          setError(null);
        })
        .catch((cause: unknown) => {
          if (request !== latest.current) return;
          setPreview(null);
          setError(cause instanceof Error ? cause : new Error(String(cause)));
        })
        .finally(() => {
          if (request === latest.current) setPending(false);
        });
    }, PREVIEW_DEBOUNCE_MS);

    return () => clearTimeout(timer);
  }, [serverId, runtimeId, answers, refreshToken]);

  return { preview, pending, error };
}

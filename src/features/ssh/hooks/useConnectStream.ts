import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

/** A single line in the connection console (mirrors the Rust SshProgressEvent). */
export interface SshProgressLine {
  sessionId: string;
  phase: string;
  status: "start" | "ok" | "warn" | "fail" | "pending";
  message: string;
  tsMs: number;
  detail?: unknown;
}

/** Host-key confirmation pending against the active session, if any. */
export interface PendingHostKey {
  fingerprint: string;
  /** Set on a mismatch — the saved fingerprint differs from the server's. */
  expected?: string;
}

/**
 * Subscribe to the `ssh://connect-stream` Tauri event channel and expose the
 * running connection log.
 *
 * Pass a `hostKey` (the host's id or `system:<alias>`) to scope events to a
 * host: when it changes the log resets. Within a host we show the most recent
 * session's events (the app only drives one SSH op at a time per panel).
 *
 * Outside Tauri (browser dev) the channel never fires, so the hook is inert —
 * callers can seed an artificial log there if they want a demo.
 */
export function useConnectStream(hostKey: string | null) {
  const [lines, setLines] = useState<SshProgressLine[]>([]);
  const [pendingHostKey, setPendingHostKey] = useState<PendingHostKey | null>(null);
  const activeHost = useRef<string | null>(null);

  useEffect(() => {
    // Reset the console when the selected host changes.
    if (activeHost.current !== hostKey) {
      activeHost.current = hostKey;
      setLines([]);
      setPendingHostKey(null);
    }
    if (!hostKey || !isTauri()) return;

    let unlisten: (() => void) | undefined;
    listen<SshProgressLine>("ssh://connect-stream", (event) => {
      const line = event.payload;
      setLines((prev) => [...prev.slice(-200), line]); // keep last 200 lines
      if (line.phase === "host_key" && line.status === "pending") {
        const detail = (line.detail ?? {}) as { fingerprint?: string };
        setPendingHostKey({ fingerprint: detail.fingerprint ?? "" });
      } else if (line.phase === "host_key") {
        setPendingHostKey(null);
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [hostKey]);

  return { lines, pendingHostKey };
}

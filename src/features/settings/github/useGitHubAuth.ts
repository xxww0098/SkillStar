import { useCallback, useEffect, useRef, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type {
  GitHubAuthError,
  GitHubConnectionStatus,
  GitHubDeviceAuthorization,
  GitHubDeviceFlowPoll,
} from "../../../types";

function normalizeError(error: unknown): GitHubAuthError {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    return {
      code: String(error.code) as GitHubAuthError["code"],
      message: String(error.message),
    };
  }
  return {
    code: "protocol",
    message: error instanceof Error ? error.message : "GitHub authentication failed",
  };
}

export function useGitHubAuth() {
  const [status, setStatus] = useState<GitHubConnectionStatus | null>(null);
  const [authorization, setAuthorization] = useState<GitHubDeviceAuthorization | null>(null);
  const [flow, setFlow] = useState<GitHubDeviceFlowPoll | null>(null);
  const [error, setError] = useState<GitHubAuthError | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [lastFailure, setLastFailure] = useState<"load" | "start" | "poll" | "refresh" | "logout" | null>(null);
  const attemptRef = useRef(0);

  const loadStatus = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setStatus(await tauriInvoke("github_auth_status"));
      setLastFailure(null);
    } catch (cause) {
      setError(normalizeError(cause));
      setLastFailure("load");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadStatus();
    return () => {
      attemptRef.current += 1;
    };
  }, [loadStatus]);

  const start = useCallback(async () => {
    const attempt = attemptRef.current + 1;
    attemptRef.current = attempt;
    setBusy(true);
    setError(null);
    setFlow(null);
    try {
      const next = await tauriInvoke("github_auth_start");
      if (attemptRef.current === attempt) {
        setAuthorization(next);
        setLastFailure(null);
      }
    } catch (cause) {
      if (attemptRef.current === attempt) {
        setError(normalizeError(cause));
        setLastFailure("start");
      }
    } finally {
      if (attemptRef.current === attempt) setBusy(false);
    }
  }, []);

  const pollNow = useCallback(async () => {
    const attempt = attemptRef.current;
    if (!authorization) return;
    setError(null);
    try {
      const outcome = await tauriInvoke("github_auth_poll");
      if (attemptRef.current !== attempt) return;
      setFlow(outcome);
      setLastFailure(null);
      if (outcome.state === "connected") {
        setStatus(outcome.connection);
        setAuthorization(null);
      } else if (outcome.state === "denied" || outcome.state === "expired") {
        setAuthorization(null);
      }
    } catch (cause) {
      if (attemptRef.current === attempt) {
        setError(normalizeError(cause));
        setLastFailure("poll");
      }
    }
  }, [authorization]);

  useEffect(() => {
    if (!authorization) return;
    if (flow && flow.state !== "pending" && flow.state !== "slow_down") return;
    const delaySeconds =
      flow?.state === "pending" || flow?.state === "slow_down"
        ? flow.retry_after_seconds
        : authorization.interval_seconds;
    const timer = window.setTimeout(() => void pollNow(), delaySeconds * 1_000);
    return () => window.clearTimeout(timer);
  }, [authorization, flow, pollNow]);

  const cancel = useCallback(async () => {
    attemptRef.current += 1;
    setAuthorization(null);
    setFlow(null);
    setError(null);
    setLastFailure(null);
    await tauriInvoke("github_auth_cancel").catch(() => false);
  }, []);

  const refresh = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await tauriInvoke("github_auth_refresh"));
      setLastFailure(null);
    } catch (cause) {
      setError(normalizeError(cause));
      setLastFailure("refresh");
    } finally {
      setBusy(false);
    }
  }, []);

  const logout = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await tauriInvoke("github_auth_logout");
      attemptRef.current += 1;
      setAuthorization(null);
      setFlow(null);
      setStatus({ state: "signed_out" });
      setLastFailure(null);
    } catch (cause) {
      setError(normalizeError(cause));
      setLastFailure("logout");
    } finally {
      setBusy(false);
    }
  }, []);

  const retry = useCallback(async () => {
    if (lastFailure === "load") return loadStatus();
    if (lastFailure === "poll") return pollNow();
    if (lastFailure === "refresh") return refresh();
    if (lastFailure === "logout") return logout();
    return start();
  }, [lastFailure, loadStatus, logout, pollNow, refresh, start]);

  return {
    status,
    authorization,
    flow,
    error,
    loading,
    busy,
    start,
    pollNow,
    cancel,
    refresh,
    logout,
    retry,
    reload: loadStatus,
  };
}

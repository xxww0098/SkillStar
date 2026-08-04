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
  const attemptRef = useRef(0);

  const loadStatus = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setStatus(await tauriInvoke("github_auth_status"));
    } catch (cause) {
      setError(normalizeError(cause));
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
      if (attemptRef.current === attempt) setAuthorization(next);
    } catch (cause) {
      if (attemptRef.current === attempt) setError(normalizeError(cause));
    } finally {
      if (attemptRef.current === attempt) setBusy(false);
    }
  }, []);

  const pollNow = useCallback(async () => {
    const attempt = attemptRef.current;
    if (!authorization) return;
    try {
      const outcome = await tauriInvoke("github_auth_poll");
      if (attemptRef.current !== attempt) return;
      setFlow(outcome);
      if (outcome.state === "connected") {
        setStatus(outcome.connection);
        setAuthorization(null);
      } else if (outcome.state === "denied" || outcome.state === "expired") {
        setAuthorization(null);
      }
    } catch (cause) {
      if (attemptRef.current === attempt) setError(normalizeError(cause));
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
    await tauriInvoke("github_auth_cancel").catch(() => false);
  }, []);

  const refresh = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await tauriInvoke("github_auth_refresh"));
    } catch (cause) {
      setError(normalizeError(cause));
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
    } catch (cause) {
      setError(normalizeError(cause));
    } finally {
      setBusy(false);
    }
  }, []);

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
    retry: start,
    reload: loadStatus,
  };
}

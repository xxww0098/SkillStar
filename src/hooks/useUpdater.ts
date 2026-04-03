import { useState, useEffect, useRef, useCallback } from "react";
import { useTranslation } from "react-i18next";
import type { Update } from "@tauri-apps/plugin-updater";

const UPDATER_ERROR_PATTERN = /could not fetch a valid release json/i;

export type UpdateStatus = "idle" | "checking" | "available" | "downloading" | "ready" | "error";

export interface UpdateState {
  status: UpdateStatus;
  version: string;
  progress: number;
  error: string;
  /** How many automatic retries remain before giving up. */
  retriesLeft: number;
}

const SKIPPED_KEY = "skillstar_skipped_version";
const LAST_CHECK_KEY = "skillstar_last_check";
const CHECK_INTERVAL_MS = 60 * 60 * 1000; // 1h
const CHECK_TIMEOUT_MS = 12_000;           // 12s for release JSON fetch
const DOWNLOAD_TIMEOUT_MS = 5 * 60_000;    // 5min for binary download
const MAX_DOWNLOAD_RETRIES = 2;

function getSkipped(): string {
  return localStorage.getItem(SKIPPED_KEY) ?? "";
}

function getLastCheck(): number {
  return Number(localStorage.getItem(LAST_CHECK_KEY)) || 0;
}

/** Race a promise against a timeout. */
function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`${label} timed out after ${ms / 1000}s`)), ms);
    promise.then(
      (v) => { clearTimeout(timer); resolve(v); },
      (e) => { clearTimeout(timer); reject(e); },
    );
  });
}

export function useUpdater() {
  const { t } = useTranslation();
  const [state, setState] = useState<UpdateState>({
    status: "idle",
    version: "",
    progress: 0,
    error: "",
    retriesLeft: MAX_DOWNLOAD_RETRIES,
  });

  const mapUpdaterError = useCallback((e: unknown): string => {
    const msg = e instanceof Error ? e.message : String(e);
    if (UPDATER_ERROR_PATTERN.test(msg)) {
      return t("sidebar.updateErrorFetchRelease");
    }
    return msg;
  }, [t]);

  const candidateRef = useRef<Update | null>(null);
  const checkingRef = useRef(false);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ── Check ─────────────────────────────────────────────────────────
  const check = useCallback(async () => {
    if (checkingRef.current) return;
    checkingRef.current = true;

    try {
      setState((s) => ({ ...s, status: "checking", error: "" }));

      const { check: checkUpdate } = await import("@tauri-apps/plugin-updater");
      const update = await withTimeout(checkUpdate(), CHECK_TIMEOUT_MS, "Update check");

      if (!update) {
        setState((s) => ({ ...s, status: "idle", version: "", progress: 0, error: "" }));
        localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
        return;
      }

      if (update.version === getSkipped()) {
        setState((s) => ({ ...s, status: "idle", version: "", progress: 0, error: "" }));
        localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
        return;
      }

      candidateRef.current = update;
      setState({
        status: "available",
        version: update.version,
        progress: 0,
        error: "",
        retriesLeft: MAX_DOWNLOAD_RETRIES,
      });
      localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
    } catch (e) {
      setState((s) => ({
        ...s,
        status: "error",
        version: "",
        progress: 0,
        error: mapUpdaterError(e),
      }));
    } finally {
      checkingRef.current = false;
    }
  }, [mapUpdaterError]);

  // ── Download ──────────────────────────────────────────────────────
  const download = useCallback(async () => {
    const candidate = candidateRef.current;
    if (!candidate) return;

    try {
      setState((s) => ({ ...s, status: "downloading", progress: 0, error: "" }));

      let downloaded = 0;
      let contentLength = 0;

      const downloadPromise = candidate.download((event) => {
        if (event.event === "Started") {
          contentLength = event.data.contentLength ?? 0;
          setState((s) => ({ ...s, progress: 0 }));
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          const pct =
            contentLength > 0
              ? Math.min(100, Math.round((downloaded / contentLength) * 100))
              : Math.min(95, downloaded > 0 ? Math.round(Math.log2(downloaded / 1024)) : 1);
          setState((s) => ({ ...s, progress: pct }));
        }
      });

      await withTimeout(downloadPromise, DOWNLOAD_TIMEOUT_MS, "Download");

      setState((s) => ({ ...s, status: "ready", progress: 100 }));
    } catch (e) {
      setState((prev) => {
        const retriesLeft = prev.retriesLeft - 1;
        if (retriesLeft > 0) {
          // Schedule automatic retry
          retryTimerRef.current = setTimeout(() => {
            download();
          }, 3000); // retry after 3s
          return {
            ...prev,
            status: "downloading",
            progress: 0,
            error: "",
            retriesLeft,
          };
        }
        return {
          ...prev,
          status: "error",
          progress: 0,
          error: mapUpdaterError(e),
          retriesLeft: 0,
        };
      });
    }
  }, [mapUpdaterError]);

  // ── Apply (install + relaunch) ────────────────────────────────────
  const apply = useCallback(async () => {
    const candidate = candidateRef.current;
    if (!candidate) return;
    try {
      await candidate.install();
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (e) {
      setState((s) => ({
        ...s,
        status: "error",
        error: mapUpdaterError(e),
      }));
    }
  }, [mapUpdaterError]);

  // ── Skip this version ─────────────────────────────────────────────
  const skip = useCallback(() => {
    if (state.version) {
      localStorage.setItem(SKIPPED_KEY, state.version);
    }
    setState({ status: "idle", version: "", progress: 0, error: "", retriesLeft: MAX_DOWNLOAD_RETRIES });
    candidateRef.current = null;
    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
  }, [state.version]);

  // ── Dismiss error ─────────────────────────────────────────────────
  const dismiss = useCallback(() => {
    setState({ status: "idle", version: "", progress: 0, error: "", retriesLeft: MAX_DOWNLOAD_RETRIES });
    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
  }, []);

  // ── Retry (re-check + re-download if candidate lost) ─────────────
  const retry = useCallback(async () => {
    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
    setState((s) => ({ ...s, retriesLeft: MAX_DOWNLOAD_RETRIES }));
    if (candidateRef.current) {
      await download();
    } else {
      await check();
    }
  }, [check, download]);

  // ── Auto-check on mount + periodic ────────────────────────────────
  useEffect(() => {
    const lastCheck = getLastCheck();
    const elapsed = Date.now() - lastCheck;
    const firstDelay = elapsed >= CHECK_INTERVAL_MS ? 500 : (CHECK_INTERVAL_MS - elapsed);

    const firstTimer = setTimeout(() => {
      check();
    }, firstDelay);

    const interval = setInterval(check, CHECK_INTERVAL_MS);

    return () => {
      clearTimeout(firstTimer);
      clearInterval(interval);
    };
  }, [check]);

  // Cleanup retry timer on unmount
  useEffect(() => {
    return () => {
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
      }
    };
  }, []);

  return { state, check, download, apply, skip, dismiss, retry };
}

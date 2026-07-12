import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openExternalUrl } from "../lib/externalOpen";
import { tauriInvoke } from "../lib/ipc";

/**
 * How the app handles updates.
 *
 * - `"github-release"` (active, 0.0.3+): check GitHub Releases API via
 *   `check_app_update`, then open the release page for the user to download.
 *   No signing keys required. See docs/backend.md § Auto-Update.
 * - `"tauri-plugin"`: full signed in-app install (needs
 *   `createUpdaterArtifacts` + `TAURI_SIGNING_PRIVATE_KEY` + published
 *   `latest.json`). Not enabled yet.
 * - `"disabled"`: no check, no UI action (legacy kill switch).
 */
export type UpdaterMode = "github-release" | "tauri-plugin" | "disabled";

export const UPDATER_MODE: UpdaterMode = "github-release";

/** True when the About "Check for Updates" button and auto-check are live. */
export const UPDATER_ENABLED = UPDATER_MODE !== "disabled";

export type UpdateStatus = "idle" | "checking" | "available" | "downloading" | "ready" | "error";

export interface UpdateState {
  status: UpdateStatus;
  version: string;
  progress: number;
  error: string;
  /** How many automatic retries remain before giving up (tauri-plugin only). */
  retriesLeft: number;
  /** GitHub release page URL for github-release mode. */
  releaseUrl: string;
}

const SKIPPED_KEY = "skillstar_skipped_version";
const LAST_CHECK_KEY = "skillstar_last_check";
const CHECK_INTERVAL_MS = 60 * 60 * 1000; // 1h
const CHECK_TIMEOUT_MS = 20_000; // 20s (mirror may add latency)
const MAX_DOWNLOAD_RETRIES = 2;
const FALLBACK_RELEASES_URL = "https://github.com/xxww0098/SkillStar/releases/latest";

const IDLE_STATE: UpdateState = {
  status: "idle",
  version: "",
  progress: 0,
  error: "",
  retriesLeft: MAX_DOWNLOAD_RETRIES,
  releaseUrl: "",
};

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
      (v) => {
        clearTimeout(timer);
        resolve(v);
      },
      (e) => {
        clearTimeout(timer);
        reject(e);
      },
    );
  });
}

export function useUpdater() {
  const { t } = useTranslation();
  const [state, setState] = useState<UpdateState>(IDLE_STATE);

  const checkingRef = useRef(false);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const releaseUrlRef = useRef("");

  const mapUpdaterError = useCallback(
    (e: unknown): string => {
      const msg = e instanceof Error ? e.message : String(e);
      if (/could not fetch|update check failed|timed out|network|dns|connection/i.test(msg)) {
        return t("sidebar.updateErrorFetchRelease");
      }
      return msg;
    },
    [t],
  );

  // ── Check (GitHub Releases API via Rust) ──────────────────────────
  const check = useCallback(async (): Promise<{ found: boolean; version?: string; error?: boolean }> => {
    if (!UPDATER_ENABLED) return { found: false };
    if (checkingRef.current) return { found: false };
    checkingRef.current = true;

    try {
      setState((s) => ({ ...s, status: "checking", error: "" }));

      const result = await withTimeout(tauriInvoke("check_app_update"), CHECK_TIMEOUT_MS, "Update check");

      if (!result.available || !result.version) {
        setState((s) => ({
          ...s,
          status: "idle",
          version: "",
          progress: 0,
          error: "",
          releaseUrl: result.release_url ?? "",
        }));
        localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
        return { found: false };
      }

      if (result.version === getSkipped()) {
        setState((s) => ({
          ...s,
          status: "idle",
          version: "",
          progress: 0,
          error: "",
          releaseUrl: result.release_url ?? "",
        }));
        localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
        return { found: false };
      }

      const releaseUrl = result.release_url?.trim() || FALLBACK_RELEASES_URL;
      releaseUrlRef.current = releaseUrl;

      setState({
        status: "available",
        version: result.version,
        progress: 0,
        error: "",
        retriesLeft: MAX_DOWNLOAD_RETRIES,
        releaseUrl,
      });
      localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
      return { found: true, version: result.version };
    } catch (e) {
      setState((s) => ({
        ...s,
        status: "error",
        version: "",
        progress: 0,
        error: mapUpdaterError(e),
        releaseUrl: "",
      }));
      return { found: false, error: true };
    } finally {
      checkingRef.current = false;
    }
  }, [mapUpdaterError]);

  // ── Primary action: open release page OR in-app install ───────────
  const download = useCallback(async () => {
    if (!UPDATER_ENABLED) return;

    // Active path: open GitHub release page for manual install.
    if (UPDATER_MODE === "github-release") {
      const url = releaseUrlRef.current || state.releaseUrl || FALLBACK_RELEASES_URL;
      const ok = await openExternalUrl(url);
      if (!ok) {
        setState((s) => ({
          ...s,
          status: "error",
          error: t("sidebar.updateErrorOpenRelease"),
        }));
      }
      return;
    }

    // Deferred path: tauri-plugin signed install.
    if (UPDATER_MODE !== "tauri-plugin") return;

    try {
      setState((s) => ({ ...s, status: "downloading", progress: 0, error: "" }));

      let downloaded = 0;
      let contentLength = 0;

      const { listen } = await import("@tauri-apps/api/event");
      const unlisten = await listen<{ chunk_length: number; content_length: number | null }>(
        "updater://download-progress",
        (event) => {
          if (event.payload.content_length) {
            contentLength = event.payload.content_length;
          }
          downloaded += event.payload.chunk_length;
          const pct =
            contentLength > 0
              ? Math.min(100, Math.round((downloaded / contentLength) * 100))
              : Math.min(95, downloaded > 0 ? Math.round(Math.log2(downloaded / 1024)) : 1);
          setState((s) => ({ ...s, progress: pct }));
        },
      );

      try {
        await tauriInvoke("download_and_install_update");
        setState((s) => ({ ...s, status: "ready", progress: 100 }));
      } finally {
        unlisten();
      }
    } catch (e) {
      setState((prev) => {
        const retriesLeft = prev.retriesLeft - 1;
        if (retriesLeft > 0) {
          retryTimerRef.current = setTimeout(async () => {
            try {
              const res = await tauriInvoke("check_app_update");
              if (res.available) {
                download();
              } else {
                setState((s) => ({ ...s, ...IDLE_STATE }));
              }
            } catch {
              setState((s) => ({ ...s, status: "error", progress: 0, error: mapUpdaterError(e) }));
            }
          }, 3000);
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
  }, [mapUpdaterError, state.releaseUrl, t]);

  // ── Apply (restart) — tauri-plugin only ───────────────────────────
  const apply = useCallback(async () => {
    try {
      await tauriInvoke("restart_after_update");
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
    setState({ ...IDLE_STATE });
    releaseUrlRef.current = "";
    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
  }, [state.version]);

  // ── Dismiss error ─────────────────────────────────────────────────
  const dismiss = useCallback(() => {
    setState({ ...IDLE_STATE });
    releaseUrlRef.current = "";
    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
  }, []);

  // ── Retry (re-check) ──────────────────────────────────────────────
  const retry = useCallback(async () => {
    if (retryTimerRef.current) {
      clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
    }
    setState((s) => ({ ...s, retriesLeft: MAX_DOWNLOAD_RETRIES }));
    await check();
  }, [check]);

  // ── Auto-check on mount + periodic ────────────────────────────────
  useEffect(() => {
    if (!UPDATER_ENABLED) return;
    const lastCheck = getLastCheck();
    const elapsed = Date.now() - lastCheck;
    const firstDelay = elapsed >= CHECK_INTERVAL_MS ? 500 : CHECK_INTERVAL_MS - elapsed;

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

import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { tauriInvokeDynamic } from "../lib/ipc";
import type { AiStreamPayload } from "../types";
import { getAiConfigCached } from "./useAiConfig";

/**
 * Shared hook for AI streaming operations (summarize).
 * Consolidates the streaming logic for AI summarization.
 */

interface AiStreamState {
  content: string | null;
  visible: boolean;
  loading: boolean;
  hasDelta: boolean;
  wasNonStreaming: boolean;
  error: string | null;
  source: string | null;
}

const INITIAL_STATE: AiStreamState = {
  content: null,
  visible: false,
  loading: false,
  hasDelta: false,
  wasNonStreaming: false,
  error: null,
  source: null,
};

interface UseAiStreamOptions {
  /** Tauri invoke command name, e.g. "ai_summarize_skill_stream" */
  command: string;
  /** Event channel, e.g. "ai://summarize-stream" */
  eventChannel: string;
  /** Optional: transform the final result before storing */
  normalizeResult?: (source: string, result: string) => string;
  /**
   * Optional: parse a non-string invoke result into text.
   * If not provided, the invoke result is used as-is (assumed string).
   */
  parseInvokeResult?: (raw: unknown) => { text: string };
}

interface ExecuteAiStreamOptions {
  /** Bypass cache/read-toggle logic and force a fresh backend request. */
  forceRefresh?: boolean;
  /**
   * Keep currently visible content while refreshing, until new deltas/final
   * result arrive.
   */
  keepVisibleWhileLoading?: boolean;
  /** Additional params to pass to the Tauri invoke call. */
  extraInvokeParams?: Record<string, unknown>;
}

/** Default safety timeout for summarize streams. */
const SAFETY_TIMEOUT_MS = 60_000;

export function useAiStream({ command, eventChannel, normalizeResult, parseInvokeResult }: UseAiStreamOptions) {
  const [state, setState] = useState<AiStreamState>(INITIAL_STATE);
  const [aiConfigured, setAiConfigured] = useState(false);

  const activeIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);
  // Mirror state in a ref so execute can read latest values without
  // being re-created on every state change (avoids stale closures).
  const stateRef = useRef(state);
  stateRef.current = state;
  // Guard async setState after component unmount
  const mountedRef = useRef(true);

  // Load AI readiness on mount.
  useEffect(() => {
    (async () => {
      try {
        const config = await getAiConfigCached();
        if (!mountedRef.current) return;
        setAiConfigured(config.enabled && (config.provider_ref != null || config.api_format === "local"));
      } catch {
        if (mountedRef.current) setAiConfigured(false);
      }
    })();
  }, []);

  const cancel = useCallback(() => {
    activeIdRef.current = null;
    if (unlistenRef.current) {
      unlistenRef.current();
      unlistenRef.current = null;
    }
    setState((prev) => ({ ...prev, loading: false }));
  }, []);

  const dismiss = useCallback(() => {
    setState((prev) => ({ ...prev, visible: false }));
  }, []);

  const execute = useCallback(
    async (sourceContent: string, options: ExecuteAiStreamOptions = {}): Promise<string | null> => {
      if (!aiConfigured) return null;
      const forceRefresh = options.forceRefresh ?? false;
      const keepVisibleWhileLoading = options.keepVisibleWhileLoading ?? false;
      const extraInvokeParams = options.extraInvokeParams ?? {};

      const snap = stateRef.current;

      if (!forceRefresh) {
        if (snap.loading) {
          if (snap.source === sourceContent) {
            // Already loading the same thing, treat as toggle cancel
            cancel();
            if (!snap.content) {
              setState((prev) => ({ ...prev, visible: false }));
            }
            return null;
          } else {
            // Loading something else, cancel it and proceed to load the new one
            cancel();
          }
        } else {
          // If visible, toggle off only if the source content matches
          if (snap.visible && snap.source === sourceContent) {
            dismiss();
            return null;
          }

          // If cached result matches source, show it
          if (snap.content && snap.source === sourceContent) {
            setState((prev) => ({ ...prev, visible: true }));
            return snap.content;
          }
        }
      } else if (snap.loading) {
        // Force refresh while loading, cancel old and proceed
        cancel();
      }

      // Start new request
      const requestId =
        typeof crypto !== "undefined" && "randomUUID" in crypto
          ? crypto.randomUUID()
          : `ai-${Date.now()}-${Math.random().toString(16).slice(2)}`;
      activeIdRef.current = requestId;
      let streamedRaw = "";
      let deltaCount = 0;
      let rafId: number | null = null;
      let requestUnlisten: (() => void) | null = null;
      let safetyTimer: ReturnType<typeof setTimeout> | undefined;

      const cleanupRequestUnlisten = () => {
        if (!requestUnlisten) return;
        requestUnlisten();
        if (unlistenRef.current === requestUnlisten) {
          unlistenRef.current = null;
        }
        requestUnlisten = null;
      };

      const clearSafetyTimer = () => {
        if (safetyTimer !== undefined) {
          clearTimeout(safetyTimer);
          safetyTimer = undefined;
        }
      };

      const armSafetyTimer = () => {
        clearSafetyTimer();
        safetyTimer = setTimeout(() => {
          if (activeIdRef.current !== requestId) return;
          activeIdRef.current = null;
          cleanupRequestUnlisten();
          setState({
            content: streamedRaw.trim() ? streamedRaw : null,
            visible: !!streamedRaw.trim(),
            loading: false,
            hasDelta: deltaCount >= 2,
            wasNonStreaming: false,
            error: "Stream timed out",
            source: sourceContent,
          });
        }, SAFETY_TIMEOUT_MS);
      };

      setState((prev) => ({
        content: keepVisibleWhileLoading ? prev.content : null,
        visible: keepVisibleWhileLoading ? prev.visible : false,
        loading: true,
        hasDelta: false,
        wasNonStreaming: false,
        error: null,
        source: keepVisibleWhileLoading ? prev.source : sourceContent,
      }));

      try {
        const flushDelta = () => {
          rafId = null;
          // NOTE: We intentionally do NOT call setState here.
          // Delta updates are accumulated internally (streamedRaw) so the final
          // result is available immediately on completion, but the UI does NOT
          // re-render per-delta — the completed result is rendered all at
          // once when the backend finishes.
          // If the backend hangs, the safety timer recovers with streamedRaw.
        };

        const unlisten = await listen<AiStreamPayload>(eventChannel, (event) => {
          if (activeIdRef.current !== requestId) return;
          const payload = event.payload;
          if (payload.requestId !== requestId) return;

          if (payload.event === "delta" && payload.delta) {
            if (payload.delta === "\0CLEAR\0") {
              streamedRaw = "";
              deltaCount = 0;
              armSafetyTimer();
              if (rafId == null) {
                rafId = requestAnimationFrame(flushDelta);
              }
              return;
            }
            deltaCount += 1;
            streamedRaw += payload.delta;
            armSafetyTimer();
            if (rafId == null) {
              rafId = requestAnimationFrame(flushDelta);
            }
            return;
          }

          if (payload.event === "start" && payload.message) {
            armSafetyTimer();
            return;
          }

          if (payload.event === "error" && payload.message) {
            setState((prev) => ({ ...prev, error: String(payload.message) }));
          }
        });
        requestUnlisten = unlisten;
        unlistenRef.current = unlisten;
        if (activeIdRef.current !== requestId) {
          cleanupRequestUnlisten();
          return null;
        }

        // Safety timeout — if the backend hangs beyond this, force-recover the UI.
        armSafetyTimer();

        const invokePayload = {
          requestId,
          content: sourceContent,
          ...(forceRefresh ? { forceRefresh: true } : {}),
          ...extraInvokeParams,
        };
        const rawResult = await tauriInvokeDynamic(command, invokePayload);

        if (activeIdRef.current !== requestId) return null;

        let finalText: string;
        if (parseInvokeResult) {
          const parsed = parseInvokeResult(rawResult);
          finalText = parsed.text;
        } else {
          finalText = rawResult as string;
        }
        if (normalizeResult) {
          finalText = normalizeResult(sourceContent, finalText);
        }
        setState({
          content: finalText,
          visible: true,
          loading: false,
          hasDelta: deltaCount >= 2,
          wasNonStreaming: deltaCount < 2,
          error: null,
          source: sourceContent,
        });
        return finalText;
      } catch (e) {
        if (activeIdRef.current !== requestId) return null;
        setState({
          content: streamedRaw.trim() ? streamedRaw : null,
          visible: !!streamedRaw.trim(),
          loading: false,
          hasDelta: deltaCount >= 2,
          wasNonStreaming: false,
          error: String(e),
          source: null,
        });
        return null;
      } finally {
        clearSafetyTimer();
        if (rafId != null) {
          cancelAnimationFrame(rafId);
          rafId = null;
        }
        cleanupRequestUnlisten();
        if (activeIdRef.current === requestId) {
          activeIdRef.current = null;
        }
      }
    },
    [aiConfigured, cancel, dismiss, command, eventChannel, normalizeResult, parseInvokeResult],
  );

  // Cleanup on unmount
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      activeIdRef.current = null;
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, []);

  const hydrate = useCallback((content: string | null, source: string | null) => {
    setState((prev) => ({ ...prev, content, source }));
  }, []);

  const setVisible = useCallback((visible: boolean) => {
    setState((prev) => ({ ...prev, visible }));
  }, []);

  const setContent = useCallback((content: string | null) => {
    setState((prev) => ({ ...prev, content }));
  }, []);

  const setError = useCallback((error: string | null) => {
    setState((prev) => ({ ...prev, error }));
  }, []);

  return {
    ...state,
    aiConfigured,
    execute,
    cancel,
    dismiss,
    hydrate,
    setVisible,
    setContent,
    setError,
  };
}

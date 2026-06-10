import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { tauriInvoke } from "../lib/ipc";
import type { AiTranslateMetrics, AiTranslatePipelineProgress, AiTranslateStreamPayload } from "../types";
import { getAiConfigCached } from "./useAiConfig";

/**
 * Hook for invoking AST-based SKILL.md translation with progress events.
 *
 * Streams `ai://translate-stream` events from the backend (start →
 * progress* → complete | error), exposing the latest pipeline phase to the
 * `TranslationWaitBanner`. The final translated content is the resolved value
 * of the invoke call.
 *
 * Caching note: the backend owns durable content-hash caching per target_lang
 * + model. Frontend state only caches the active panel/session.
 */

interface TranslateState {
  translated: string | null;
  /** When true, preview should show the translated content instead of the original. */
  showTranslated: boolean;
  loading: boolean;
  error: string | null;
  pipelineProgress: AiTranslatePipelineProgress | null;
  metrics: AiTranslateMetrics | null;
  /** Content the translation was produced from — used to invalidate on edit. */
  source: string | null;
  /** Elapsed seconds since the current invocation began. */
  elapsedSec: number;
}

const INITIAL: TranslateState = {
  translated: null,
  showTranslated: false,
  loading: false,
  error: null,
  pipelineProgress: null,
  metrics: null,
  source: null,
  elapsedSec: 0,
};

/** Safety budget — wall clock cap before the UI force-recovers. */
export const TRANSLATE_BUDGET_MS = 180_000;

interface TranslateOptions {
  forceRefresh?: boolean;
}

export function useAiTranslate() {
  const [state, setState] = useState<TranslateState>(INITIAL);
  const [aiConfigured, setAiConfigured] = useState(false);
  const activeIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);
  const tickRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const startedAtRef = useRef<number>(0);
  const completionMetricsRef = useRef<AiTranslateMetrics | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      activeIdRef.current = null;
      if (unlistenRef.current) unlistenRef.current();
      unlistenRef.current = null;
      if (tickRef.current) clearInterval(tickRef.current);
      tickRef.current = null;
    };
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const cfg = await getAiConfigCached();
        if (!mountedRef.current) return;
        setAiConfigured(cfg.enabled && (cfg.provider_ref != null || cfg.api_format === "local"));
      } catch {
        if (mountedRef.current) setAiConfigured(false);
      }
    })();
  }, []);

  const stopTicker = () => {
    if (tickRef.current) {
      clearInterval(tickRef.current);
      tickRef.current = null;
    }
  };

  const startTicker = () => {
    stopTicker();
    startedAtRef.current = Date.now();
    setState((p) => ({ ...p, elapsedSec: 0 }));
    tickRef.current = setInterval(() => {
      setState((p) => ({ ...p, elapsedSec: Math.floor((Date.now() - startedAtRef.current) / 1000) }));
    }, 1000);
  };

  const cancel = useCallback(() => {
    activeIdRef.current = null;
    completionMetricsRef.current = null;
    if (unlistenRef.current) unlistenRef.current();
    unlistenRef.current = null;
    stopTicker();
    setState((p) => ({ ...p, loading: false }));
  }, []);

  const toggleShow = useCallback(() => {
    setState((p) => (p.translated == null ? p : { ...p, showTranslated: !p.showTranslated }));
  }, []);

  const setError = useCallback((error: string | null) => {
    setState((p) => ({ ...p, error }));
  }, []);

  const translate = useCallback(
    async (content: string, options: TranslateOptions = {}): Promise<string | null> => {
      if (!aiConfigured) return null;
      const forceRefresh = options.forceRefresh ?? false;

      // If we already have a translation for THIS content, just toggle the view.
      if (!forceRefresh && !state.loading && state.translated != null && state.source === content) {
        setState((p) => ({ ...p, showTranslated: !p.showTranslated }));
        return state.translated;
      }

      // Already running — second click cancels.
      if (state.loading) {
        cancel();
        return null;
      }

      const requestId =
        typeof crypto !== "undefined" && "randomUUID" in crypto
          ? crypto.randomUUID()
          : `tr-${Date.now()}-${Math.random().toString(16).slice(2)}`;
      activeIdRef.current = requestId;

      let safetyTimer: ReturnType<typeof setTimeout> | undefined;
      const armSafety = () => {
        if (safetyTimer !== undefined) clearTimeout(safetyTimer);
        safetyTimer = setTimeout(() => {
          if (activeIdRef.current !== requestId) return;
          activeIdRef.current = null;
          completionMetricsRef.current = null;
          if (unlistenRef.current) unlistenRef.current();
          unlistenRef.current = null;
          stopTicker();
          setState((p) => ({ ...p, loading: false, error: "Translation timed out" }));
        }, TRANSLATE_BUDGET_MS);
      };

      setState({
        translated: null,
        showTranslated: false,
        loading: true,
        error: null,
        pipelineProgress: null,
        metrics: null,
        source: content,
        elapsedSec: 0,
      });
      completionMetricsRef.current = null;
      startTicker();

      // This request's own subscription. The shared unlistenRef can be
      // overwritten by a newer request while our invoke is still in flight,
      // so cleanup below must only ever detach THIS listener.
      let myUnlisten: (() => void) | null = null;

      try {
        const unlisten = await listen<AiTranslateStreamPayload>("ai://translate-stream", (event) => {
          if (activeIdRef.current !== requestId) return;
          const payload = event.payload;
          if (payload.requestId !== requestId) return;

          if (payload.event === "progress" && payload.pipelineProgress) {
            armSafety();
            setState((p) => ({ ...p, pipelineProgress: payload.pipelineProgress ?? null }));
          } else if (payload.event === "error" && payload.message) {
            setState((p) => ({ ...p, error: String(payload.message) }));
          } else if (payload.event === "start") {
            armSafety();
          } else if (payload.event === "complete") {
            completionMetricsRef.current = payload.metrics ?? null;
            setState((p) => ({ ...p, metrics: payload.metrics ?? null }));
          }
        });
        if (activeIdRef.current !== requestId) {
          // Cancelled while the subscription was being created — don't touch
          // unlistenRef, a newer request may already own it.
          unlisten();
          return null;
        }
        myUnlisten = unlisten;
        unlistenRef.current = unlisten;

        armSafety();

        const result = await tauriInvoke("ai_translate_skill_stream", {
          requestId,
          content,
          ...(forceRefresh ? { forceRefresh: true } : {}),
        });
        const metrics = result.metrics ?? completionMetricsRef.current;

        if (activeIdRef.current !== requestId) return null;
        setState({
          translated: result.content,
          showTranslated: true,
          loading: false,
          error: null,
          pipelineProgress: null,
          metrics,
          source: content,
          elapsedSec: Math.floor((Date.now() - startedAtRef.current) / 1000),
        });
        return result.content;
      } catch (e) {
        if (activeIdRef.current !== requestId) return null;
        setState((p) => ({
          ...p,
          loading: false,
          error: String(e),
        }));
        return null;
      } finally {
        if (safetyTimer !== undefined) clearTimeout(safetyTimer);
        myUnlisten?.();
        if (unlistenRef.current === myUnlisten) unlistenRef.current = null;
        if (activeIdRef.current === requestId) {
          // Only tear down shared state while we are still the active
          // request — a newer request owns the ticker otherwise.
          activeIdRef.current = null;
          stopTicker();
        }
      }
    },
    [aiConfigured, cancel, state.loading, state.source, state.translated],
  );

  /** Reset translated content (e.g. when user edits the source). */
  const reset = useCallback(() => {
    setState(INITIAL);
  }, []);

  return {
    ...state,
    aiConfigured,
    translate,
    cancel,
    toggleShow,
    reset,
    setError,
  };
}

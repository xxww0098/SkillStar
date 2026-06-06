import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { tauriInvoke } from "../lib/ipc";
import type { AiTranslatePipelineProgress, AiTranslateStreamPayload } from "../types";
import { getAiConfigCached } from "./useAiConfig";

/**
 * Hook for invoking AST-based SKILL.md translation with progress events.
 *
 * Streams `ai://translate-stream` events from the backend (start →
 * progress* → complete | error), exposing the latest pipeline phase to the
 * `TranslationWaitBanner`. The final translated content is the resolved value
 * of the invoke call.
 *
 * Caching note: the backend keeps a 1-hour content-hash cache per
 * target_lang + model, so re-translating the same SKILL.md within a session
 * returns near-instantly.
 */

interface TranslateState {
  translated: string | null;
  /** When true, preview should show the translated content instead of the original. */
  showTranslated: boolean;
  loading: boolean;
  error: string | null;
  pipelineProgress: AiTranslatePipelineProgress | null;
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
  source: null,
  elapsedSec: 0,
};

/** Safety budget — wall clock cap before the UI force-recovers. */
export const TRANSLATE_BUDGET_MS = 180_000;

export function useAiTranslate() {
  const [state, setState] = useState<TranslateState>(INITIAL);
  const [aiConfigured, setAiConfigured] = useState(false);
  const activeIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);
  const tickRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const startedAtRef = useRef<number>(0);
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
    if (unlistenRef.current) unlistenRef.current();
    unlistenRef.current = null;
    stopTicker();
    setState((p) => ({ ...p, loading: false }));
  }, []);

  const toggleShow = useCallback(() => {
    setState((p) => (p.translated == null ? p : { ...p, showTranslated: !p.showTranslated }));
  }, []);

  const translate = useCallback(
    async (content: string): Promise<string | null> => {
      if (!aiConfigured) return null;

      // If we already have a translation for THIS content, just toggle the view.
      if (!state.loading && state.translated != null && state.source === content) {
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
        source: content,
        elapsedSec: 0,
      });
      startTicker();

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
          }
        });
        unlistenRef.current = unlisten;
        if (activeIdRef.current !== requestId) {
          unlisten();
          unlistenRef.current = null;
          return null;
        }

        armSafety();

        const translated = await tauriInvoke("ai_translate_skill_stream", {
          requestId,
          content,
        });

        if (activeIdRef.current !== requestId) return null;
        setState({
          translated,
          showTranslated: true,
          loading: false,
          error: null,
          pipelineProgress: null,
          source: content,
          elapsedSec: Math.floor((Date.now() - startedAtRef.current) / 1000),
        });
        return translated;
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
        if (unlistenRef.current) unlistenRef.current();
        unlistenRef.current = null;
        if (activeIdRef.current === requestId) activeIdRef.current = null;
        stopTicker();
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
  };
}

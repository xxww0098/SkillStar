import { useState, useRef, useCallback, useEffect } from "react";
import type { AiConfig, AiStreamPayload } from "../types";

/**
 * Shared hook for AI streaming operations (translate / summarize).
 * Consolidates the identical streaming logic previously duplicated
 * across SkillEditor, SkillReader, and DetailPanel.
 */

interface AiStreamState {
  content: string | null;
  visible: boolean;
  loading: boolean;
  hasDelta: boolean;
  wasNonStreaming: boolean;
  error: string | null;
  source: string | null;
  /** Provider that produced the translation (e.g. "ai" | "mymemory"). */
  provider: string | null;
}

const INITIAL_STATE: AiStreamState = {
  content: null,
  visible: false,
  loading: false,
  hasDelta: false,
  wasNonStreaming: false,
  error: null,
  source: null,
  provider: null,
};

interface UseAiStreamOptions {
  /** Tauri invoke command name, e.g. "ai_translate_skill_stream" */
  command: string;
  /** Event channel, e.g. "ai://translate-stream" */
  eventChannel: string;
  /** Optional: transform the final result before storing */
  normalizeResult?: (source: string, result: string) => string;
  /**
   * Optional: parse a non-string invoke result into text + optional provider.
   * For commands that return objects (e.g. `ShortTextTranslationResult`).
   * If not provided, the invoke result is used as-is (assumed string).
   */
  parseInvokeResult?: (raw: unknown) => { text: string; provider?: string };
}

interface ExecuteAiStreamOptions {
  /** Bypass cache/read-toggle logic and force a fresh backend request. */
  forceRefresh?: boolean;
  /**
   * Keep currently visible content while refreshing, until new deltas/final
   * result arrive. Useful for "retranslate" flows.
   */
  keepVisibleWhileLoading?: boolean;
  /** Additional params to pass to the Tauri invoke call. */
  extraInvokeParams?: Record<string, unknown>;
}

export function useAiStream({
  command,
  eventChannel,
  normalizeResult,
  parseInvokeResult,
}: UseAiStreamOptions) {
  const [state, setState] = useState<AiStreamState>(INITIAL_STATE);
  const [aiConfigured, setAiConfigured] = useState(false);
  const [targetLanguage, setTargetLanguage] = useState("zh-CN");

  const activeIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);
  // Mirror state in a ref so execute can read latest values without
  // being re-created on every state change (avoids stale closures).
  const stateRef = useRef(state);
  stateRef.current = state;
  // Guard async setState after component unmount
  const mountedRef = useRef(true);

  // Load AI config on mount
  useEffect(() => {
    (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const config = await invoke<AiConfig>("get_ai_config");
        if (!mountedRef.current) return;
        setAiConfigured(config.enabled && config.api_key.trim().length > 0);
        if (config.target_language) setTargetLanguage(config.target_language);
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
    async (
      sourceContent: string,
      options: ExecuteAiStreamOptions = {}
    ): Promise<string | null> => {
      if (!aiConfigured) return null;
      const forceRefresh = options.forceRefresh ?? false;
      const keepVisibleWhileLoading = options.keepVisibleWhileLoading ?? false;
      const extraInvokeParams = options.extraInvokeParams ?? {};

      const snap = stateRef.current;

      if (!forceRefresh) {
        // If already loading, cancel
        if (snap.loading) {
          cancel();
          if (!snap.content) {
            setState((prev) => ({ ...prev, visible: false }));
          }
          return null;
        }

        // If visible, toggle off
        if (snap.visible) {
          dismiss();
          return null;
        }

        // If cached result matches source, show it
        if (snap.content && snap.source === sourceContent) {
          setState((prev) => ({ ...prev, visible: true }));
          return snap.content;
        }
      } else if (snap.loading) {
        return null;
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

      setState((prev) => ({
        content: keepVisibleWhileLoading ? prev.content : null,
        visible: keepVisibleWhileLoading ? prev.visible : false,
        loading: true,
        hasDelta: false,
        wasNonStreaming: false,
        error: null,
        source: keepVisibleWhileLoading ? prev.source : null,
        provider: keepVisibleWhileLoading ? prev.provider : null,
      }));

      try {
        const { listen } = await import("@tauri-apps/api/event");
        const { invoke } = await import("@tauri-apps/api/core");

        const flushDelta = () => {
          rafId = null;
          if (activeIdRef.current !== requestId) return;
          setState((prev) => ({
            ...prev,
            content: streamedRaw,
            visible: true,
            hasDelta: deltaCount >= 2,
          }));
        };

        const unlisten = await listen<AiStreamPayload>(eventChannel, (event) => {
          if (activeIdRef.current !== requestId) return;
          const payload = event.payload;
          if (payload.requestId !== requestId) return;

          if (payload.event === "delta" && payload.delta) {
            deltaCount += 1;
            streamedRaw += payload.delta;
            if (rafId == null) {
              rafId = requestAnimationFrame(flushDelta);
            }
            return;
          }

          if (payload.event === "error" && payload.message) {
            setState((prev) => ({ ...prev, error: String(payload.message) }));
          }
        });
        unlistenRef.current = unlisten;

        const invokePayload = {
          requestId,
          content: sourceContent,
          ...(forceRefresh ? { forceRefresh: true } : {}),
          ...extraInvokeParams,
        };
        const rawResult = await invoke(command, invokePayload);

        if (activeIdRef.current !== requestId) return null;

        let finalText: string;
        let resultProvider: string | null = null;
        if (parseInvokeResult) {
          const parsed = parseInvokeResult(rawResult);
          finalText = parsed.text;
          resultProvider = parsed.provider ?? null;
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
          provider: resultProvider,
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
          provider: null,
        });
        return null;
      } finally {
        if (rafId != null) {
          cancelAnimationFrame(rafId);
          rafId = null;
        }
        if (unlistenRef.current) {
          unlistenRef.current();
          unlistenRef.current = null;
        }
        if (activeIdRef.current === requestId) {
          activeIdRef.current = null;
        }
      }
    },
    [aiConfigured, cancel, dismiss, command, eventChannel, normalizeResult, parseInvokeResult]
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

  return {
    ...state,
    aiConfigured,
    targetLanguage,
    execute,
    cancel,
    dismiss,
    hydrate: (c: string | null, source: string | null) =>
      setState((prev) => ({ ...prev, content: c, source })),
    setVisible: (v: boolean) => setState((prev) => ({ ...prev, visible: v })),
    setContent: (c: string | null) => setState((prev) => ({ ...prev, content: c })),
    setError: (e: string | null) => setState((prev) => ({ ...prev, error: e })),
  };
}

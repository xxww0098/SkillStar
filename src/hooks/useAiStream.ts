import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import type { AiStreamPayload } from "../types";
import { estimateAiStreamSafetyTimeoutMs, estimateMarkdownSectionCount } from "./aiStreamTimeouts";
import { getAiConfigCached } from "./useAiConfig";
import { getTranslationSettingsCached } from "./useTranslationSettings";

export { estimateAiStreamSafetyTimeoutMs, estimateMarkdownSectionCount };

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
  providerId: string | null;
  providerType: "translation_api" | "llm" | "fallback" | null;
  routeMode: "fast" | "balanced" | "quality" | null;
  fallbackHop: number | null;
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
  providerId: null,
  providerType: null,
  routeMode: null,
  fallbackHop: null,
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
  /**
   * When false, execute() will proceed even when AI is not configured.
   * This allows backends that have non-AI fallbacks (e.g. MyMemory) to work.
   * Default: true.
   */
  requiresAiConfig?: boolean;
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

function buildForceRefreshParams(command: string, forceRefresh: boolean): Record<string, boolean> {
  if (!forceRefresh) return {};
  // `ai_translate_skill(_stream)` expects `force`, while short-text streams
  // use the camel-cased `forceRefresh` argument.
  if (command === "ai_translate_skill" || command === "ai_translate_skill_stream") {
    return { force: true };
  }
  return { forceRefresh: true };
}

export function useAiStream({
  command,
  eventChannel,
  normalizeResult,
  parseInvokeResult,
  requiresAiConfig = true,
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

  // Load AI readiness plus Translation Center target language on mount.
  useEffect(() => {
    (async () => {
      try {
        const [config, translationSettings] = await Promise.all([
          getAiConfigCached(),
          getTranslationSettingsCached().catch(() => null),
        ]);
        if (!mountedRef.current) return;
        setAiConfigured(config.enabled && (config.api_format === "local" || config.api_key.trim().length > 0));
        const translationTarget = translationSettings?.target_language?.trim();
        if (translationTarget) {
          setTargetLanguage(translationTarget);
        }
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
      if (requiresAiConfig && !aiConfigured) return null;
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

      const SAFETY_TIMEOUT_MS = estimateAiStreamSafetyTimeoutMs(command, sourceContent);

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
            error: "Translation timed out",
            source: sourceContent,
            provider: null,
            providerId: null,
            providerType: null,
            routeMode: null,
            fallbackHop: null,
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
        provider: keepVisibleWhileLoading ? prev.provider : null,
        providerId: keepVisibleWhileLoading ? prev.providerId : null,
        providerType: keepVisibleWhileLoading ? prev.providerType : null,
        routeMode: keepVisibleWhileLoading ? prev.routeMode : null,
        fallbackHop: keepVisibleWhileLoading ? prev.fallbackHop : null,
      }));

      try {
        const flushDelta = () => {
          rafId = null;
          // NOTE: We intentionally do NOT call setState here.
          // Delta updates are accumulated internally (streamedRaw) so the final
          // result is available immediately on completion, but the UI does NOT
          // re-render per-delta — the completed translation is rendered all at
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
            const provider = payload.message.trim();
            setState((prev) => ({
              ...prev,
              provider: provider || prev.provider,
              providerId: payload.providerId ?? prev.providerId,
              providerType: payload.providerType ?? prev.providerType,
              routeMode: payload.routeMode ?? prev.routeMode,
              fallbackHop: payload.fallbackHop ?? prev.fallbackHop,
            }));
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
          ...buildForceRefreshParams(command, forceRefresh),
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
        setState((prev) => ({
          content: finalText,
          visible: true,
          loading: false,
          hasDelta: deltaCount >= 2,
          wasNonStreaming: deltaCount < 2,
          error: null,
          source: sourceContent,
          provider: resultProvider ?? prev.provider,
          providerId: prev.providerId,
          providerType: prev.providerType,
          routeMode: prev.routeMode,
          fallbackHop: prev.fallbackHop,
        }));
        return finalText;
      } catch (e) {
        if (activeIdRef.current !== requestId) return null;
        setState((prev) => ({
          content: streamedRaw.trim() ? streamedRaw : null,
          visible: !!streamedRaw.trim(),
          loading: false,
          hasDelta: deltaCount >= 2,
          wasNonStreaming: false,
          error: String(e),
          source: null,
          provider: prev.provider,
          providerId: prev.providerId,
          providerType: prev.providerType,
          routeMode: prev.routeMode,
          fallbackHop: prev.fallbackHop,
        }));
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
    [aiConfigured, requiresAiConfig, cancel, dismiss, command, eventChannel, normalizeResult, parseInvokeResult],
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
    targetLanguage,
    execute,
    cancel,
    dismiss,
    hydrate,
    setVisible,
    setContent,
    setError,
  };
}

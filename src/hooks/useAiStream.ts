import { useState, useRef, useCallback, useEffect } from "react";
import type { AiConfigStatus, AiStreamPayload } from "../types";

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
  /** Tauri invoke command name, e.g. "ai_translate_skill_stream" */
  command: string;
  /** Event channel, e.g. "ai://translate-stream" */
  eventChannel: string;
  /** Optional: transform the final result before storing */
  normalizeResult?: (source: string, result: string) => string;
}

export function useAiStream({ command, eventChannel, normalizeResult }: UseAiStreamOptions) {
  const [state, setState] = useState<AiStreamState>(INITIAL_STATE);
  const [aiConfigured, setAiConfigured] = useState(false);

  const activeIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void) | null>(null);

  // Load AI config on mount
  useEffect(() => {
    (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const config = await invoke<AiConfigStatus>("get_ai_config");
        setAiConfigured(config.enabled && config.api_key.trim().length > 0);
      } catch {
        setAiConfigured(false);
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
    async (sourceContent: string) => {
      if (!aiConfigured) return;

      // If already loading, cancel
      if (state.loading) {
        cancel();
        if (!state.content) {
          setState((prev) => ({ ...prev, visible: false }));
        }
        return;
      }

      // If visible, toggle off
      if (state.visible) {
        dismiss();
        return;
      }

      // If cached result matches source, show it
      if (state.content && state.source === sourceContent) {
        setState((prev) => ({ ...prev, visible: true }));
        return;
      }

      // Start new request
      const requestId =
        typeof crypto !== "undefined" && "randomUUID" in crypto
          ? crypto.randomUUID()
          : `ai-${Date.now()}-${Math.random().toString(16).slice(2)}`;
      activeIdRef.current = requestId;
      let streamedRaw = "";
      let deltaCount = 0;

      setState({
        content: null,
        visible: false,
        loading: true,
        hasDelta: false,
        wasNonStreaming: false,
        error: null,
        source: null,
      });

      try {
        const { listen } = await import("@tauri-apps/api/event");
        const { invoke } = await import("@tauri-apps/api/core");

        const unlisten = await listen<AiStreamPayload>(eventChannel, (event) => {
          if (activeIdRef.current !== requestId) return;
          const payload = event.payload;
          if (payload.requestId !== requestId) return;

          if (payload.event === "delta" && payload.delta) {
            deltaCount += 1;
            streamedRaw += payload.delta;
            setState((prev) => ({
              ...prev,
              content: streamedRaw,
              visible: true,
              hasDelta: deltaCount >= 2,
            }));
            return;
          }

          if (payload.event === "error" && payload.message) {
            setState((prev) => ({ ...prev, error: String(payload.message) }));
          }
        });
        unlistenRef.current = unlisten;

        const result = await invoke<string>(command, {
          requestId,
          content: sourceContent,
        });

        if (activeIdRef.current !== requestId) return;
        const finalContent = normalizeResult
          ? normalizeResult(sourceContent, result)
          : result;
        setState({
          content: finalContent,
          visible: true,
          loading: false,
          hasDelta: deltaCount >= 2,
          wasNonStreaming: deltaCount < 2,
          error: null,
          source: sourceContent,
        });
      } catch (e) {
        if (activeIdRef.current !== requestId) return;
        setState({
          content: streamedRaw.trim() ? streamedRaw : null,
          visible: !!streamedRaw.trim(),
          loading: false,
          hasDelta: deltaCount >= 2,
          wasNonStreaming: false,
          error: String(e),
          source: null,
        });
      } finally {
        if (unlistenRef.current) {
          unlistenRef.current();
          unlistenRef.current = null;
        }
        if (activeIdRef.current === requestId) {
          activeIdRef.current = null;
        }
      }
    },
    [aiConfigured, state.loading, state.visible, state.content, state.source, cancel, dismiss, command, eventChannel, normalizeResult]
  );

  // Cleanup on unmount
  useEffect(() => {
    return () => {
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
    execute,
    cancel,
    dismiss,
    setVisible: (v: boolean) => setState((prev) => ({ ...prev, visible: v })),
    setContent: (c: string | null) => setState((prev) => ({ ...prev, content: c })),
    setError: (e: string | null) => setState((prev) => ({ ...prev, error: e })),
  };
}

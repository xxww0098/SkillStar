/**
 * Debounced autosave with an explicit save-state machine. Owns the 600ms
 * debounce semantics that previously lived inside useProviderFormState, plus
 * one fix: `flush()` lets the drawer push a pending edit out immediately when
 * it closes (the old implementation just cleared the timer on unmount and the
 * edit was silently lost).
 *
 * Re-arm semantics match the old hook: after a failed attempt the timer does
 * NOT re-arm until `save` changes identity (i.e. the user edits again), so a
 * validation error toasts once instead of every 600ms.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { ProviderSaveState, SaveAttemptResult } from "../types";

const AUTO_SAVE_DEBOUNCE_MS = 600;

export interface UseAutosaveOptions {
  /** Whether the current values differ from the persisted entity. */
  dirty: boolean;
  /** Perform one save attempt. Must never throw. */
  save: () => Promise<SaveAttemptResult>;
  debounceMs?: number;
}

export function useAutosave({ dirty, save, debounceMs = AUTO_SAVE_DEBOUNCE_MS }: UseAutosaveOptions) {
  const [state, setState] = useState<ProviderSaveState>("idle");
  const runningRef = useRef(false);
  const saveRef = useRef(save);
  saveRef.current = save;
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  const run = useCallback(async () => {
    if (runningRef.current) return;
    runningRef.current = true;
    setState("saving");
    try {
      const result = await saveRef.current();
      setState(result === "saved" ? "saved" : "error");
    } finally {
      runningRef.current = false;
    }
  }, []);

  useEffect(() => {
    if (!dirty || runningRef.current) return;
    setState("dirty");
    const timer = window.setTimeout(() => void run(), debounceMs);
    return () => window.clearTimeout(timer);
    // `save` identity changes whenever form values change — that is the re-arm signal.
  }, [dirty, save, debounceMs, run]);

  /** Save now if there is anything pending (used when the drawer closes). */
  const flush = useCallback(async () => {
    if (!dirtyRef.current || runningRef.current) return;
    await run();
  }, [run]);

  return { state, flush, saving: state === "saving" };
}

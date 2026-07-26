/**
 * Debounced autosave with an explicit save-state machine. Owns the 600ms
 * debounce semantics that previously lived inside useProviderFormState, plus
 * one fix: `flush()` lets the drawer push a pending edit out immediately when
 * it closes (the old implementation just cleared the timer on unmount and the
 * edit was silently lost).
 *
 * Re-arm semantics match the old hook: after a failed attempt the timer does
 * NOT re-arm until the form change token advances (i.e. the user edits
 * again), so a validation error toasts once instead of every 600ms.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { ProviderSaveState, SaveAttemptResult } from "../types";

const AUTO_SAVE_DEBOUNCE_MS = 600;

export interface UseAutosaveOptions {
  /** Whether the current values differ from the persisted entity. */
  dirty: boolean;
  /** Perform one save attempt. Must never throw. */
  save: () => Promise<SaveAttemptResult>;
  /** Identity token for the values captured by `save` (normally the form values object). */
  changeToken?: unknown;
  debounceMs?: number;
}

export function useAutosave({ dirty, save, changeToken, debounceMs = AUTO_SAVE_DEBOUNCE_MS }: UseAutosaveOptions) {
  const [state, setState] = useState<ProviderSaveState>("idle");
  const runningRef = useRef(false);
  const runningPromiseRef = useRef<Promise<SaveAttemptResult | null> | null>(null);
  const saveRef = useRef(save);
  const saveRevisionRef = useRef(0);
  const attemptedRevisionRef = useRef(-1);
  const revisionSignal = changeToken === undefined ? save : changeToken;
  const revisionSignalRef = useRef(revisionSignal);
  saveRef.current = save;
  if (revisionSignalRef.current !== revisionSignal) {
    revisionSignalRef.current = revisionSignal;
    saveRevisionRef.current += 1;
  }
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  const run = useCallback((): Promise<SaveAttemptResult | null> => {
    if (runningPromiseRef.current) return runningPromiseRef.current;
    const pending = (async () => {
      runningRef.current = true;
      setState("saving");
      try {
        let result: SaveAttemptResult | null = null;
        do {
          const revision = saveRevisionRef.current;
          const saveAttempt = saveRef.current;
          result = await saveAttempt();
          attemptedRevisionRef.current = revision;
          // A user edit can land while the request is in flight. Keep this
          // promise alive and persist the latest snapshot before flush() lets
          // a drawer or dialog close.
        } while (dirtyRef.current && saveRevisionRef.current > attemptedRevisionRef.current);
        setState(result === "saved" ? "saved" : "error");
        return result;
      } finally {
        runningRef.current = false;
        runningPromiseRef.current = null;
      }
    })();
    runningPromiseRef.current = pending;
    return pending;
  }, []);

  useEffect(() => {
    if (!dirty) return;
    if (!runningRef.current) setState("dirty");
    const timer = window.setTimeout(() => void run(), debounceMs);
    return () => window.clearTimeout(timer);
    // `revisionSignal` changes whenever form values change — that is the re-arm signal.
  }, [dirty, revisionSignal, debounceMs, run]);

  /** Save now if there is anything pending (used when the drawer closes). */
  const flush = useCallback(async (): Promise<SaveAttemptResult | null> => {
    if (runningPromiseRef.current) return runningPromiseRef.current;
    if (!dirtyRef.current) return null;
    return run();
  }, [run]);

  return { state, flush, saving: state === "saving" };
}

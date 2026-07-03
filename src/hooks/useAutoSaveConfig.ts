import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Debounced auto-save state machine, generalized from the (formerly)
 * triplicated proxy / mirror / AI-provider config flows in
 * `src/pages/Settings.tsx`.
 *
 * Flow: load() once on mount → edits go through setConfig() → once the
 * config differs from the last-saved baseline, wait `debounceMs` of
 * inactivity → save() → on success, show a `showSaved` indicator for
 * `savedIndicatorMs` then hide it; on failure, revert to the last-saved
 * baseline and report the error via `onSaveError`.
 *
 * `hydrate()` and `markSaved()` are escape hatches for call sites that need
 * to adopt a config from outside the debounce loop (e.g. re-syncing from an
 * external hook, or committing an out-of-band save immediately).
 */

const DEFAULT_DEBOUNCE_MS = 600;
const DEFAULT_SAVED_INDICATOR_MS = 2000;

export interface UseAutoSaveConfigOptions<T> {
  /** Called once on mount to fetch the initial config. */
  load: () => Promise<T>;
  /** Used as the initial (pre-load) config, and as the fallback if load() rejects. */
  fallback: T;
  /** Persists `config`. Must reject on failure so the hook can revert + report. */
  save: (config: T) => Promise<void>;
  /** Structural/value equality between two configs — drives dirty detection. */
  isEqual: (a: T, b: T) => boolean;
  /** Debounce window before an edit triggers a save. Default 600ms. */
  debounceMs?: number;
  /** How long `showSaved` stays true after a successful save. Default 2000ms. */
  savedIndicatorMs?: number;
  /** Called with the raw error whenever save() rejects (e.g. to toast it). */
  onSaveError?: (error: unknown) => void;
  /** While true, suppresses auto-save scheduling (e.g. a concurrent test-connection flow). */
  skip?: boolean;
}

export interface UseAutoSaveConfigResult<T> {
  config: T;
  setConfig: (next: T) => void;
  /** True once the initial load() has settled (success or fallback). */
  loaded: boolean;
  /** True while a save() call is in flight. */
  saving: boolean;
  /** True for `savedIndicatorMs` after a successful save. */
  showSaved: boolean;
  /** Discards unsaved edits, resetting `config` back to the last-saved baseline. */
  revert: () => void;
  /** Adopts an externally-obtained config as both `config` and the saved baseline. */
  hydrate: (config: T) => void;
  /** Re-baselines the saved config without touching `config` or `loaded` (for out-of-band saves). */
  markSaved: (config: T) => void;
}

export function useAutoSaveConfig<T>({
  load,
  fallback,
  save,
  isEqual,
  debounceMs = DEFAULT_DEBOUNCE_MS,
  savedIndicatorMs = DEFAULT_SAVED_INDICATOR_MS,
  onSaveError,
  skip = false,
}: UseAutoSaveConfigOptions<T>): UseAutoSaveConfigResult<T> {
  const [config, setConfigState] = useState<T>(fallback);
  const [savedConfig, setSavedConfig] = useState<T>(fallback);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [showSaved, setShowSaved] = useState(false);

  // Latest-value refs so the debounce effect's closure always calls the
  // current `load`/`save`/`onSaveError` without needing them in its deps
  // (which would re-arm the timer on every parent re-render).
  const loadRef = useRef(load);
  loadRef.current = load;
  const saveRef = useRef(save);
  saveRef.current = save;
  const onSaveErrorRef = useRef(onSaveError);
  onSaveErrorRef.current = onSaveError;

  // ── Initial load (once) ───────────────────────────────────────────────────

  useEffect(() => {
    let cancelled = false;
    loadRef
      .current()
      .then((loadedConfig) => {
        if (cancelled) return;
        setConfigState(loadedConfig);
        setSavedConfig(loadedConfig);
        setLoaded(true);
      })
      .catch(() => {
        if (cancelled) return;
        setConfigState(fallback);
        setSavedConfig(fallback);
        setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
    // Intentionally run once: `load` identity changes are not a reload signal
    // (matches the previous per-config useEffect(..., []) behavior).
  }, []);

  // ── Debounced auto-save ────────────────────────────────────────────────────

  useEffect(() => {
    if (!loaded || saving || skip || isEqual(config, savedConfig)) {
      return;
    }

    const previousConfig = savedConfig;
    const timer = setTimeout(() => {
      setSaving(true);
      saveRef
        .current(config)
        .then(() => {
          setSavedConfig(config);
          setShowSaved(true);
          setTimeout(() => setShowSaved(false), savedIndicatorMs);
        })
        .catch((error) => {
          setConfigState(previousConfig);
          onSaveErrorRef.current?.(error);
        })
        .finally(() => setSaving(false));
    }, debounceMs);

    return () => clearTimeout(timer);
  }, [config, savedConfig, loaded, saving, skip, isEqual, debounceMs, savedIndicatorMs]);

  const setConfig = useCallback((next: T) => setConfigState(next), []);

  const revert = useCallback(() => setConfigState(savedConfig), [savedConfig]);

  const hydrate = useCallback((next: T) => {
    setConfigState(next);
    setSavedConfig(next);
    setLoaded(true);
  }, []);

  const markSaved = useCallback((next: T) => setSavedConfig(next), []);

  return { config, setConfig, loaded, saving, showSaved, revert, hydrate, markSaved };
}

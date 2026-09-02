import { useEffect, useState } from "react";

/**
 * Delay propagating `value` until it has been stable for `delayMs`.
 *
 * Search boxes that compile into IPC (MCP's 21k-row catalog, marketplace FTS)
 * must not fire on every keystroke. The typed value stays in the input; only
 * the query key waits.
 */
export function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    if (delayMs <= 0) {
      setDebounced(value);
      return;
    }
    const timer = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(timer);
  }, [value, delayMs]);

  return debounced;
}

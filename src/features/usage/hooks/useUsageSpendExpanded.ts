import { useCallback, useState } from "react";

const STORAGE_KEY = "skillstar:usage-spend-expanded";

function readExpanded(): boolean {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "false") return false;
  } catch {
    // ignore
  }
  return true;
}

export function useUsageSpendExpanded() {
  const [expanded, setExpandedState] = useState(readExpanded);

  const setExpanded = useCallback((next: boolean) => {
    try {
      localStorage.setItem(STORAGE_KEY, String(next));
    } catch {
      // ignore
    }
    setExpandedState(next);
  }, []);

  const toggle = useCallback(() => {
    setExpandedState((prev) => {
      const next = !prev;
      try {
        localStorage.setItem(STORAGE_KEY, String(next));
      } catch {
        // ignore
      }
      return next;
    });
  }, []);

  return { expanded, setExpanded, toggle };
}

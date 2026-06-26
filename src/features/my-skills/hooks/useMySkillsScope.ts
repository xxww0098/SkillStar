import { useCallback, useEffect, useState } from "react";
import type { MySkillsScope } from "../components/MySkillsScopeSwitch";

const SCOPE_STORAGE_KEY = "skillstar.mySkills.scope";

/**
 * Owns the My Skills local/remote scope: `localStorage` persistence plus the
 * legacy `#ssh` deep-link that jumps straight to the remote scope. Extracted so
 * the scope set lives in one place — widening it to a third scope is a union
 * edit here + one render arm in the page.
 */
export function useMySkillsScope() {
  const [scope, setScopeState] = useState<MySkillsScope>(() => {
    if (typeof localStorage === "undefined") return "local";
    return localStorage.getItem(SCOPE_STORAGE_KEY) === "remote" ? "remote" : "local";
  });

  const setScope = useCallback((next: MySkillsScope) => {
    setScopeState(next);
    if (typeof localStorage !== "undefined") localStorage.setItem(SCOPE_STORAGE_KEY, next);
  }, []);

  // Legacy deep-link: `#ssh` (remote) or `#cloud` opens the matching scope,
  // then normalises the hash.
  useEffect(() => {
    if (typeof window === "undefined") return;
    const hash = window.location.hash.slice(1);
    if (hash === "ssh" || hash === "cloud") {
      const target: MySkillsScope = hash === "cloud" ? "cloud" : "remote";
      setScopeState(target);
      if (typeof localStorage !== "undefined") localStorage.setItem(SCOPE_STORAGE_KEY, target);
      window.location.hash = "skills";
    }
  }, []);

  return { scope, setScope };
}

import {
  createElement,
  createContext,
  useState,
  useEffect,
  useCallback,
  useRef,
  useContext,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Skill, SkillContent, SkillUpdateState, UpdateResult } from "../types";
import { getSkillUpdateRefreshIntervalMs } from "../lib/skillUpdateRefresh";

const SKILL_LIST_REFRESH_INTERVAL_MS = 30_000;

function sortedJoined(values?: string[]): string {
  if (!values || values.length === 0) return "";
  return [...values].sort().join(",");
}

function skillSignature(skill: Skill): string {
  return [
    skill.name,
    skill.description,
    skill.skill_type ?? "hub",
    String(skill.stars),
    String(skill.installed),
    String(skill.update_available),
    skill.last_updated,
    skill.git_url,
    skill.tree_hash ?? "",
    skill.category,
    skill.author ?? "",
    skill.source ?? "",
    String(skill.rank ?? ""),
    sortedJoined(skill.topics),
    sortedJoined(skill.agent_links),
  ].join("|");
}

function sameSkillSnapshot(prev: Skill[], next: Skill[]): boolean {
  if (prev.length !== next.length) return false;

  const prevMap = new Map(prev.map((skill) => [skill.name, skillSignature(skill)]));
  for (const skill of next) {
    const nextSig = skillSignature(skill);
    if (prevMap.get(skill.name) !== nextSig) {
      return false;
    }
  }
  return true;
}

function mergeLocalSkillSnapshot(prev: Skill[], next: Skill[]): Skill[] {
  if (prev.length === 0) return next;

  const prevMap = new Map(prev.map((skill) => [skill.name, skill]));
  return next.map((skill) => {
    const previous = prevMap.get(skill.name);
    if (!previous) return skill;
    return {
      ...skill,
      update_available: previous.update_available,
    };
  });
}

type SkillsState = ReturnType<typeof useSkillsState>;

const SkillsContext = createContext<SkillsState | null>(null);

function useSkillsState() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pendingUpdateNames, setPendingUpdateNames] = useState<Set<string>>(new Set());
  const pendingUpdateRef = useRef<Set<string>>(new Set());
  const [pendingAgentToggleKeys, setPendingAgentToggleKeys] = useState<Set<string>>(
    new Set()
  );
  const isTogglingAgentRef = useRef(false);
  const isCheckingUpdatesRef = useRef(false);
  const refreshRequestIdRef = useRef(0);
  const updateCheckRequestIdRef = useRef(0);
  const lastUpdateCheckAtRef = useRef(0);
  const mutationEpochRef = useRef(0);
  const pendingAgentToggleRef = useRef<Set<string>>(new Set());
  const getUpdateCheckIntervalMs = useCallback(
    () => getSkillUpdateRefreshIntervalMs(),
    []
  );

  const refreshUpdateAvailability = useCallback(async (force = false) => {
    if (isCheckingUpdatesRef.current) return;
    const checkIntervalMs = getUpdateCheckIntervalMs();
    if (!force && Date.now() - lastUpdateCheckAtRef.current < checkIntervalMs) {
      return;
    }

    isCheckingUpdatesRef.current = true;
    const requestId = ++updateCheckRequestIdRef.current;

    try {
      const updates = await invoke<SkillUpdateState[]>("refresh_skill_updates");
      if (requestId !== updateCheckRequestIdRef.current) return;

      lastUpdateCheckAtRef.current = Date.now();
      setSkills((prev) => {
        if (prev.length === 0) return prev;

        const updatesByName = new Map(
          updates.map((update) => [update.name, update.update_available])
        );

        let changed = false;
        const next = prev.map((skill) => {
          const updateAvailable = updatesByName.get(skill.name);
          if (
            updateAvailable === undefined ||
            updateAvailable === skill.update_available
          ) {
            return skill;
          }
          changed = true;
          return { ...skill, update_available: updateAvailable };
        });

        return changed ? next : prev;
      });
    } catch (e) {
      console.error("Update availability refresh failed:", e);
    } finally {
      isCheckingUpdatesRef.current = false;
    }
  }, [getUpdateCheckIntervalMs]);

  const refresh = useCallback(async (silent = false, force = false) => {
    // During an agent-link toggle, ignore background polling refreshes so
    // stale list data can't momentarily overwrite optimistic UI state.
    if (silent && !force && isTogglingAgentRef.current) return;
    const requestId = ++refreshRequestIdRef.current;
    const startedMutationEpoch = mutationEpochRef.current;

    if (!silent) {
      setLoading(true);
      setError(null);
    }
    try {
      const result = await invoke<Skill[]>("list_skills");
      // Drop stale refresh results (older than the latest request or started
      // before a mutation) unless this call is explicitly forced.
      if (!force) {
        if (requestId !== refreshRequestIdRef.current) return;
        if (startedMutationEpoch !== mutationEpochRef.current) return;
      }
      setSkills((prev) => {
        const merged = mergeLocalSkillSnapshot(prev, result);
        return sameSkillSnapshot(prev, merged) ? prev : merged;
      });
      void refreshUpdateAvailability(force);
    } catch (e) {
      if (!silent) setError(String(e));
    } finally {
      if (!silent) setLoading(false);
    }
  }, [refreshUpdateAvailability]);

  useEffect(() => {
    // Auto-migrate local skills on first mount (fire-and-forget)
    invoke<number>("migrate_local_skills").catch(() => {});

    refresh();
    // Background polling for local list snapshot refresh.
    const interval = setInterval(() => refresh(true), SKILL_LIST_REFRESH_INTERVAL_MS);
    
    // Refresh immediately when window gains focus
    const handleFocus = () => refresh(true);
    window.addEventListener("focus", handleFocus);

    return () => {
      clearInterval(interval);
      window.removeEventListener("focus", handleFocus);
    };
  }, [refresh]);

  useEffect(() => {
    const handleExternalRefresh = () => {
      void refresh(true, true);
    };

    window.addEventListener("skillstar:refresh-skills", handleExternalRefresh);
    return () => {
      window.removeEventListener("skillstar:refresh-skills", handleExternalRefresh);
    };
  }, [refresh]);



  // ── Patrol backend event listener ────────────────────────────────
  // The Rust backend emits "patrol://skill-checked" after each single-skill
  // update check. We merge the result into the local skills state so the UI
  // reflects updates found during background patrol checks.
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    listen<{ name: string; update_available: boolean }>(
      "patrol://skill-checked",
      (event) => {
        const { name, update_available } = event.payload;
        setSkills((prev) => {
          const skill = prev.find((s) => s.name === name);
          if (!skill || skill.update_available === update_available) return prev;
          return prev.map((s) =>
            s.name === name ? { ...s, update_available } : s
          );
        });
      }
    ).then((fn_) => {
      unlisten = fn_;
    });

    return () => {
      unlisten?.();
    };
  }, []);

  const installSkill = useCallback(async (url: string, name?: string) => {
    try {
      const skill = await invoke<Skill>("install_skill", { url, name });
      setSkills((prev) => [...prev, skill]);
      lastUpdateCheckAtRef.current = 0;
      return skill;
    } catch (e) {
      throw new Error(String(e));
    }
  }, []);

  const uninstallSkill = useCallback(async (name: string) => {
    try {
      await invoke("uninstall_skill", { name });
      setSkills((prev) => prev.filter((s) => s.name !== name));
    } catch (e) {
      throw new Error(String(e));
    }
  }, []);

  const updateSkill = useCallback(async (name: string) => {
    if (pendingUpdateRef.current.has(name)) {
      throw new Error("Update already in progress");
    }

    pendingUpdateRef.current.add(name);
    setPendingUpdateNames(new Set(pendingUpdateRef.current));

    try {
      const result = await invoke<UpdateResult>("update_skill", { name });
      const siblingSet = new Set(result.siblings_cleared);
      setSkills((prev) =>
        prev.map((s) => {
          if (s.name === name) return result.skill;
          if (siblingSet.has(s.name)) return { ...s, update_available: false };
          return s;
        })
      );
      lastUpdateCheckAtRef.current = 0;
      return result.skill;
    } catch (e) {
      throw new Error(String(e));
    } finally {
      pendingUpdateRef.current.delete(name);
      setPendingUpdateNames(new Set(pendingUpdateRef.current));
    }
  }, []);

  const toggleSkillForAgent = useCallback(async (skillName: string, agentId: string, enable: boolean, agentName?: string) => {
    const toggleKey = `${skillName}::${agentId}`;
    if (pendingAgentToggleRef.current.has(toggleKey)) return;

    pendingAgentToggleRef.current.add(toggleKey);
    setPendingAgentToggleKeys(new Set(pendingAgentToggleRef.current));
    mutationEpochRef.current += 1;
    isTogglingAgentRef.current = true;
    try {
      if (agentName) {
        setSkills(prev => prev.map(s => {
          if (s.name !== skillName) return s;
          const links = s.agent_links || [];
          return {
            ...s,
            agent_links: enable
              ? [...new Set([...links, agentName])]
              : links.filter(l => l !== agentName)
          };
        }));
      }
      await invoke("toggle_skill_for_agent", { skillName, agentId, enable });
      // Success path keeps optimistic UI state to avoid visible re-flash.
    } catch (e) {
      // Roll back from source-of-truth on failure.
      await refresh(true, true);
      throw new Error(String(e));
    } finally {
      pendingAgentToggleRef.current.delete(toggleKey);
      setPendingAgentToggleKeys(new Set(pendingAgentToggleRef.current));
      isTogglingAgentRef.current = false;
    }
  }, [refresh]);

  const readSkillContent = useCallback(async (name: string) => {
    try {
      return await invoke<SkillContent>("read_skill_content", { name });
    } catch (e) {
      throw new Error(String(e));
    }
  }, []);

  const updateSkillContent = useCallback(async (name: string, content: string) => {
    try {
      await invoke("update_skill_content", { name, content });
    } catch (e) {
      throw new Error(String(e));
    }
  }, []);

  const createLocalSkill = useCallback(async (name: string, content?: string) => {
    try {
      const skill = await invoke<Skill>("create_local_skill", { name, content });
      setSkills((prev) => [...prev, skill]);
      return skill;
    } catch (e) {
      throw new Error(String(e));
    }
  }, []);

  const deleteLocalSkill = useCallback(async (name: string) => {
    try {
      await invoke("delete_local_skill", { name });
      setSkills((prev) => prev.filter((s) => s.name !== name));
    } catch (e) {
      throw new Error(String(e));
    }
  }, []);

  return { 
    skills, 
    loading, 
    error, 
    pendingUpdateNames,
    refresh, 
    installSkill, 
    uninstallSkill, 
    updateSkill, 
    toggleSkillForAgent,
    pendingAgentToggleKeys,
    readSkillContent,
    updateSkillContent,
    createLocalSkill,
    deleteLocalSkill,
  };
}

export function SkillsProvider({ children }: { children: ReactNode }) {
  const value = useSkillsState();
  return createElement(SkillsContext.Provider, { value }, children);
}

export function useSkills() {
  const context = useContext(SkillsContext);
  if (!context) {
    throw new Error("useSkills must be used within a SkillsProvider");
  }
  return context;
}

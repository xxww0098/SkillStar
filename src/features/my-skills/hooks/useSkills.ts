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
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import type { Skill, SkillContent, SkillUpdateState, UpdateResult } from "../../../types";
import { getSkillUpdateRefreshIntervalMs } from "../../../lib/skillUpdateRefresh";

const SKILLS_QUERY_KEY = ["skills"] as const;
const SKILL_UPDATES_QUERY_KEY = ["skills", "updates"] as const;
const SKILL_LIST_REFRESH_INTERVAL_MS = 30_000;

type SkillsState = ReturnType<typeof useSkillsState>;

const SkillsContext = createContext<SkillsState | null>(null);

async function listSkills(): Promise<Skill[]> {
  return invoke<Skill[]>("list_skills");
}

function useSkillsState() {
  const queryClient = useQueryClient();
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [pendingUpdateNames, setPendingUpdateNames] = useState<Set<string>>(new Set());
  const pendingUpdateRef = useRef<Set<string>>(new Set());
  const [pendingAgentToggleKeys, setPendingAgentToggleKeys] = useState<Set<string>>(new Set());
  const pendingAgentToggleRef = useRef<Set<string>>(new Set());
  const [isTogglingAgent, setIsTogglingAgent] = useState(false);

  const updateCheckIntervalMs = getSkillUpdateRefreshIntervalMs();

  const applyUpdateStates = useCallback(
    (updates: SkillUpdateState[]) => {
      if (updates.length === 0) return;

      const updatesByName = new Map(
        updates.map((update) => [update.name, update.update_available]),
      );

      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
        if (prev.length === 0) return prev;

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
    },
    [queryClient],
  );

  const skillsQuery = useQuery({
    queryKey: SKILLS_QUERY_KEY,
    queryFn: listSkills,
    refetchOnWindowFocus: true,
    refetchInterval: isTogglingAgent ? false : SKILL_LIST_REFRESH_INTERVAL_MS,
  });

  const skills = skillsQuery.data ?? [];

  const updatesQuery = useQuery({
    queryKey: SKILL_UPDATES_QUERY_KEY,
    queryFn: () => invoke<SkillUpdateState[]>("refresh_skill_updates"),
    enabled: skills.length > 0 && !isTogglingAgent,
    refetchOnWindowFocus: false,
    refetchInterval: isTogglingAgent ? false : updateCheckIntervalMs,
    staleTime: updateCheckIntervalMs,
  });

  useEffect(() => {
    if (updatesQuery.data) {
      applyUpdateStates(updatesQuery.data);
    }
  }, [updatesQuery.data, applyUpdateStates]);

  const refetchSkills = skillsQuery.refetch;
  const refetchUpdates = updatesQuery.refetch;

  const refresh = useCallback(
    async (_silent = false, force = false) => {
      setRefreshError(null);

      try {
        if (force) {
          await queryClient.invalidateQueries({
            queryKey: SKILLS_QUERY_KEY,
            exact: true,
          });
        }

        await refetchSkills();
        await refetchUpdates();
      } catch (e) {
        setRefreshError(String(e));
      }
    },
    [queryClient, refetchSkills, refetchUpdates],
  );

  useEffect(() => {
    invoke<number>("migrate_local_skills").catch(() => {});
    void refresh(true, true);
  }, [refresh]);

  useEffect(() => {
    let unlistenTauri: UnlistenFn | null = null;
    const handleExternalRefresh = () => {
      void refresh(true, true);
    };

    window.addEventListener("skillstar:refresh-skills", handleExternalRefresh);
    
    listen("ai://translations-updated", handleExternalRefresh).then((unlistenFn) => {
      unlistenTauri = unlistenFn;
    });

    return () => {
      window.removeEventListener("skillstar:refresh-skills", handleExternalRefresh);
      unlistenTauri?.();
    };
  }, [refresh]);

  // Rust backend emits "patrol://skill-checked"; merge into query cache.
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    listen<{ name: string; update_available: boolean }>(
      "patrol://skill-checked",
      (event) => {
        const { name, update_available } = event.payload;
        queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
          const skill = prev.find((item) => item.name === name);
          if (!skill || skill.update_available === update_available) return prev;
          return prev.map((item) =>
            item.name === name ? { ...item, update_available } : item,
          );
        });
      },
    ).then((fn_) => {
      unlisten = fn_;
    });

    return () => {
      unlisten?.();
    };
  }, [queryClient]);

  const installMutation = useMutation({
    mutationFn: ({ url, name }: { url: string; name?: string }) =>
      invoke<Skill>("install_skill", { url, name }),
    onSuccess: (skill) => {
      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => {
        if (prev.some((item) => item.name === skill.name)) {
          return prev.map((item) => (item.name === skill.name ? skill : item));
        }
        return [...prev, skill];
      });
      void refetchUpdates();
    },
  });

  const uninstallMutation = useMutation({
    mutationFn: (name: string) => invoke("uninstall_skill", { name }),
    onSuccess: (_result, name) => {
      queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) =>
        prev.filter((item) => item.name !== name),
      );
    },
  });

  const installSkill = useCallback(
    async (url: string, name?: string) => {
      try {
        return await installMutation.mutateAsync({ url, name });
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [installMutation],
  );

  const uninstallSkill = useCallback(
    async (name: string) => {
      try {
        await uninstallMutation.mutateAsync(name);
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [uninstallMutation],
  );

  const updateSkill = useCallback(
    async (name: string) => {
      if (pendingUpdateRef.current.has(name)) {
        throw new Error("Update already in progress");
      }

      pendingUpdateRef.current.add(name);
      setPendingUpdateNames(new Set(pendingUpdateRef.current));

      try {
        const result = await invoke<UpdateResult>("update_skill", { name });
        const siblingSet = new Set(result.siblings_cleared);
        queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) =>
          prev.map((item) => {
            if (item.name === name) return result.skill;
            if (siblingSet.has(item.name)) {
              return { ...item, update_available: false };
            }
            return item;
          }),
        );
        void refetchUpdates();
        return result.skill;
      } catch (e) {
        throw new Error(String(e));
      } finally {
        pendingUpdateRef.current.delete(name);
        setPendingUpdateNames(new Set(pendingUpdateRef.current));
      }
    },
    [queryClient, refetchUpdates],
  );

  const toggleSkillForAgent = useCallback(
    async (skillName: string, agentId: string, enable: boolean, agentName?: string) => {
      const toggleKey = `${skillName}::${agentId}`;
      if (pendingAgentToggleRef.current.has(toggleKey)) return;

      pendingAgentToggleRef.current.add(toggleKey);
      setPendingAgentToggleKeys(new Set(pendingAgentToggleRef.current));
      setIsTogglingAgent(true);

      const previousSnapshot = queryClient.getQueryData<Skill[]>(SKILLS_QUERY_KEY) ?? [];

      try {
        if (agentName) {
          queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) =>
            prev.map((item) => {
              if (item.name !== skillName) return item;
              const links = item.agent_links ?? [];
              return {
                ...item,
                agent_links: enable
                  ? [...new Set([...links, agentName])]
                  : links.filter((link) => link !== agentName),
              };
            }),
          );
        }

        await invoke("toggle_skill_for_agent", { skillName, agentId, enable });
      } catch (e) {
        queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, previousSnapshot);
        await refresh(true, true);
        throw new Error(String(e));
      } finally {
        pendingAgentToggleRef.current.delete(toggleKey);
        setPendingAgentToggleKeys(new Set(pendingAgentToggleRef.current));
        setIsTogglingAgent(pendingAgentToggleRef.current.size > 0);
      }
    },
    [queryClient, refresh],
  );

  const batchRemoveSkillsFromAllAgents = useCallback(
    async (skillNames: string[]) => {
      try {
        await invoke("batch_remove_skills_from_all_agents", { skillNames });
        await refresh(true, true);
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [refresh],
  );

  const batchAiProcessSkills = useCallback(
    async (skillNames: string[]) => {
      try {
        await invoke("ai_batch_process_skills", { skillNames });
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [],
  );

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

  const createLocalSkill = useCallback(
    async (name: string, content?: string) => {
      try {
        const skill = await invoke<Skill>("create_local_skill", { name, content });
        queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) => [...prev, skill]);
        return skill;
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [queryClient],
  );

  const deleteLocalSkill = useCallback(
    async (name: string) => {
      try {
        await invoke("delete_local_skill", { name });
        queryClient.setQueryData<Skill[]>(SKILLS_QUERY_KEY, (prev = []) =>
          prev.filter((item) => item.name !== name),
        );
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [queryClient],
  );

  const loading = skillsQuery.isPending || (skillsQuery.isFetching && skills.length === 0);
  const error = refreshError ?? (skillsQuery.error ? String(skillsQuery.error) : null);

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
    batchRemoveSkillsFromAllAgents,
    pendingAgentToggleKeys,
    readSkillContent,
    updateSkillContent,
    createLocalSkill,
    deleteLocalSkill,
    batchAiProcessSkills,
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

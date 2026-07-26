import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect } from "react";
import { tauriInvoke, useTauriQuery } from "../lib/ipc";
import type { AgentProfile, CustomProfileDef } from "../types";

/** Must match `useTauriQuery`'s `[command]` queryKey convention (see src/lib/ipc/core.ts). */
const PROFILES_QUERY_KEY = ["list_agent_profiles"] as const;

const NO_PROFILES: AgentProfile[] = [];

/**
 * Shared agent-profiles state backed by the React Query cache.
 *
 * All hook instances observe the same `["list_agent_profiles"]` cache entry, so
 * concurrent mounts dedupe into a single `list_agent_profiles` invoke and every
 * consumer sees consistent data. The API mirrors the previous useState-based
 * implementation (`profiles` / `loading` / `refresh` / mutation helpers).
 */
export function useAgentProfiles() {
  const queryClient = useQueryClient();
  const query = useTauriQuery("list_agent_profiles", {
    // Match the previous hand-rolled behavior: a single attempt per fetch and
    // no silent refetch on window focus.
    retry: false,
    refetchOnWindowFocus: false,
  });

  const { error } = query;
  useEffect(() => {
    if (error && import.meta.env.DEV) console.error("Failed to load agent profiles:", error);
  }, [error]);

  const setProfiles = useCallback(
    (updater: (prev: AgentProfile[]) => AgentProfile[]) => {
      queryClient.setQueryData<AgentProfile[]>(PROFILES_QUERY_KEY, (prev) => updater(prev ?? NO_PROFILES));
    },
    [queryClient],
  );

  const refresh = useCallback(async () => {
    await queryClient.refetchQueries({ queryKey: PROFILES_QUERY_KEY });
  }, [queryClient]);

  const toggleProfile = useCallback(
    async (id: string) => {
      try {
        const newState = await tauriInvoke("toggle_agent_profile", { id });
        setProfiles((prev) => prev.map((p) => (p.id === id ? { ...p, enabled: newState, installed: newState } : p)));
        return newState;
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [setProfiles],
  );

  const deploySkillsToProject = useCallback(
    async (projectPath: string, selectedSkills: string[], agentTypes: string[]) => {
      try {
        const count = await tauriInvoke("create_project_skills", {
          projectPath,
          selectedSkills,
          agentTypes,
        });
        return count;
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [],
  );

  const unlinkAllFromAgent = useCallback(
    async (agentId: string) => {
      try {
        const removed = await tauriInvoke("unlink_all_skills_from_agent", { agentId });
        setProfiles((prev) => prev.map((p) => (p.id === agentId ? { ...p, synced_count: 0 } : p)));
        return removed;
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [setProfiles],
  );

  const addCustomProfile = useCallback(
    async (def: CustomProfileDef) => {
      try {
        await tauriInvoke("add_custom_agent_profile", { def });
        await refresh();
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [refresh],
  );

  const removeCustomProfile = useCallback(
    async (id: string) => {
      try {
        await tauriInvoke("remove_custom_agent_profile", { id });
        await refresh();
      } catch (e) {
        throw new Error(String(e));
      }
    },
    [refresh],
  );

  return {
    profiles: query.data ?? NO_PROFILES,
    loading: query.isPending,
    refresh,
    toggleProfile,
    deploySkillsToProject,
    unlinkAllFromAgent,
    addCustomProfile,
    removeCustomProfile,
  };
}

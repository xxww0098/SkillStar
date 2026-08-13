import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect } from "react";
import { tauriInvoke, useTauriQuery } from "../../../lib/ipc";
import type { SkillCardDeck } from "../../../types";

/** Must match `useTauriQuery`'s `[command]` queryKey convention (see src/lib/ipc/core.ts). */
const GROUPS_QUERY_KEY = ["list_skill_groups"] as const;

const NO_GROUPS: SkillCardDeck[] = [];

/**
 * Shared skill-card deck state backed by the React Query cache.
 *
 * All hook instances observe the same `["list_skill_groups"]` cache entry, so
 * concurrent mounts dedupe into a single `list_skill_groups` invoke and
 * mutations stay visible to every consumer.
 */
export function useSkillCards() {
  const queryClient = useQueryClient();
  const query = useTauriQuery("list_skill_groups", {
    // Match the previous hand-rolled behavior: a single attempt per fetch and
    // no silent refetch on window focus.
    retry: false,
    refetchOnWindowFocus: false,
  });

  const { error } = query;
  useEffect(() => {
    if (error && import.meta.env.DEV) console.error("Failed to load skill cards:", error);
  }, [error]);

  const setGroups = useCallback(
    (updater: (prev: SkillCardDeck[]) => SkillCardDeck[]) => {
      queryClient.setQueryData<SkillCardDeck[]>(GROUPS_QUERY_KEY, (prev) => updater(prev ?? NO_GROUPS));
    },
    [queryClient],
  );

  const refresh = useCallback(async () => {
    await queryClient.refetchQueries({ queryKey: GROUPS_QUERY_KEY });
  }, [queryClient]);

  const createGroup = useCallback(
    async (
      name: string,
      description: string,
      icon: string,
      skills: string[],
      skillSources?: Record<string, string>,
    ) => {
      const group = await tauriInvoke("create_skill_group", {
        name,
        description,
        icon,
        skills,
        skillSources: skillSources || {},
      });
      setGroups((prev) => [...prev, group]);
      return group;
    },
    [setGroups],
  );

  const updateGroup = useCallback(
    async (
      id: string,
      updates: {
        name?: string;
        description?: string;
        icon?: string;
        skills?: string[];
        skillSources?: Record<string, string>;
        agentLinks?: string[];
      },
    ) => {
      const group = await tauriInvoke("update_skill_group", {
        id,
        ...updates,
      });
      setGroups((prev) => prev.map((g) => (g.id === id ? group : g)));
      return group;
    },
    [setGroups],
  );

  const deleteGroup = useCallback(
    async (id: string) => {
      await tauriInvoke("delete_skill_group", { id });
      setGroups((prev) => prev.filter((g) => g.id !== id));
    },
    [setGroups],
  );

  const duplicateGroup = useCallback(
    async (id: string) => {
      const group = await tauriInvoke("duplicate_skill_group", { id });
      setGroups((prev) => [...prev, group]);
      return group;
    },
    [setGroups],
  );

  const deployGroup = useCallback(async (groupId: string, projectPath: string, agentTypes: string[]) => {
    return await tauriInvoke("deploy_skill_group", {
      groupId,
      projectPath,
      agentTypes,
    });
  }, []);

  return {
    groups: query.data ?? NO_GROUPS,
    loading: query.isPending,
    refresh,
    createGroup,
    updateGroup,
    deleteGroup,
    duplicateGroup,
    deployGroup,
  };
}

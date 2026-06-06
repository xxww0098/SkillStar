import { useCallback, useEffect, useState } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import type { SkillCardDeck } from "../../../types";

export function useSkillCards() {
  const [groups, setGroups] = useState<SkillCardDeck[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const result = await tauriInvoke("list_skill_groups");
      setGroups(result);
    } catch (e) {
      if (import.meta.env.DEV) console.error("Failed to load skill cards:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

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
    [],
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
      },
    ) => {
      const group = await tauriInvoke("update_skill_group", {
        id,
        ...updates,
      });
      setGroups((prev) => prev.map((g) => (g.id === id ? group : g)));
      return group;
    },
    [],
  );

  const deleteGroup = useCallback(async (id: string) => {
    await tauriInvoke("delete_skill_group", { id });
    setGroups((prev) => prev.filter((g) => g.id !== id));
  }, []);

  const duplicateGroup = useCallback(async (id: string) => {
    const group = await tauriInvoke("duplicate_skill_group", { id });
    setGroups((prev) => [...prev, group]);
    return group;
  }, []);

  const deployGroup = useCallback(async (groupId: string, projectPath: string, agentTypes: string[]) => {
    return await tauriInvoke("deploy_skill_group", {
      groupId,
      projectPath,
      agentTypes,
    });
  }, []);

  return {
    groups,
    loading,
    refresh,
    createGroup,
    updateGroup,
    deleteGroup,
    duplicateGroup,
    deployGroup,
  };
}

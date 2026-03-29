import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AgentProfile } from "../types";

export function useAgentProfiles() {
  const [profiles, setProfiles] = useState<AgentProfile[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<AgentProfile[]>("list_agent_profiles");
      setProfiles(result);
    } catch (e) {
      console.error("Failed to load agent profiles:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const toggleProfile = useCallback(
    async (id: string) => {
      try {
        const newState = await invoke<boolean>("toggle_agent_profile", { id });
        setProfiles((prev) =>
          prev.map((p) => (p.id === id ? { ...p, enabled: newState } : p))
        );
        return newState;
      } catch (e) {
        throw new Error(String(e));
      }
    },
    []
  );

  const deploySkillsToProject = useCallback(
    async (projectPath: string, selectedSkills: string[], agentTypes: string[]) => {
      try {
        const count = await invoke<number>("create_project_skills", {
          projectPath,
          selectedSkills,
          agentTypes,
        });
        return count;
      } catch (e) {
        throw new Error(String(e));
      }
    },
    []
  );

  const unlinkAllFromAgent = useCallback(
    async (agentId: string) => {
      try {
        const removed = await invoke<number>("unlink_all_skills_from_agent", { agentId });
        setProfiles((prev) =>
          prev.map((p) => (p.id === agentId ? { ...p, synced_count: 0 } : p))
        );
        return removed;
      } catch (e) {
        throw new Error(String(e));
      }
    },
    []
  );

  return {
    profiles,
    loading,
    refresh,
    toggleProfile,
    deploySkillsToProject,
    unlinkAllFromAgent,
  };
}

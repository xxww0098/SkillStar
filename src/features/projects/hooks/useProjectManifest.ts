import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import type {
  ImportResult,
  ImportTarget,
  ProjectAgentDetection,
  ProjectEntry,
  ProjectScanResult,
  SkillsList,
} from "../../../types";

export function useProjectManifest() {
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [skillsList, setSkillsList] = useState<SkillsList | null>(null);
  const [loading, setLoading] = useState(false);

  const loadProjects = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<ProjectEntry[]>("list_projects");
      setProjects(result);
    } catch (e) {
      console.error("Failed to load projects:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  const registerProject = useCallback(async (projectPath: string): Promise<ProjectEntry> => {
    const entry = await invoke<ProjectEntry>("register_project", {
      projectPath,
    });
    const updated = await invoke<ProjectEntry[]>("list_projects");
    setProjects(updated);
    return updated.find((project) => project.path === entry.path) ?? entry;
  }, []);

  const loadProjectSkills = useCallback(async (name: string) => {
    try {
      const result = await invoke<SkillsList | null>("get_project_skills", {
        name,
      });
      setSkillsList(result);
      return result;
    } catch (e) {
      console.error("Failed to load project skills:", e);
      return null;
    }
  }, []);

  const saveAndSync = useCallback(async (projectPath: string, agents: Record<string, string[]>) => {
    const count = await invoke<number>("save_and_sync_project", {
      projectPath,
      agents,
    });
    const updated = await invoke<ProjectEntry[]>("list_projects");
    setProjects(updated);
    return count;
  }, []);

  const saveProjectSkillsList = useCallback(async (projectPath: string, agents: Record<string, string[]>) => {
    const result = await invoke<SkillsList>("save_project_skills_list", {
      projectPath,
      agents,
    });
    const updated = await invoke<ProjectEntry[]>("list_projects");
    setProjects(updated);
    return result;
  }, []);

  const removeProject = useCallback(async (name: string) => {
    await invoke("remove_project", { name });
    setProjects((prev) => prev.filter((p) => p.name !== name));
  }, []);

  const updateProjectPath = useCallback(async (name: string, newPath: string) => {
    const count = await invoke<number>("update_project_path", {
      name,
      newPath,
    });
    const updated = await invoke<ProjectEntry[]>("list_projects");
    setProjects(updated);
    return count;
  }, []);

  const scanProjectSkills = useCallback(async (projectPath: string): Promise<ProjectScanResult> => {
    return await invoke<ProjectScanResult>("scan_project_skills", {
      projectPath,
    });
  }, []);

  const rebuildProjectSkillsFromDisk = useCallback(async (projectPath: string): Promise<SkillsList> => {
    return await invoke<SkillsList>("rebuild_project_skills_from_disk", {
      projectPath,
    });
  }, []);

  const importProjectSkills = useCallback(
    async (projectPath: string, projectName: string, targets: ImportTarget[]): Promise<ImportResult> => {
      const result = await invoke<ImportResult>("import_project_skills", {
        projectPath,
        projectName,
        targets,
      });
      // Refresh projects list after import
      const updated = await invoke<ProjectEntry[]>("list_projects");
      setProjects(updated);
      return result;
    },
    [],
  );

  const detectProjectAgents = useCallback(async (projectPath: string): Promise<ProjectAgentDetection> => {
    return await invoke<ProjectAgentDetection>("detect_project_agents", {
      projectPath,
    });
  }, []);

  return {
    projects,
    skillsList,
    loading,
    loadProjects,
    registerProject,
    loadProjectSkills,
    saveAndSync,
    saveProjectSkillsList,
    updateProjectPath,
    removeProject,
    scanProjectSkills,
    rebuildProjectSkillsFromDisk,
    importProjectSkills,
    detectProjectAgents,
  };
}

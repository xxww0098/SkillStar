import { useCallback, useState } from "react";
import type { ImportDone, ScannedSkill } from "../../../types";

interface UseProjectSkillsParams {
  pathByAgentId: Map<string, string>;
  ownerBySharedPath: Map<string, string>;
  canonicalizeAgentsBySharedPath: (
    agents: Record<string, string[]>,
    forcedOwnerByPath?: Map<string, string>,
  ) => Record<string, string[]>;
  filterAgentsByEnabledProfiles: (agents: Record<string, string[]>) => Record<string, string[]>;
  filterUnmanagedByEnabledProfiles: (skills: ScannedSkill[]) => ScannedSkill[];
  importProjectSkills: (
    projectPath: string,
    projectName: string,
    targets: { name: string; agent_id: string }[],
  ) => Promise<import("../../../types").ImportResult>;
  loadProjectSkills: (name: string) => Promise<import("../../../types").SkillsList | null>;
  scanProjectSkills: (projectPath: string) => Promise<import("../../../types").ProjectScanResult>;
  /** Called to propagate agentSkills changes back to the host component */
  onAgentSkillsChange: (updater: (prev: Record<string, string[]>) => Record<string, string[]>) => void;
}

export function useProjectSkills({
  pathByAgentId,
  ownerBySharedPath,
  canonicalizeAgentsBySharedPath,
  filterAgentsByEnabledProfiles,
  filterUnmanagedByEnabledProfiles,
  importProjectSkills,
  loadProjectSkills,
  scanProjectSkills,
  onAgentSkillsChange,
}: UseProjectSkillsParams) {
  const [unmanagedSkills, setUnmanagedSkills] = useState<ScannedSkill[]>([]);
  const [importing, setImporting] = useState(false);
  const [importDone, setImportDone] = useState<ImportDone | null>(null);
  const [scanExpanded, setScanExpanded] = useState(false);

  const resetScanState = useCallback(() => {
    setUnmanagedSkills([]);
    setImportDone(null);
    setScanExpanded(false);
  }, []);

  const setUnmanagedAndMaybeExpand = useCallback((skills: ScannedSkill[]) => {
    setUnmanagedSkills(skills);
    if (skills.length > 0) setScanExpanded(true);
  }, []);

  const handleImportAll = useCallback(
    async (selectedProject: { path: string; name: string } | null) => {
      if (!selectedProject || unmanagedSkills.length === 0) return;
      setImporting(true);
      setImportDone(null);
      try {
        const dedupedTargets = new Map<string, { name: string; agent_id: string }>();
        for (const skill of unmanagedSkills) {
          const path = pathByAgentId.get(skill.agent_id) ?? skill.agent_id;
          const ownerAgentId = ownerBySharedPath.get(path) ?? skill.agent_id;
          const key = `${path}::${skill.name}`;
          if (!dedupedTargets.has(key)) {
            dedupedTargets.set(key, {
              name: skill.name,
              agent_id: ownerAgentId,
            });
          }
        }
        const targets = Array.from(dedupedTargets.values());
        const result = await importProjectSkills(selectedProject.path, selectedProject.name, targets);
        setImportDone({
          hub: result.imported_to_hub.length,
          links: result.symlink_count,
        });
        setUnmanagedSkills([]);

        // Reload project skills to reflect merged state
        const skills = await loadProjectSkills(selectedProject.name);
        if (skills) {
          const filteredAgents = filterAgentsByEnabledProfiles({ ...skills.agents });
          const preferredOwnerByPath = new Map(ownerBySharedPath);
          for (const agentId of Object.keys(filteredAgents)) {
            const path = pathByAgentId.get(agentId) ?? agentId;
            if (!preferredOwnerByPath.has(path)) {
              preferredOwnerByPath.set(path, agentId);
            }
          }
          onAgentSkillsChange(() => canonicalizeAgentsBySharedPath(filteredAgents, preferredOwnerByPath));
        }

        // Re-scan to confirm everything is clean
        const rescan = await scanProjectSkills(selectedProject.path);
        const remaining = filterUnmanagedByEnabledProfiles(rescan.skills);
        setUnmanagedSkills(remaining);

        setTimeout(() => setImportDone(null), 4000);
      } catch (e) {
        console.error("Import failed:", e);
      } finally {
        setImporting(false);
      }
    },
    [
      unmanagedSkills,
      filterAgentsByEnabledProfiles,
      filterUnmanagedByEnabledProfiles,
      importProjectSkills,
      loadProjectSkills,
      ownerBySharedPath,
      pathByAgentId,
      scanProjectSkills,
      canonicalizeAgentsBySharedPath,
      onAgentSkillsChange,
    ],
  );

  return {
    unmanagedSkills,
    setUnmanagedSkills,
    importing,
    importDone,
    scanExpanded,
    setScanExpanded,
    resetScanState,
    setUnmanagedAndMaybeExpand,
    handleImportAll,
  };
}

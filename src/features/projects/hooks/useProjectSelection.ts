import { useCallback } from "react";
import { tauriInvoke } from "../../../lib/ipc";
import { toast } from "../../../lib/toast";
import type { AgentProfile, ProjectDeployMode, ProjectEntry, ScannedSkill } from "../../../types";

interface UseProjectSelectionParams {
  enabledProfiles: AgentProfile[];
  enabledProfileIdSet: Set<string>;
  pathByAgentId: Map<string, string>;
  canonicalizeAgentsBySharedPath: (
    agents: Record<string, string[]>,
    forcedOwnerByPath?: Map<string, string>,
  ) => Record<string, string[]>;
  filterAgentsByEnabledProfiles: (agents: Record<string, string[]>) => Record<string, string[]>;
  filterUnmanagedByEnabledProfiles: (skills: ScannedSkill[]) => ScannedSkill[];
  buildSymlinkSkillIndex: (skills: ScannedSkill[]) => Record<string, string[]>;
  loadProjectSkills: (name: string) => Promise<import("../../../types").SkillsList | null>;
  scanProjectSkills: (projectPath: string) => Promise<import("../../../types").ProjectScanResult>;
  rebuildProjectSkillsFromDisk: (projectPath: string) => Promise<import("../../../types").SkillsList>;
  resetScanState: () => void;
  resetDisambigState: () => void;
  setScannedSymlinkSkillsByAgent: (index: Record<string, string[]>) => void;
  runAgentDetection: (
    projectPath: string,
    currentAgents: Record<string, string[]>,
    symlinkSkillsByAgent: Record<string, string[]>,
    selectedProject: { path: string } | null,
    suppressDisambiguationDialog?: boolean,
  ) => Promise<void>;
  setUnmanagedAndMaybeExpand: (skills: ScannedSkill[]) => void;
  /** Page-owned: applies the resolved project + agent map to on-screen state. */
  presentProjectState: (project: ProjectEntry, agents: Record<string, string[]>, isDirty?: boolean) => void;
  /** Page-owned: opens the "which agent(s) should receive these skills" dialog. */
  openDeployAgentDialog: (project: ProjectEntry, agents: Record<string, string[]>) => void;
  /** Page-owned: skills carried over from a SkillCards deploy, pending assignment. */
  pendingGroupSkills: string[] | null;
  /** Page-owned: hydrates per-agent deploy mode from the persisted (path-keyed) config. */
  setDeployModes: (modes: Record<string, ProjectDeployMode>) => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}

/**
 * Orchestrates the full project-switch flow: disk scan, symlink index
 * rebuild, the one-time self-heal (rebuild `skills-list.json` from disk when
 * it has skills-on-disk but empty config), shared-path canonicalization,
 * deploy-mode hydration, stale-copy refresh, and agent detection.
 *
 * Extracted verbatim from `Projects.tsx`'s `handleSelectProject` — see that
 * file's git history for the original inline version.
 */
export function useProjectSelection({
  enabledProfiles,
  enabledProfileIdSet,
  pathByAgentId,
  canonicalizeAgentsBySharedPath,
  filterAgentsByEnabledProfiles,
  filterUnmanagedByEnabledProfiles,
  buildSymlinkSkillIndex,
  loadProjectSkills,
  scanProjectSkills,
  rebuildProjectSkillsFromDisk,
  resetScanState,
  resetDisambigState,
  setScannedSymlinkSkillsByAgent,
  runAgentDetection,
  setUnmanagedAndMaybeExpand,
  presentProjectState,
  openDeployAgentDialog,
  pendingGroupSkills,
  setDeployModes,
  t,
}: UseProjectSelectionParams) {
  const handleSelectProject = useCallback(
    async (project: ProjectEntry) => {
      // Reset scan state
      resetScanState();
      resetDisambigState();

      const skills = await loadProjectSkills(project.name);
      let agentsFromConfig: Record<string, string[]> = skills
        ? filterAgentsByEnabledProfiles({ ...skills.agents })
        : {};

      // First scan happens immediately on project selection so we can hydrate
      // existing symlinked skills before agent detection/disambiguation.
      let scannedSkills: ScannedSkill[] = [];
      try {
        const firstScan = await scanProjectSkills(project.path);
        scannedSkills = firstScan.skills;
      } catch (e) {
        if (import.meta.env.DEV) console.error("Initial scan failed:", e);
        toast.error(String(e) || t("projects.scanFailed", { defaultValue: "Project scan failed" }));
      }

      const symlinkSkillsByAgent = buildSymlinkSkillIndex(scannedSkills);
      setScannedSymlinkSkillsByAgent(symlinkSkillsByAgent);

      const hasScannedProjectSkills = scannedSkills.some(
        (skill) => enabledProfileIdSet.has(skill.agent_id) && (skill.is_symlink || skill.has_skill_md),
      );
      const hasConfiguredSkills = Object.values(agentsFromConfig).some((skillNames) => skillNames.length > 0);

      // One-time self-heal: if disk has project skills but config is empty,
      // rebuild skills-list.json from project directories first.
      if (hasScannedProjectSkills && !hasConfiguredSkills) {
        try {
          const rebuilt = await rebuildProjectSkillsFromDisk(project.path);
          agentsFromConfig = filterAgentsByEnabledProfiles({
            ...rebuilt.agents,
          });
        } catch (e) {
          if (import.meta.env.DEV) console.error("Rebuild project skills from disk failed:", e);
        }
      }

      const preferredOwnerByPath = new Map<string, string>();
      for (const profile of enabledProfiles) {
        if (!(profile.id in agentsFromConfig)) continue;
        const path = pathByAgentId.get(profile.id) ?? profile.id;
        if (!preferredOwnerByPath.has(path)) {
          preferredOwnerByPath.set(path, profile.id);
        }
      }
      for (const profile of enabledProfiles) {
        const scanned = symlinkSkillsByAgent[profile.id] ?? [];
        if (scanned.length === 0) continue;
        const path = pathByAgentId.get(profile.id) ?? profile.id;
        if (!preferredOwnerByPath.has(path)) {
          preferredOwnerByPath.set(path, profile.id);
        }
      }

      let agents: Record<string, string[]> = canonicalizeAgentsBySharedPath(
        { ...agentsFromConfig },
        preferredOwnerByPath,
      );
      for (const profile of enabledProfiles) {
        const scanned = symlinkSkillsByAgent[profile.id] ?? [];
        if (scanned.length === 0) continue;
        const path = pathByAgentId.get(profile.id) ?? profile.id;
        const owner = preferredOwnerByPath.get(path) ?? profile.id;
        const current = agents[owner] ?? [];
        agents[owner] = [...new Set([...current, ...scanned])];
      }
      agents = canonicalizeAgentsBySharedPath(agents, preferredOwnerByPath);

      presentProjectState(project, agents, false);

      // Hydrate per-agent deploy mode from the persisted (path-keyed) config.
      // Absence means the default (symlink); only explicit entries are stored.
      const loadedDeployModes: Record<string, ProjectDeployMode> = {};
      if (skills?.deploy_modes) {
        for (const profile of enabledProfiles) {
          const path = pathByAgentId.get(profile.id) ?? profile.id;
          const mode = skills.deploy_modes[path];
          if (mode) loadedDeployModes[profile.id] = mode;
        }
      }
      setDeployModes(loadedDeployModes);

      // Refresh stale copy-deployed skills in background
      tauriInvoke("refresh_stale_project_copies", { projectPath: project.path }).catch((e) => {
        if (import.meta.env.DEV) console.warn("Stale copy refresh failed:", e);
      });

      if (pendingGroupSkills && pendingGroupSkills.length > 0) {
        openDeployAgentDialog(project, agents);
      }

      // Run agent detection for the project
      await runAgentDetection(
        project.path,
        agents,
        symlinkSkillsByAgent,
        project,
        Boolean(pendingGroupSkills && pendingGroupSkills.length > 0),
      );

      const unmanaged = filterUnmanagedByEnabledProfiles(scannedSkills);
      setUnmanagedAndMaybeExpand(unmanaged);
    },
    [
      buildSymlinkSkillIndex,
      enabledProfileIdSet,
      enabledProfiles,
      filterAgentsByEnabledProfiles,
      filterUnmanagedByEnabledProfiles,
      loadProjectSkills,
      openDeployAgentDialog,
      pathByAgentId,
      pendingGroupSkills,
      presentProjectState,
      rebuildProjectSkillsFromDisk,
      resetScanState,
      resetDisambigState,
      runAgentDetection,
      scanProjectSkills,
      setScannedSymlinkSkillsByAgent,
      setUnmanagedAndMaybeExpand,
      canonicalizeAgentsBySharedPath,
      setDeployModes,
      t,
    ],
  );

  return { handleSelectProject };
}

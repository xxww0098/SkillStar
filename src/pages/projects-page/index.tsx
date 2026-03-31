import { useState, useEffect, useMemo, useCallback, lazy, Suspense } from "react";
import { useTranslation } from "react-i18next";
import { Plus } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "../../components/ui/button";
import { Badge } from "../../components/ui/badge";
import { LoadingLogo } from "../../components/ui/LoadingLogo";
import { ProjectDeployAgentDialog } from "../../components/skills/ProjectDeployAgentDialog";
import { AgentDisambiguationDialog } from "../../components/skills/AgentDisambiguationDialog";
import { useProjectManifest } from "../../hooks/useProjectManifest";
import { useSkills } from "../../hooks/useSkills";
import { useAgentProfiles } from "../../hooks/useAgentProfiles";
import type { ProjectEntry, ScannedSkill, AmbiguousGroup, DetectedAgent, Skill } from "../../types";
import { DeployBanner } from "./DeployBanner";
import { ProjectListPanel } from "./ProjectListPanel";
import { ProjectDetailPanel } from "./ProjectDetailPanel";

interface ProjectsProps {
  preSelectedSkills?: string[] | null;
  onClearPreSelected?: () => void;
}

const DetailPanel = lazy(() =>
  import("../../components/layout/DetailPanel").then((mod) => ({
    default: mod.DetailPanel,
  }))
);

export function Projects({
  preSelectedSkills,
  onClearPreSelected,
}: ProjectsProps) {
  const { t } = useTranslation();
  const {
    projects, loadProjects, registerProject, loadProjectSkills, saveAndSync,
    saveProjectSkillsList,
    updateProjectPath, removeProject, scanProjectSkills, importProjectSkills,
    detectProjectAgents, rebuildProjectSkillsFromDisk,
  } = useProjectManifest();
  const {
    skills: hubSkills,
    installSkill,
    updateSkill,
    uninstallSkill,
    readSkillContent,
    updateSkillContent,
  } = useSkills();
  const { profiles } = useAgentProfiles();
  const enabledProfiles = useMemo(
    () => profiles.filter((profile) => profile.enabled && profile.project_skills_rel && profile.id !== "openclaw"),
    [profiles]
  );
  const enabledProfileIdSet = useMemo(
    () => new Set(enabledProfiles.map((profile) => profile.id)),
    [enabledProfiles]
  );
  const enabledProfilesById = useMemo(
    () => new Map(enabledProfiles.map((profile) => [profile.id, profile])),
    [enabledProfiles]
  );
  const pathByAgentId = useMemo(() => {
    const map = new Map<string, string>();
    for (const profile of enabledProfiles) {
      map.set(profile.id, profile.project_skills_rel || profile.id);
    }
    return map;
  }, [enabledProfiles]);
  const agentIdsByPath = useMemo(() => {
    const map = new Map<string, string[]>();
    for (const profile of enabledProfiles) {
      const path = profile.project_skills_rel || profile.id;
      const current = map.get(path) ?? [];
      current.push(profile.id);
      map.set(path, current);
    }
    return map;
  }, [enabledProfiles]);
  const conflictAgentIdsByAgent = useMemo(() => {
    const map = new Map<string, Set<string>>();
    for (const ids of agentIdsByPath.values()) {
      if (ids.length <= 1) continue;
      const set = new Set(ids);
      for (const id of ids) {
        map.set(id, set);
      }
    }
    return map;
  }, [agentIdsByPath]);
  const canonicalizeAgentsBySharedPath = useCallback(
    (
      agents: Record<string, string[]>,
      forcedOwnerByPath?: Map<string, string>
    ): Record<string, string[]> => {
      const inputByAgent = new Map<string, string[]>();
      for (const [agentId, skills] of Object.entries(agents)) {
        if (!enabledProfileIdSet.has(agentId)) continue;
        inputByAgent.set(agentId, [...new Set((skills ?? []).filter(Boolean))]);
      }

      const next: Record<string, string[]> = {};
      for (const [path, ids] of agentIdsByPath.entries()) {
        const activeIds = ids.filter((id) => inputByAgent.has(id));
        if (activeIds.length === 0) continue;

        const forcedOwner = forcedOwnerByPath?.get(path);
        const owner =
          forcedOwner && activeIds.includes(forcedOwner) ? forcedOwner : activeIds[0];

        next[owner] = [
          ...new Set(activeIds.flatMap((id) => inputByAgent.get(id) ?? [])),
        ];
      }
      return next;
    },
    [agentIdsByPath, enabledProfileIdSet]
  );

  const [selectedProject, setSelectedProject] = useState<ProjectEntry | null>(null);
  const [agentSkills, setAgentSkills] = useState<Record<string, string[]>>({});
  const [expandedAgent, setExpandedAgent] = useState<string | null>(null);
  const [skillFilter, setSkillFilter] = useState("");
  const [projectFilter, setProjectFilter] = useState("");
  const [saving, setSaving] = useState(false);
  const [syncResult, setSyncResult] = useState<number | null>(null);
  const [dirty, setDirty] = useState(false);
  const [pendingGroupSkills, setPendingGroupSkills] = useState<string[] | null>(null);
  const [deployTargetProject, setDeployTargetProject] = useState<ProjectEntry | null>(null);
  const [deployDialogOpen, setDeployDialogOpen] = useState(false);
  const [deployDialogInitialAgents, setDeployDialogInitialAgents] = useState<string[]>([]);

  // Scan & Import state
  const [unmanagedSkills, setUnmanagedSkills] = useState<ScannedSkill[]>([]);
  const [importing, setImporting] = useState(false);
  const [importDone, setImportDone] = useState<{ hub: number; links: number } | null>(null);
  const [scanExpanded, setScanExpanded] = useState(false);

  // Agent disambiguation state
  const [disambigOpen, setDisambigOpen] = useState(false);
  const [disambigGroup, setDisambigGroup] = useState<AmbiguousGroup | null>(null);
  const [disambigCandidates, setDisambigCandidates] = useState<DetectedAgent[]>([]);
  const [disambigQueue, setDisambigQueue] = useState<AmbiguousGroup[]>([]);
  const [scannedSymlinkSkillsByAgent, setScannedSymlinkSkillsByAgent] = useState<
    Record<string, string[]>
  >({});
  const [detailSkillName, setDetailSkillName] = useState<string | null>(null);
  const ownerBySharedPath = useMemo(() => {
    const map = new Map<string, string>();
    for (const agentId of Object.keys(agentSkills)) {
      if (!enabledProfileIdSet.has(agentId)) continue;
      const path = pathByAgentId.get(agentId) ?? agentId;
      if (!map.has(path)) {
        map.set(path, agentId);
      }
    }
    return map;
  }, [agentSkills, enabledProfileIdSet, pathByAgentId]);

  const selectedDetailSkill = useMemo(() => {
    const target = detailSkillName?.trim();
    if (!target) return null;

    const exact = hubSkills.find((skill) => skill.name === target);
    if (exact) return exact;

    const caseInsensitive = hubSkills.find(
      (skill) => skill.name.toLowerCase() === target.toLowerCase()
    );
    if (caseInsensitive) return caseInsensitive;

    // Fallback: keep detail panel open even when the skill list snapshot
    // is temporarily stale/missing this project-linked skill.
    const fallbackSkill: Skill = {
      name: target,
      description: "",
      skill_type: "hub",
      stars: 0,
      installed: true,
      update_available: false,
      last_updated: "",
      git_url: "",
      tree_hash: null,
      category: "None",
      author: null,
      topics: [],
    };
    return fallbackSkill;
  }, [detailSkillName, hubSkills]);

  const handleOpenSkillDetail = useCallback((skillName: string) => {
    const normalized = skillName.trim();
    if (!normalized) return;
    setDetailSkillName(normalized);
  }, []);

  const handleCloseSkillDetail = useCallback(() => {
    setDetailSkillName(null);
  }, []);

  const handleDetailInstall = useCallback(
    async (url: string, name: string) => {
      try {
        await installSkill(url, name);
      } catch (e) {
        console.error("Install from detail panel failed:", e);
      }
    },
    [installSkill]
  );

  const handleDetailUpdate = useCallback(
    async (name: string) => {
      try {
        await updateSkill(name);
      } catch (e) {
        console.error("Update from detail panel failed:", e);
      }
    },
    [updateSkill]
  );

  const handleDetailUninstall = useCallback(
    async (name: string) => {
      try {
        await uninstallSkill(name);
        setDetailSkillName((current) => (current === name ? null : current));
      } catch (e) {
        console.error("Uninstall from detail panel failed:", e);
      }
    },
    [uninstallSkill]
  );

  useEffect(() => {
    loadProjects();
  }, [loadProjects]);

  // Absorb pre-selected skills from SkillCards deploy
  useEffect(() => {
    if (preSelectedSkills && preSelectedSkills.length > 0) {
      setPendingGroupSkills([...preSelectedSkills]);
      onClearPreSelected?.();
    }
  }, [preSelectedSkills, onClearPreSelected]);

  const filterAgentsByEnabledProfiles = useCallback(
    (agents: Record<string, string[]>) =>
      Object.fromEntries(
        Object.entries(agents).filter(([agentId]) => enabledProfileIdSet.has(agentId))
      ),
    [enabledProfileIdSet]
  );

  const filterUnmanagedByEnabledProfiles = useCallback(
    (skills: ScannedSkill[]) => {
      const deduped = new Map<string, ScannedSkill>();
      for (const skill of skills) {
        if (
          !enabledProfileIdSet.has(skill.agent_id) ||
          skill.is_symlink ||
          !skill.has_skill_md
        ) {
          continue;
        }
        const path = pathByAgentId.get(skill.agent_id) ?? skill.agent_id;
        const key = `${path}::${skill.name}`;
        if (!deduped.has(key)) {
          deduped.set(key, skill);
        }
      }
      return Array.from(deduped.values());
    },
    [enabledProfileIdSet, pathByAgentId]
  );

  const buildSymlinkSkillIndex = useCallback(
    (skills: ScannedSkill[]): Record<string, string[]> => {
      const index: Record<string, string[]> = {};
      for (const skill of skills) {
        if (!enabledProfileIdSet.has(skill.agent_id) || !skill.is_symlink) continue;
        const name = skill.name.trim();
        if (!name) continue;

        const current = index[skill.agent_id] ?? [];
        if (!current.includes(name)) {
          index[skill.agent_id] = [...current, name];
        }
      }
      return index;
    },
    [enabledProfileIdSet]
  );

  const suggestDeployAgentIds = useCallback(
    (agents: Record<string, string[]>) => {
      const currentIds = Object.keys(agents).filter((id) =>
        enabledProfileIdSet.has(id)
      );
      if (currentIds.length > 0) return currentIds;
      const first = enabledProfiles[0];
      return first ? [first.id] : [];
    },
    [enabledProfileIdSet, enabledProfiles]
  );

  const mergePendingSkillsIntoAgents = useCallback(
    (
      agents: Record<string, string[]>,
      targetAgentIds: string[],
      skillNames: string[]
    ): Record<string, string[]> => {
      if (targetAgentIds.length === 0 || skillNames.length === 0) return agents;

      const next = { ...agents };
      for (const agentId of [...new Set(targetAgentIds)]) {
        next[agentId] = [...new Set([...(next[agentId] ?? []), ...skillNames])];
      }
      return next;
    },
    []
  );

  const presentProjectState = useCallback(
    (project: ProjectEntry, agents: Record<string, string[]>, isDirty = false) => {
      setSelectedProject(project);
      setSyncResult(null);
      setSkillFilter("");

      setAgentSkills(agents);
      setExpandedAgent(Object.keys(agents)[0] ?? null);
      setDirty(isDirty);
    },
    []
  );

  const openDeployAgentDialog = useCallback(
    (project: ProjectEntry, agents: Record<string, string[]>) => {
      setDeployTargetProject(project);
      setDeployDialogInitialAgents(suggestDeployAgentIds(agents));
      setDeployDialogOpen(true);
    },
    [suggestDeployAgentIds]
  );

  // ── Agent detection flow ───────────────────────────────────────────

  const runAgentDetection = useCallback(
    async (
      projectPath: string,
      currentAgents: Record<string, string[]>,
      symlinkSkillsByAgent: Record<string, string[]>,
      suppressDisambiguationDialog = false
    ) => {
      try {
        const detection = await detectProjectAgents(projectPath);

        // Skip detection if the project already has configured agents
        const hasExistingConfig = Object.keys(currentAgents).length > 0;
        if (hasExistingConfig) return;

        const autoEnable = new Set(
          detection.auto_enable.filter((agentId) =>
            enabledProfileIdSet.has(agentId)
          )
        );

        // Queue ambiguous groups for disambiguation.
        // Only include agents that are enabled in Settings.
        const relevantGroups: AmbiguousGroup[] = [];
        for (const group of detection.ambiguous_groups) {
          const enabledAgentIds = group.agent_ids.filter((id) =>
            enabledProfileIdSet.has(id)
          );

          if (enabledAgentIds.length > 1) {
            relevantGroups.push({
              path: group.path,
              agent_ids: enabledAgentIds,
              agent_names: group.agent_names.filter((_, index) =>
                enabledProfileIdSet.has(group.agent_ids[index])
              ),
            });
            continue;
          }

          if (enabledAgentIds.length === 1) {
            autoEnable.add(enabledAgentIds[0]);
          }
        }

        // Auto-enable agents with unique detected paths
        if (autoEnable.size > 0) {
          setAgentSkills((prev) => {
            const next = { ...prev };
            for (const agentId of autoEnable) {
              const current = next[agentId] ?? [];
              const scanned = symlinkSkillsByAgent[agentId] ?? [];
              const merged = [...new Set([...current, ...scanned])];
              next[agentId] = merged;
            }
            return canonicalizeAgentsBySharedPath(next);
          });
          setDirty(true);
        }

        if (!suppressDisambiguationDialog && relevantGroups.length > 0) {
          const enabledDetected = detection.detected.filter((agent) =>
            enabledProfileIdSet.has(agent.agent_id)
          );
          setDisambigCandidates(enabledDetected);
          // Show the first group immediately, queue the rest
          setDisambigGroup(relevantGroups[0]);
          setDisambigQueue(relevantGroups.slice(1));
          setDisambigOpen(true);
        }
      } catch (e) {
        console.error("Agent detection failed:", e);
      }
    },
    [detectProjectAgents, enabledProfileIdSet, canonicalizeAgentsBySharedPath]
  );

  const handleDisambigConfirm = useCallback(
    async (selectedAgentId: string) => {
      const forcedOwnerByPath = new Map<string, string>();
      const selectedPath = pathByAgentId.get(selectedAgentId);
      if (selectedPath) {
        forcedOwnerByPath.set(selectedPath, selectedAgentId);
      }

      // Hydrate from the whole conflict group (shared path), not just the
      // selected agent id. This keeps rendering stable even if scan attribution
      // lands on another agent in the same shared directory group.
      const conflictAgentIds = disambigGroup?.agent_ids?.length
        ? disambigGroup.agent_ids
        : [selectedAgentId];
      let nextAgents = {
        ...agentSkills,
      };
      for (const conflictAgentId of conflictAgentIds) {
        if (conflictAgentId !== selectedAgentId) {
          delete nextAgents[conflictAgentId];
        }
      }
      if (enabledProfileIdSet.has(selectedAgentId) && !(selectedAgentId in nextAgents)) {
        nextAgents[selectedAgentId] = [];
      }
      nextAgents = canonicalizeAgentsBySharedPath(nextAgents, forcedOwnerByPath);

      const preScannedSkills = [
        ...new Set(
          conflictAgentIds.flatMap(
            (agentId) => scannedSymlinkSkillsByAgent[agentId] ?? []
          )
        ),
      ];
      if (preScannedSkills.length > 0) {
        const current = nextAgents[selectedAgentId] ?? [];
        const merged = [...new Set([...current, ...preScannedSkills])];
        if (merged.length !== current.length) {
          nextAgents = {
            ...nextAgents,
            [selectedAgentId]: merged,
          };
          nextAgents = canonicalizeAgentsBySharedPath(
            nextAgents,
            forcedOwnerByPath
          );
        }
      }
      setAgentSkills(nextAgents);
      setExpandedAgent(selectedAgentId);
      setDirty(true);

      // "Confirm" in disambiguation should persist immediately, but avoid
      // destructive full-sync behavior for shared-path resolution.
      if (selectedProject) {
        setSaving(true);
        try {
          await saveProjectSkillsList(
            selectedProject.path,
            filterAgentsByEnabledProfiles(nextAgents)
          );
          setDirty(false);
          loadProjects();
        } catch (e) {
          console.error("Auto-persist after disambiguation failed:", e);
        } finally {
          setSaving(false);
        }
      }

      // Process next group in queue, or close
      if (disambigQueue.length > 0) {
        setDisambigGroup(disambigQueue[0]);
        setDisambigQueue((prev) => prev.slice(1));
      } else {
        setDisambigOpen(false);
        setDisambigGroup(null);
        setDisambigCandidates([]);
      }
    },
    [
      agentSkills,
      enabledProfileIdSet,
      disambigGroup,
      disambigQueue,
      scannedSymlinkSkillsByAgent,
      selectedProject,
      saveProjectSkillsList,
      filterAgentsByEnabledProfiles,
      canonicalizeAgentsBySharedPath,
      pathByAgentId,
      loadProjects,
    ]
  );

  const handleDisambigClose = useCallback(() => {
    setDisambigOpen(false);
    setDisambigGroup(null);
    setDisambigQueue([]);
    setDisambigCandidates([]);
  }, []);

  // ── Project selection ─────────────────────────────────────────────

  const handleSelectProject = useCallback(
    async (project: ProjectEntry) => {
      // Reset scan state
      setDetailSkillName(null);
      setUnmanagedSkills([]);
      setImportDone(null);
      setScanExpanded(false);
      setDisambigOpen(false);
      setDisambigGroup(null);
      setDisambigCandidates([]);
      setDisambigQueue([]);

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
        console.error("Initial scan failed:", e);
      }

      const symlinkSkillsByAgent = buildSymlinkSkillIndex(scannedSkills);
      setScannedSymlinkSkillsByAgent(symlinkSkillsByAgent);

      const hasScannedProjectSkills = scannedSkills.some(
        (skill) =>
          enabledProfileIdSet.has(skill.agent_id) &&
          (skill.is_symlink || skill.has_skill_md)
      );
      const hasConfiguredSkills = Object.values(agentsFromConfig).some(
        (skillNames) => skillNames.length > 0
      );

      // One-time self-heal: if disk has project skills but config is empty,
      // rebuild skills-list.json from project directories first.
      if (hasScannedProjectSkills && !hasConfiguredSkills) {
        try {
          const rebuilt = await rebuildProjectSkillsFromDisk(project.path);
          agentsFromConfig = filterAgentsByEnabledProfiles({
            ...rebuilt.agents,
          });
        } catch (e) {
          console.error("Rebuild project skills from disk failed:", e);
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
        preferredOwnerByPath
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

      presentProjectState(project, agents);

      if (pendingGroupSkills && pendingGroupSkills.length > 0) {
        openDeployAgentDialog(project, agents);
      }

      // Run agent detection for the project
      await runAgentDetection(
        project.path,
        agents,
        symlinkSkillsByAgent,
        Boolean(pendingGroupSkills && pendingGroupSkills.length > 0)
      );

      const unmanaged = filterUnmanagedByEnabledProfiles(scannedSkills);
      setUnmanagedSkills(unmanaged);
      if (unmanaged.length > 0) setScanExpanded(true);
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
      runAgentDetection,
      scanProjectSkills,
      canonicalizeAgentsBySharedPath,
    ]
  );

  const handleOpenFolder = useCallback(async () => {
    const path = await open({ directory: true, title: t("projects.chooseDir") });
    if (!path) return;
    const projectPath = path as string;
    const existing = projects.find((p) => p.path === projectPath);
    if (existing) {
      await handleSelectProject(existing);
      return;
    }

    try {
      const entry = await registerProject(projectPath);
      await handleSelectProject(entry);
    } catch (e) {
      console.error("Register project failed:", e);
    }
  }, [
    projects,
    handleSelectProject,
    registerProject,
  ]);

  const handleCloseDeployDialog = useCallback(() => {
    setDeployDialogOpen(false);
    setDeployTargetProject(null);
  }, []);

  const handleConfirmDeployAgents = useCallback(
    (selectedAgentIds: string[]) => {
      if (!pendingGroupSkills || pendingGroupSkills.length === 0 || !deployTargetProject) {
        handleCloseDeployDialog();
        return;
      }
      const allowedAgentIds = selectedAgentIds.filter((id) =>
        enabledProfileIdSet.has(id)
      );
      if (allowedAgentIds.length === 0) {
        handleCloseDeployDialog();
        return;
      }

      setAgentSkills((prev) =>
        canonicalizeAgentsBySharedPath(
          mergePendingSkillsIntoAgents(prev, allowedAgentIds, pendingGroupSkills)
        )
      );
      setExpandedAgent(allowedAgentIds[0] ?? null);
      setDirty(true);
      setPendingGroupSkills(null);
      setSkillFilter("");

      handleCloseDeployDialog();
    },
    [
      deployTargetProject,
      enabledProfileIdSet,
      handleCloseDeployDialog,
      mergePendingSkillsIntoAgents,
      canonicalizeAgentsBySharedPath,
      pendingGroupSkills,
    ]
  );

  const handleToggleAgent = useCallback(
    (agentId: string) => {
      if (!enabledProfileIdSet.has(agentId)) return;
      setAgentSkills((prev) => {
        const next = { ...prev };
        if (next[agentId]) {
          // Turning OFF → remove agent, collapse if expanded
          delete next[agentId];
          if (expandedAgent === agentId) {
            setExpandedAgent(null);
          }
        } else {
          const conflictGroup = conflictAgentIdsByAgent.get(agentId);
          const inherited = conflictGroup
            ? [
                ...new Set(
                  Array.from(conflictGroup).flatMap(
                    (conflictAgentId) => next[conflictAgentId] ?? []
                  )
                ),
              ]
            : [];
          if (conflictGroup) {
            for (const conflictAgentId of conflictGroup) {
              if (conflictAgentId !== agentId) {
                delete next[conflictAgentId];
              }
            }
          }
          // Turning ON → add agent with empty skills, auto-expand (exclusive)
          next[agentId] = [...new Set([...(next[agentId] ?? []), ...inherited])];
          setExpandedAgent(agentId);
        }
        return canonicalizeAgentsBySharedPath(next);
      });
      setDirty(true);
    },
    [
      enabledProfileIdSet,
      expandedAgent,
      conflictAgentIdsByAgent,
      canonicalizeAgentsBySharedPath,
    ]
  );

  const handleToggleExpand = useCallback(
    (agentId: string) => {
      // Only enabled agents can be expanded; exclusive accordion
      const isEnabled = agentId in agentSkills;
      if (!isEnabled) return;
      setExpandedAgent((prev) => {
        const next = prev === agentId ? null : agentId;
        return next;
      });
      setSkillFilter("");
    },
    [agentSkills]
  );

  const handleAddSkill = useCallback(
    (agentId: string, skillName: string) => {
      setAgentSkills((prev) => {
        const current = prev[agentId] ?? [];
        if (current.includes(skillName)) return prev;
        return { ...prev, [agentId]: [...current, skillName] };
      });
      setDirty(true);
    },
    []
  );

  const handleAddAllSkills = useCallback(
    (agentId: string, skillNames: string[]) => {
      setAgentSkills((prev) => {
        const current = prev[agentId] ?? [];
        const newSkills = skillNames.filter(name => !current.includes(name));
        if (newSkills.length === 0) return prev;
        return { ...prev, [agentId]: [...current, ...newSkills] };
      });
      setDirty(true);
    },
    []
  );

  const handleRemoveAllSkills = useCallback(
    (agentId: string) => {
      setAgentSkills((prev) => {
        if (!prev[agentId] || prev[agentId].length === 0) return prev;
        return { ...prev, [agentId]: [] };
      });
      setDirty(true);
    },
    []
  );

  const handleRemoveSkill = useCallback((agentId: string, skillName: string) => {
    setAgentSkills((prev) => ({
      ...prev,
      [agentId]: (prev[agentId] ?? []).filter((s) => s !== skillName),
    }));
    setDirty(true);
  }, []);

  const handleApply = useCallback(async () => {
    if (!selectedProject) return;
    setSaving(true);
    setSyncResult(null);
    try {
      const count = await saveAndSync(
        selectedProject.path,
        filterAgentsByEnabledProfiles(agentSkills)
      );
      setSyncResult(count);
      setDirty(false);
      loadProjects();
      setTimeout(() => setSyncResult(null), 4000);
    } catch (e) {
      console.error("Save and sync failed:", e);
    } finally {
      setSaving(false);
    }
  }, [
    selectedProject,
    agentSkills,
    filterAgentsByEnabledProfiles,
    saveAndSync,
    loadProjects,
  ]);

  const handleRemoveProject = useCallback(
    async (e: React.MouseEvent, name: string) => {
      e.stopPropagation();
      try {
        await removeProject(name);
        if (selectedProject?.name === name) {
          setSelectedProject(null);
          setAgentSkills({});
          setExpandedAgent(null);
    
          setDirty(false);
        }
      } catch (e) {
        console.error("Remove failed:", e);
      }
    },
    [removeProject, selectedProject]
  );

  const handleRelinkPath = useCallback(async () => {
    if (!selectedProject) return;
    const path = await open({ directory: true, title: t("projects.chooseNewDir") });
    if (!path) return;
    try {
      await updateProjectPath(selectedProject.name, path as string);
      setSelectedProject((prev) => prev ? { ...prev, path: path as string } : null);
    } catch (e) {
      console.error("Relink failed:", e);
    }
  }, [selectedProject, updateProjectPath]);

  const getAvailableSkills = useCallback(
    (agentId: string) => {
      const current = agentSkills[agentId] ?? [];
      return hubSkills.filter(
        (s) =>
          s.installed &&
          !current.includes(s.name) &&
          (!skillFilter || s.name.toLowerCase().includes(skillFilter.toLowerCase()))
      );
    },
    [hubSkills, agentSkills, skillFilter]
  );

  const handleImportAll = useCallback(async () => {
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
      const result = await importProjectSkills(
        selectedProject.path,
        selectedProject.name,
        targets
      );
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
        setAgentSkills(
          canonicalizeAgentsBySharedPath(filteredAgents, preferredOwnerByPath)
        );
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
  }, [
    selectedProject,
    unmanagedSkills,
    filterAgentsByEnabledProfiles,
    filterUnmanagedByEnabledProfiles,
    importProjectSkills,
    loadProjectSkills,
    ownerBySharedPath,
    pathByAgentId,
    scanProjectSkills,
    canonicalizeAgentsBySharedPath,
  ]);

  const enabledAgents = useMemo(
    () => Object.keys(agentSkills).filter((agentId) => enabledProfileIdSet.has(agentId)),
    [agentSkills, enabledProfileIdSet]
  );
  const totalSkills = useMemo(
    () =>
      enabledAgents.reduce(
        (sum, agentId) => sum + (agentSkills[agentId]?.length ?? 0),
        0
      ),
    [enabledAgents, agentSkills]
  );

  const filteredProjects = useMemo(() => {
    if (!projectFilter) return projects;
    const normalizedProjectFilter = projectFilter.toLowerCase();
    return projects.filter(
      (project) =>
        project.name.toLowerCase().includes(normalizedProjectFilter) ||
        project.path.toLowerCase().includes(normalizedProjectFilter)
    );
  }, [projects, projectFilter]);



  return (
    <div className="flex-1 flex flex-col overflow-hidden relative">
      <div className="h-14 flex items-center justify-between px-6 border-b border-border bg-card/30 backdrop-blur-sm">
        <div className="flex items-center gap-3">
          <h1>{t("sidebar.projects")}</h1>
          {projects.length > 0 && (
            <Badge variant="outline">{t("projects.projectsCount", { count: projects.length })}</Badge>
          )}
        </div>
        <Button size="sm" onClick={handleOpenFolder}>
          <Plus className="w-3.5 h-3.5" />
          {t("projects.registerProject")}
        </Button>
      </div>

      <DeployBanner
        pendingGroupSkills={pendingGroupSkills}
        onDismiss={() => setPendingGroupSkills(null)}
      />

      <div className="flex-1 flex overflow-hidden">
        <ProjectListPanel
          filteredProjects={filteredProjects}
          selectedProject={selectedProject}
          projectFilter={projectFilter}
          onProjectFilterChange={setProjectFilter}
          onSelectProject={handleSelectProject}
          onRemoveProject={handleRemoveProject}
          onOpenFolder={handleOpenFolder}
        />

        <div className="flex-1 flex flex-col overflow-hidden">
          <ProjectDetailPanel
            selectedProject={selectedProject}
            onRelinkPath={handleRelinkPath}
            unmanagedSkills={unmanagedSkills}
            scanExpanded={scanExpanded}
            importing={importing}
            importDone={importDone}
            enabledProfilesById={enabledProfilesById}
            enabledProfiles={enabledProfiles}
            enabledAgents={enabledAgents}
            expandedAgent={expandedAgent}
            agentSkills={agentSkills}
            skillFilter={skillFilter}
            totalSkills={totalSkills}
            syncResult={syncResult}
            saving={saving}
            dirty={dirty}
            getAvailableSkills={getAvailableSkills}
            onToggleScanExpanded={() => setScanExpanded((value) => !value)}
            onImportAll={handleImportAll}
            onToggleExpand={handleToggleExpand}
            onToggleAgent={handleToggleAgent}
            onNavigateToSkill={handleOpenSkillDetail}
            onRemoveSkill={handleRemoveSkill}
            onSkillFilterChange={setSkillFilter}
            onAddSkill={handleAddSkill}
            onAddAllSkills={handleAddAllSkills}
            onRemoveAllSkills={handleRemoveAllSkills}
            onApply={handleApply}
          />
        </div>
      </div>

      <ProjectDeployAgentDialog
        open={deployDialogOpen}
        project={deployTargetProject}
        skillNames={pendingGroupSkills ?? []}
        profiles={enabledProfiles}
        initialSelectedAgentIds={deployDialogInitialAgents}
        onClose={handleCloseDeployDialog}
        onConfirm={handleConfirmDeployAgents}
      />

      <AgentDisambiguationDialog
        open={disambigOpen}
        group={disambigGroup}
        allDetected={disambigCandidates}
        onClose={handleDisambigClose}
        onConfirm={handleDisambigConfirm}
      />

      {selectedDetailSkill && (
        <Suspense
          fallback={
            <div className="absolute right-0 top-0 bottom-0 w-[400px] h-full border-l border-white/10 bg-card/60 backdrop-blur-xl shadow-2xl overflow-y-auto z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
              <LoadingLogo size="sm" />
            </div>
          }
        >
          <DetailPanel
            skill={selectedDetailSkill}
            onClose={handleCloseSkillDetail}
            onInstall={handleDetailInstall}
            onUpdate={handleDetailUpdate}
            onUninstall={handleDetailUninstall}
            onReadContent={readSkillContent}
            onSaveContent={updateSkillContent}
          />
        </Suspense>
      )}
    </div>
  );
}

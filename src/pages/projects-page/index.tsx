import { useState, useEffect, useMemo, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Plus } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "../../components/ui/button";
import { Badge } from "../../components/ui/badge";
import { ProjectDeployAgentDialog } from "../../components/skills/ProjectDeployAgentDialog";
import { useProjectManifest } from "../../hooks/useProjectManifest";
import { useSkills } from "../../hooks/useSkills";
import { useAgentProfiles } from "../../hooks/useAgentProfiles";
import type { ProjectEntry, ScannedSkill } from "../../types";
import { DeployBanner } from "./DeployBanner";
import { ProjectListPanel } from "./ProjectListPanel";
import { ProjectDetailPanel } from "./ProjectDetailPanel";

interface ProjectsProps {
  preSelectedSkills?: string[] | null;
  onClearPreSelected?: () => void;
  onNavigateToSkill?: (skillName: string) => void;
}

export function Projects({
  preSelectedSkills,
  onClearPreSelected,
  onNavigateToSkill,
}: ProjectsProps) {
  const { t } = useTranslation();
  const {
    projects, loadProjects, registerProject, loadProjectSkills, saveAndSync,
    updateProjectPath, removeProject, scanProjectSkills, importProjectSkills,
  } = useProjectManifest();
  const { skills: hubSkills } = useSkills();
  const { profiles } = useAgentProfiles();
  const enabledProfiles = useMemo(
    () => profiles.filter((profile) => profile.enabled),
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
    (skills: ScannedSkill[]) =>
      skills.filter(
        (skill) => enabledProfileIdSet.has(skill.agent_id) && !skill.is_symlink
      ),
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

  const handleSelectProject = useCallback(
    async (project: ProjectEntry) => {
      // Reset scan state
      setUnmanagedSkills([]);
      setImportDone(null);
      setScanExpanded(false);

      const skills = await loadProjectSkills(project.name);
      const agents: Record<string, string[]> = skills
        ? filterAgentsByEnabledProfiles({ ...skills.agents })
        : {};
      presentProjectState(project, agents);

      if (pendingGroupSkills && pendingGroupSkills.length > 0) {
        openDeployAgentDialog(project, agents);
      }

      // Auto-scan for unmanaged skills
      try {
        const result = await scanProjectSkills(project.path);
        const unmanaged = filterUnmanagedByEnabledProfiles(result.skills);
        setUnmanagedSkills(unmanaged);
        if (unmanaged.length > 0) setScanExpanded(true);
      } catch (e) {
        console.error("Scan failed:", e);
      }
    },
    [
      filterAgentsByEnabledProfiles,
      filterUnmanagedByEnabledProfiles,
      loadProjectSkills,
      openDeployAgentDialog,
      pendingGroupSkills,
      presentProjectState,
      scanProjectSkills,
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
        mergePendingSkillsIntoAgents(prev, allowedAgentIds, pendingGroupSkills)
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
          // Turning ON → add agent with empty skills, auto-expand (exclusive)
          next[agentId] = [];
          setExpandedAgent(agentId);
    
        }
        return next;
      });
      setDirty(true);
    },
    [enabledProfileIdSet, expandedAgent]
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
      const targets = unmanagedSkills.map((s) => ({
        name: s.name,
        agent_id: s.agent_id,
      }));
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
        setAgentSkills(filterAgentsByEnabledProfiles({ ...skills.agents }));
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
    scanProjectSkills,
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
    const q = projectFilter.toLowerCase();
    return projects.filter(
      (p) => p.name.toLowerCase().includes(q) || p.path.toLowerCase().includes(q)
    );
  }, [projects, projectFilter]);



  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="h-14 flex items-center justify-between px-6 border-b border-border bg-card/30 backdrop-blur-sm">
        <div className="flex items-center gap-3">
          <h1 className="text-heading-md text-zinc-100">{t("projects.title")}</h1>
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
            onNavigateToSkill={onNavigateToSkill}
            onRemoveSkill={handleRemoveSkill}
            onSkillFilterChange={setSkillFilter}
            onAddSkill={handleAddSkill}
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
    </div>
  );
}

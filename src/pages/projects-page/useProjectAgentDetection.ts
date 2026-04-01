import { useState, useCallback } from "react";
import type { AmbiguousGroup, DetectedAgent } from "../../types";

interface UseProjectAgentDetectionParams {
  enabledProfileIdSet: Set<string>;
  pathByAgentId: Map<string, string>;
  canonicalizeAgentsBySharedPath: (
    agents: Record<string, string[]>,
    forcedOwnerByPath?: Map<string, string>
  ) => Record<string, string[]>;
  detectProjectAgents: (projectPath: string) => Promise<import("../../types").ProjectAgentDetection>;
  saveProjectSkillsList: (
    projectPath: string,
    agents: Record<string, string[]>
  ) => Promise<import("../../types").SkillsList>;
  filterAgentsByEnabledProfiles: (
    agents: Record<string, string[]>
  ) => Record<string, string[]>;
  loadProjects: () => Promise<void>;
  /** Called to propagate agentSkills changes back to the host component */
  onAgentSkillsChange: (updater: (prev: Record<string, string[]>) => Record<string, string[]>) => void;
  /** Called when dirty state should be set */
  onDirtyChange: (dirty: boolean) => void;
  /** Called to propagate saving state */
  onSavingChange: (saving: boolean) => void;
  /** Called to propagate expandedAgent changes */
  onExpandedAgentChange: (agentId: string | null) => void;
}

export function useProjectAgentDetection({
  enabledProfileIdSet,
  pathByAgentId,
  canonicalizeAgentsBySharedPath,
  detectProjectAgents,
  saveProjectSkillsList,
  filterAgentsByEnabledProfiles,
  loadProjects,
  onAgentSkillsChange,
  onDirtyChange,
  onSavingChange,
  onExpandedAgentChange,
}: UseProjectAgentDetectionParams) {
  const [disambigOpen, setDisambigOpen] = useState(false);
  const [disambigGroup, setDisambigGroup] = useState<AmbiguousGroup | null>(null);
  const [disambigCandidates, setDisambigCandidates] = useState<DetectedAgent[]>([]);
  const [disambigQueue, setDisambigQueue] = useState<AmbiguousGroup[]>([]);
  const [scannedSymlinkSkillsByAgent, setScannedSymlinkSkillsByAgent] = useState<
    Record<string, string[]>
  >({});

  const resetDisambigState = useCallback(() => {
    setDisambigOpen(false);
    setDisambigGroup(null);
    setDisambigCandidates([]);
    setDisambigQueue([]);
  }, []);

  const runAgentDetection = useCallback(
    async (
      projectPath: string,
      currentAgents: Record<string, string[]>,
      symlinkSkillsByAgent: Record<string, string[]>,
      _selectedProject: { path: string } | null,
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
          onAgentSkillsChange((prev) => {
            const next = { ...prev };
            for (const agentId of autoEnable) {
              const current = next[agentId] ?? [];
              const scanned = symlinkSkillsByAgent[agentId] ?? [];
              const merged = [...new Set([...current, ...scanned])];
              next[agentId] = merged;
            }
            return canonicalizeAgentsBySharedPath(next);
          });
          onDirtyChange(true);
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
    [detectProjectAgents, enabledProfileIdSet, canonicalizeAgentsBySharedPath, onAgentSkillsChange, onDirtyChange]
  );

  const handleDisambigConfirm = useCallback(
    async (selectedAgentId: string, agentSkills: Record<string, string[]>, selectedProject: { path: string } | null) => {
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
      onAgentSkillsChange(() => nextAgents);
      onExpandedAgentChange(selectedAgentId);
      onDirtyChange(true);

      // "Confirm" in disambiguation should persist immediately, but avoid
      // destructive full-sync behavior for shared-path resolution.
      if (selectedProject) {
        onSavingChange(true);
        try {
          await saveProjectSkillsList(
            selectedProject.path,
            filterAgentsByEnabledProfiles(nextAgents)
          );
          onDirtyChange(false);
          loadProjects();
        } catch (e) {
          console.error("Auto-persist after disambiguation failed:", e);
        } finally {
          onSavingChange(false);
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
      enabledProfileIdSet,
      disambigGroup,
      disambigQueue,
      scannedSymlinkSkillsByAgent,
      saveProjectSkillsList,
      filterAgentsByEnabledProfiles,
      canonicalizeAgentsBySharedPath,
      pathByAgentId,
      loadProjects,
      onAgentSkillsChange,
      onDirtyChange,
      onExpandedAgentChange,
      onSavingChange,
    ]
  );

  const handleDisambigClose = useCallback(() => {
    setDisambigOpen(false);
    setDisambigGroup(null);
    setDisambigQueue([]);
    setDisambigCandidates([]);
  }, []);

  return {
    disambigOpen,
    disambigGroup,
    disambigCandidates,
    setScannedSymlinkSkillsByAgent,
    resetDisambigState,
    runAgentDetection,
    handleDisambigConfirm,
    handleDisambigClose,
  };
}

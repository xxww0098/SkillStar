import { useCallback, useMemo } from "react";
import { supportsProjectDeploy } from "../../../lib/agentProfiles";
import type { AgentProfile, ScannedSkill } from "../../../types";

export interface ProjectDeployAgents {
  /** Profiles that are enabled AND support project deploy (non-empty `project_skills_rel`). */
  enabledProfiles: AgentProfile[];
  enabledProfileIdSet: Set<string>;
  enabledProfilesById: Map<string, AgentProfile>;
  /** agentId → its on-disk `skills/` relative path (`project_skills_rel`, falling back to id). */
  pathByAgentId: Map<string, string>;
  /** agentId → the set of agent ids that share its path (only for paths with >1 agent). */
  conflictAgentIdsByAgent: Map<string, Set<string>>;
  /** Collapse a per-agent skills map so each shared path has a single owner agent. */
  canonicalizeAgentsBySharedPath: (
    agents: Record<string, string[]>,
    forcedOwnerByPath?: Map<string, string>,
  ) => Record<string, string[]>;
  filterAgentsByEnabledProfiles: (agents: Record<string, string[]>) => Record<string, string[]>;
  filterUnmanagedByEnabledProfiles: (skills: ScannedSkill[]) => ScannedSkill[];
  buildSymlinkSkillIndex: (skills: ScannedSkill[]) => Record<string, string[]>;
}

/**
 * Derives the project-deploy-eligible agent profiles plus the shared-path
 * canonicalization helpers `Projects.tsx` relies on.
 *
 * Several agents can target the same on-disk `skills/` directory (their
 * `project_skills_rel` collides — e.g. tools that both read `~/.config/...`).
 * These helpers keep that mapping **single-owner** so deploy/scan state never
 * double-counts a shared directory.
 */
export function useProjectDeployAgents(profiles: AgentProfile[]): ProjectDeployAgents {
  const enabledProfiles = useMemo(
    () => profiles.filter((profile) => profile.enabled && supportsProjectDeploy(profile)),
    [profiles],
  );
  const enabledProfileIdSet = useMemo(() => new Set(enabledProfiles.map((profile) => profile.id)), [enabledProfiles]);
  const enabledProfilesById = useMemo(
    () => new Map(enabledProfiles.map((profile) => [profile.id, profile])),
    [enabledProfiles],
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
    (agents: Record<string, string[]>, forcedOwnerByPath?: Map<string, string>): Record<string, string[]> => {
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
        const owner = forcedOwner && activeIds.includes(forcedOwner) ? forcedOwner : activeIds[0];

        next[owner] = [...new Set(activeIds.flatMap((id) => inputByAgent.get(id) ?? []))];
      }
      return next;
    },
    [agentIdsByPath, enabledProfileIdSet],
  );

  const filterAgentsByEnabledProfiles = useCallback(
    (agents: Record<string, string[]>) =>
      Object.fromEntries(Object.entries(agents).filter(([agentId]) => enabledProfileIdSet.has(agentId))),
    [enabledProfileIdSet],
  );

  const filterUnmanagedByEnabledProfiles = useCallback(
    (skills: ScannedSkill[]) => {
      const deduped = new Map<string, ScannedSkill>();
      for (const skill of skills) {
        // `managed` is computed by the backend scan against skills-list.json,
        // so copy-mode deployments are correctly excluded here too.
        if (!enabledProfileIdSet.has(skill.agent_id) || skill.is_symlink || !skill.has_skill_md || skill.managed) {
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
    [enabledProfileIdSet, pathByAgentId],
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
    [enabledProfileIdSet],
  );

  return {
    enabledProfiles,
    enabledProfileIdSet,
    enabledProfilesById,
    pathByAgentId,
    conflictAgentIdsByAgent,
    canonicalizeAgentsBySharedPath,
    filterAgentsByEnabledProfiles,
    filterUnmanagedByEnabledProfiles,
    buildSymlinkSkillIndex,
  };
}

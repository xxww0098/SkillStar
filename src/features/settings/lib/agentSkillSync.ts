import type { AgentManagedSkillsState } from "../../../lib/ipc/commands/agents";

export type AgentSkillPauseStatus = "loading" | "empty" | "active" | "paused" | "partial";
export type AgentSkillPauseAction = "pause" | "restore";

export interface AgentSkillPauseSnapshot {
  status: AgentSkillPauseStatus;
  activeSkillNames: string[];
  suspendedSkillNames: string[];
  action: AgentSkillPauseAction | null;
  checked: boolean;
}

function uniqueSkillNames(names: readonly string[]): string[] {
  const seen = new Set<string>();
  const unique: string[] = [];

  for (const rawName of names) {
    const name = rawName.trim();
    if (!name || seen.has(name)) continue;
    seen.add(name);
    unique.push(name);
  }

  return unique;
}

/**
 * Derive the compact Settings control from one directory's backend-owned
 * state. The suspended set is an exact recovery journal, never a Hub inventory.
 */
export function getAgentSkillPauseSnapshot(state: AgentManagedSkillsState | undefined): AgentSkillPauseSnapshot {
  if (!state) {
    return {
      status: "loading",
      activeSkillNames: [],
      suspendedSkillNames: [],
      action: null,
      checked: false,
    };
  }

  const activeSkillNames = uniqueSkillNames(state.active_skill_names);
  const suspendedSkillNames = uniqueSkillNames(state.suspended_skill_names);
  const hasActive = activeSkillNames.length > 0;
  const hasSuspended = suspendedSkillNames.length > 0;

  const status: AgentSkillPauseStatus = hasSuspended
    ? hasActive
      ? "partial"
      : "paused"
    : hasActive
      ? "active"
      : "empty";

  return {
    status,
    activeSkillNames,
    suspendedSkillNames,
    action: hasSuspended ? "restore" : hasActive ? "pause" : null,
    // A mixed state has a recovery action, so do not imply that its remaining
    // active links are a complete enabled set.
    checked: status === "active",
  };
}

/** Use one key for equivalent path spellings within the current UI snapshot. */
export function globalSkillsTargetKey(path: string): string {
  return path.trim().replace(/\\/g, "/").replace(/\/+$/, "");
}

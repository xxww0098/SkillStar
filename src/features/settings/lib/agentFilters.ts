import type { AgentProfile } from "../../../types";

/**
 * Narrowing offered by the Settings agent list. The registry ships 70+ built-in
 * Agents, so the list is unusable without text search plus an activation filter.
 */
export type AgentStatusFilter = "all" | "enabled" | "disabled";

export const AGENT_STATUS_FILTERS: readonly AgentStatusFilter[] = ["all", "enabled", "disabled"];

export interface AgentFilterState {
  query: string;
  status: AgentStatusFilter;
}

export const EMPTY_AGENT_FILTER: AgentFilterState = { query: "", status: "all" };

/** Counts per status for the segmented control, computed after text search. */
export type AgentStatusCounts = Record<AgentStatusFilter, number>;

function normalizeQuery(query: string): string {
  return query.trim().toLowerCase();
}

/**
 * Matches on display name and id so both `Claude Code` and `claude` find the
 * same row — users type whichever one they remember from the CLI.
 */
export function searchAgentProfiles(profiles: readonly AgentProfile[], query: string): AgentProfile[] {
  const normalized = normalizeQuery(query);
  if (!normalized) return [...profiles];

  return profiles.filter(
    (profile) =>
      profile.display_name.toLowerCase().includes(normalized) || profile.id.toLowerCase().includes(normalized),
  );
}

export function filterAgentProfilesByStatus(
  profiles: readonly AgentProfile[],
  status: AgentStatusFilter,
): AgentProfile[] {
  if (status === "all") return [...profiles];
  const wantEnabled = status === "enabled";
  return profiles.filter((profile) => profile.enabled === wantEnabled);
}

export function countAgentStatuses(profiles: readonly AgentProfile[]): AgentStatusCounts {
  let enabled = 0;
  for (const profile of profiles) {
    if (profile.enabled) enabled += 1;
  }
  return { all: profiles.length, enabled, disabled: profiles.length - enabled };
}

/** An active filter turns an empty result into "no matches", not "no agents". */
export function isAgentFilterActive(filter: AgentFilterState): boolean {
  return normalizeQuery(filter.query).length > 0 || filter.status !== "all";
}

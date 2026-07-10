import type { AgentProfile } from "../types";

/**
 * Whether Settings currently exposes an agent as a valid local target.
 *
 * `enabled` is a persisted user preference and can remain true after the
 * underlying agent is uninstalled, so neither flag is sufficient alone.
 */
export function isTargetableAgentProfile(profile: AgentProfile): boolean {
  return profile.installed && profile.enabled;
}

/** Keep Settings order while removing agents that cannot receive local work. */
export function selectTargetableAgentProfiles(profiles: readonly AgentProfile[]): AgentProfile[] {
  return profiles.filter(isTargetableAgentProfile);
}

/**
 * Whether an agent can receive project-level skill deploys. Global-only
 * agents (e.g. OpenClaw) are expressed in the builtin data table with an
 * empty `project_skills_rel` — keep this check data-driven so new
 * global-only agents never need frontend edits.
 */
export function supportsProjectDeploy(profile: AgentProfile): boolean {
  return profile.project_skills_rel.trim().length > 0;
}

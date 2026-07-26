import type { AgentProfile } from "../types";

/**
 * Whether the user has explicitly activated an Agent in Settings.
 * Host installation is deliberately not inferred from binaries or paths.
 */
export function isTargetableAgentProfile(profile: Pick<AgentProfile, "enabled">): boolean {
  return profile.enabled;
}

/** Keep Settings order while removing Agents the user has not activated. */
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

/** Whether an Agent exposes a user-level skills directory. */
export function supportsGlobalDeploy(profile: AgentProfile): boolean {
  return profile.global_skills_dir.trim().length > 0;
}

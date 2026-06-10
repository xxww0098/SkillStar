import type { AgentProfile } from "../types";

/**
 * Whether an agent can receive project-level skill deploys. Global-only
 * agents (e.g. OpenClaw) are expressed in the builtin data table with an
 * empty `project_skills_rel` — keep this check data-driven so new
 * global-only agents never need frontend edits.
 */
export function supportsProjectDeploy(profile: AgentProfile): boolean {
  return profile.project_skills_rel.trim().length > 0;
}

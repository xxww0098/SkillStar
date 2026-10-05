/**
 * Mutable browser-dev store for the Skill *update* flow.
 *
 * The static sample list alone cannot exercise it: it re-asserted
 * `update_available` on every refresh and no command ever reported a
 * skipped (upstream-removed) name, so the skip toast and the batch update
 * summary were unreachable outside the Tauri shell.
 *
 * This store keeps a per-session copy of the installed Skills and mirrors the
 * backend contract: an update clears the badge and hands back the refreshed
 * Skill, a Skill its upstream removed is reported as `skipped` (it is not
 * pulled), and an unknown name fails by name. Two Skills start out removed
 * upstream so the removed chip and the skip toast have something to show.
 *
 * DEV ONLY — reachable only from ./index.ts, which ../core.ts imports behind
 * `import.meta.env.DEV`. Shared by the skills and github fragments because
 * reinstalling a repository source spans both command domains.
 */

import type { Skill, SkillUpdateReport, SkillUpdateState, UpdateResult } from "../../../types";
import { iso } from "./shared";
import { SAMPLE_SKILLS } from "./skillsData";

let skills: Skill[] = SAMPLE_SKILLS.map((skill) => ({ ...skill }));

function isUpstreamRemoved(skill: Skill): boolean {
  return skill.skill_type !== "local" && skill.upstream_change?.kind === "removed";
}

export function devListSkills(): Skill[] {
  return skills;
}

export function devSkillUpdateStates(): SkillUpdateState[] {
  return skills
    .filter((skill) => skill.skill_type !== "local")
    .map((skill) => ({
      name: skill.name,
      update_available: skill.update_available,
      upstream_change: skill.upstream_change ?? null,
    }));
}

/** Pull one Skill: the badge clears and the refreshed Skill is handed back.
 *  A Skill its upstream removed is skipped, exactly as the backend does. */
export function devUpdateSkills(names: string[]): SkillUpdateReport {
  const report: SkillUpdateReport = {
    updated: [],
    failed: [],
    skipped: [],
    channel_managed: [],
  };

  for (const name of names) {
    const skill = skills.find((candidate) => candidate.name === name);
    if (!skill) {
      report.failed.push({ name, error: `Skill '${name}' is not installed` });
      continue;
    }
    if (isUpstreamRemoved(skill)) {
      report.skipped.push(name);
      continue;
    }
    const fresh: Skill = { ...skill, update_available: false, last_updated: iso(0) };
    skills = skills.map((candidate) => (candidate.name === name ? fresh : candidate));
    const result: UpdateResult = { skill: fresh, siblings_cleared: [], agent_link_failures: [] };
    report.updated.push(result);
  }

  return report;
}

/** Repo sources are reinstalled wholesale; the store keeps the member list. */
export function devRepoSourceSkills(source: string): Skill[] {
  return skills.filter((skill) => skill.source === source && skill.skill_type !== "local");
}

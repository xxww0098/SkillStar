/**
 * Mutable browser-dev store for the Skill *update* flows.
 *
 * The static sample list alone cannot exercise them: it re-asserted
 * `update_available` on every refresh and no command ever answered with a
 * blocked local divergence, so the divergence dialog, the batch resolve loop
 * and the repo reinstall action were all unreachable outside the Tauri shell.
 *
 * This store keeps a per-session copy of the installed Skills and mirrors the
 * backend contract closely enough to click the whole flow through:
 * `crates/skillstar-skills/src/skill_update` blocks a whole checkout when any
 * Skill inside it diverges, reports the other requested names of a pulled
 * checkout as `skipped`, and answers a resolution with the checkout's still
 * divergent members. Two Skills start out "locally edited" (with two different
 * reasons) so the batch dialog has something to show, and two more start out
 * dropped by their source — one with content left to keep, one without — so
 * the second dialog and its removal exit are reachable too.
 *
 * DEV ONLY — reachable only from ./index.ts, which ../core.ts imports behind
 * `import.meta.env.DEV`. Shared by the skills and github fragments because
 * reinstalling a repository source spans both command domains.
 */

import type {
  LocalDivergenceReason,
  LocalDivergenceResolution,
  ResolveSkillUpdateResult,
  Skill,
  SkillUpdateBlocked,
  SkillUpdateReport,
  SkillUpdateState,
  UpdateResult,
} from "../../../types";
import { iso } from "./shared";
import { SAMPLE_SKILLS } from "./skillsData";

/** Installed Skills the backend would stop an update on, and why. */
const INITIAL_DIVERGENCES: Array<[string, LocalDivergenceReason]> = [
  ["pdf-tools", "content_changed"],
  ["xlsx", "baseline_missing"],
  // Dropped by their source: the first can still be kept as a local copy, the
  // second has nothing left on disk and can only be removed.
  ["deep-research", "source_removed"],
  ["svg2icon", "source_missing"],
];

function isSourceGoneReason(reason: LocalDivergenceReason): boolean {
  return reason === "source_removed" || reason === "source_missing";
}

let skills: Skill[] = SAMPLE_SKILLS.map((skill) => ({ ...skill }));
const divergences = new Map<string, LocalDivergenceReason>(INITIAL_DIVERGENCES);

/** Skills sharing one physical checkout move together, exactly as in the hub. */
function checkoutKey(skill: Skill): string {
  return skill.skill_type === "local" || !skill.git_url ? `standalone:${skill.name}` : skill.git_url;
}

function checkoutMembers(key: string): Skill[] {
  return skills.filter((skill) => checkoutKey(skill) === key);
}

function blockedIn(key: string): SkillUpdateBlocked[] {
  return checkoutMembers(key)
    .filter((skill) => divergences.has(skill.name))
    .map((skill) => {
      const reason = divergences.get(skill.name) ?? "content_changed";
      return {
        name: skill.name,
        reason,
        suggested_local_name: suggestedLocalName(skill.name),
        // Only an unexpected failure carries a message; `reason` speaks for itself.
        error: reason === "snapshot_failed" ? "Skill content could not be read" : null,
      };
    });
}

function suggestedLocalName(name: string): string {
  const base = `${name}.local`;
  for (let suffix = 1; ; suffix += 1) {
    const candidate = suffix === 1 ? base : `${base}.${suffix}`;
    if (!skills.some((skill) => skill.name === candidate)) return candidate;
  }
}

/** Pull one checkout: every member loses its badge, the requested one is returned. */
function pullCheckout(name: string, alsoRequested: string[]): UpdateResult {
  const target = requireSkill(name);
  const key = checkoutKey(target);
  const siblings = checkoutMembers(key)
    .filter((skill) => skill.name !== name && !alsoRequested.includes(skill.name))
    .map((skill) => skill.name);

  skills = skills.map((skill) =>
    checkoutKey(skill) === key ? { ...skill, update_available: false, last_updated: iso(0) } : skill,
  );

  return {
    skill: requireSkill(name),
    siblings_cleared: siblings,
    agent_link_failures: [],
  };
}

function requireSkill(name: string): Skill {
  const skill = skills.find((candidate) => candidate.name === name);
  if (!skill) throw new Error(`Skill '${name}' is not installed`);
  return skill;
}

export function devListSkills(): Skill[] {
  return skills;
}

export function devSkillUpdateStates(): SkillUpdateState[] {
  return skills
    .filter((skill) => skill.skill_type !== "local")
    .map((skill) => ({ name: skill.name, update_available: skill.update_available }));
}

export function devUpdateSkills(names: string[]): SkillUpdateReport {
  const report: SkillUpdateReport = { updated: [], blocked: [], failed: [], skipped: [] };

  const requestedByCheckout = new Map<string, string[]>();
  for (const name of names) {
    const skill = skills.find((candidate) => candidate.name === name);
    if (!skill) {
      report.failed.push({ name, error: `Skill '${name}' is not installed` });
      continue;
    }
    const key = checkoutKey(skill);
    requestedByCheckout.set(key, [...(requestedByCheckout.get(key) ?? []), name]);
  }

  for (const [key, requested] of requestedByCheckout) {
    const blocked = blockedIn(key);
    if (blocked.length > 0) {
      report.blocked.push(...blocked);
      continue;
    }
    const [representative, ...covered] = requested;
    report.updated.push(pullCheckout(representative, covered));
    report.skipped.push(...covered);
  }

  return report;
}

export function devResolveSkillUpdate(name: string, resolution: LocalDivergenceResolution): ResolveSkillUpdateResult {
  const skill = requireSkill(name);
  const key = checkoutKey(skill);
  const reason = divergences.get(name);
  const sourceGone = reason !== undefined && isSourceGoneReason(reason);

  if (sourceGone && resolution.kind === "discard") {
    throw new Error(`Skill '${name}' was dropped by its source; discarding local changes cannot bring it back`);
  }
  if (!sourceGone && resolution.kind === "uninstall") {
    throw new Error(`Skill '${name}' still comes from its source; keep or discard the local changes instead`);
  }
  if (reason === "source_missing" && resolution.kind === "preserve") {
    throw new Error(`Skill '${name}' was dropped by its source and its content is already gone`);
  }

  let localCopy: Skill | null = null;
  if (resolution.kind === "preserve") {
    const localName = resolution.local_name.trim();
    if (!localName) throw new Error("A local copy name is required");
    if (skills.some((candidate) => candidate.name === localName)) {
      throw new Error(`A Skill named '${localName}' already exists`);
    }
    localCopy = {
      ...skill,
      name: localName,
      skill_type: "local",
      stars: 0,
      category: "None",
      update_available: false,
      git_url: "",
      tree_hash: null,
      source: undefined,
      last_updated: iso(0),
    };
    skills = [...skills, localCopy];
  }

  divergences.delete(name);

  // Both exits for a dropped Skill end in removal — keeping a copy only decides
  // whether its content survives as an independent local Skill first.
  const uninstalled: string[] = [];
  if (sourceGone) {
    skills = skills.filter((candidate) => candidate.name !== name);
    uninstalled.push(name);
  }

  const remainingBlocked = blockedIn(key);
  if (remainingBlocked.length > 0) {
    return { update: null, local_copy: localCopy, uninstalled, remaining_blocked: remainingBlocked };
  }
  // A removed Skill cannot pull its own checkout; a survivor of the same
  // checkout does it instead, and a checkout with none simply has nothing left.
  const representative = sourceGone ? checkoutMembers(key)[0]?.name : name;
  return {
    update: representative ? pullCheckout(representative, []) : null,
    local_copy: localCopy,
    uninstalled,
    remaining_blocked: [],
  };
}

/** Repo sources are reinstalled wholesale; a dirty checkout fails closed. */
export function devRepoSourceSkills(source: string): Skill[] {
  return skills.filter((skill) => skill.source === source && skill.skill_type !== "local");
}

export function devAssertRepoSourceClean(source: string): void {
  const dirty = devRepoSourceSkills(source).filter((skill) => divergences.has(skill.name));
  if (dirty.length > 0) {
    throw new Error(
      `Cached repository has local Skill changes: ${dirty.map((skill) => skill.name).join(", ")}. ` +
        "Preserve or discard them before reinstalling.",
    );
  }
}

/**
 * Validation for the local-copy names typed in the divergence dialog.
 *
 * Resolving a blocked update is a *sequence* of backend calls, one per Skill.
 * A name the backend rejects therefore fails partway through: earlier Skills
 * are already preserved and updated while the rest stay blocked. Checking the
 * names before the first call is what keeps that batch atomic from the user's
 * point of view, so these rules mirror the backend's own
 * (`content::validate_skill_name` in crates/skillstar-skills) instead of being
 * a looser cosmetic check.
 */

import type { SkillUpdateBlocked } from "../../../types";

export type LocalCopyNameIssue = "required" | "invalid" | "duplicate" | "taken";

/** Characters the hub refuses in a Skill directory name, on every platform. */
// biome-ignore lint/suspicious/noControlCharactersInRegex: the hub rejects control characters in directory names.
const FORBIDDEN_CHARACTERS = /[/\\<>:"|?*\u0000-\u001f\u007f]/;
const WINDOWS_RESERVED = /^(con|prn|aux|nul|clock\$|com[1-9]|lpt[1-9])$/i;

/** Mirror of the backend rule — a name the hub can hold as its own directory. */
export function isValidSkillFolderName(name: string): boolean {
  if (!name || name === "." || name === "..") return false;
  if (FORBIDDEN_CHARACTERS.test(name)) return false;
  if (name.endsWith(".") || name.endsWith(" ")) return false;
  return !WINDOWS_RESERVED.test(name.split(".")[0]);
}

/**
 * Issues keyed by the *blocked* Skill name (not by the copy name).
 * `takenNames` is the library as it stands now: a copy may not land on a name
 * some Skill already occupies — including the blocked Skill itself, whose
 * directory stays in place and is updated from its source.
 */
export function validateLocalCopyNames(
  blocked: SkillUpdateBlocked[],
  localNames: Record<string, string>,
  takenNames: Iterable<string>,
): Record<string, LocalCopyNameIssue> {
  const taken = new Set(Array.from(takenNames, (name) => name.toLowerCase()));
  const seen = new Set<string>();
  const issues: Record<string, LocalCopyNameIssue> = {};

  for (const item of blocked) {
    const name = (localNames[item.name] ?? "").trim();
    if (!name) {
      issues[item.name] = "required";
      continue;
    }
    if (!isValidSkillFolderName(name)) {
      issues[item.name] = "invalid";
      continue;
    }
    const key = name.toLowerCase();
    if (taken.has(key)) {
      issues[item.name] = "taken";
      continue;
    }
    if (seen.has(key)) {
      issues[item.name] = "duplicate";
      continue;
    }
    seen.add(key);
  }

  return issues;
}
